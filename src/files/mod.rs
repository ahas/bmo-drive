//! The file viewer (`docs/files.md`): this machine's drives, its Garage and the nodes' volumes, the
//! administrator's alone, and each owner's own BMO drive, each a place a caller walks and changes as a file
//! explorer does, and the routes that read them (`docs/api.md`, 「Places」 and 「Entries」).

mod change;
mod drives;
mod garage;
mod guard;
mod node;
mod order;
pub mod shares;
mod space;
mod volume;
mod zip;

use std::cmp::Ordering;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::Response;
use futures_util::stream::BoxStream;
use http_range_header::{EndPosition, StartPosition};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

use crate::http::{Call, Refused, Route, Standing, answer};
use crate::owner::Owner;
pub(crate) use crate::wire::EntryKind as Kind;
use crate::wire::{
  self, ContentQuery, EntriesQuery, EntryPage, EntrySort, ListedNode, MachineQuery, NodePage, NodesQuery,
  PlacePage, PlacesQuery, SortOrder,
};
use drives::Drive;
use garage::Garage;
use guard::Guard;
use space::{Room, SPACE, Space, kept_name, space_place};

/// What one of a place's answers is still waiting on.
type Pending<'a, T> = Pin<Box<dyn Future<Output = Result<T, Refused>> + Send + 'a>>;

/// A file arriving to be written: its bytes as they stream in, and how many there are to be.
pub struct Upload {
  pub body: BoxStream<'static, io::Result<Bytes>>,
  pub length: u64,
}

/// One root the viewer walks under, and nothing above it. Each kind answers in its own way what stands
/// in a folder and what a file holds, and makes each change a file explorer makes. A change is given
/// paths already fenced by name, settles no conflict — the caller has — and refuses a name already
/// taken.
trait Place: Send + Sync {
  fn id(&self) -> &str;
  /// Every entry of the folder at `within`, read whole.
  fn read_folder<'a>(&'a self, within: &'a Within) -> Pending<'a, Vec<Entry>>;
  /// The file at `within`, opened to be streamed: the bytes `span` names of it.
  fn open<'a>(&'a self, within: &'a Within, span: Span) -> Pending<'a, Opened>;
  /// A new folder at `within`.
  fn make_folder<'a>(&'a self, within: &'a Within) -> Pending<'a, ()>;
  /// A file at `within` written from `upload`, in place only once all of it has arrived.
  fn write<'a>(&'a self, within: &'a Within, upload: Upload) -> Pending<'a, ()>;
  /// The entry at `within` given `name` in its own folder.
  fn rename<'a>(&'a self, within: &'a Within, name: &'a str) -> Pending<'a, ()>;
  /// The entry at `within` taken away, a folder with everything in it.
  fn remove<'a>(&'a self, within: &'a Within) -> Pending<'a, ()>;
  /// The entry at `from` copied, or moved, to `to` inside this same place, by the place itself.
  fn copy_here<'a>(&'a self, from: &'a Within, to: &'a Within, moving: bool) -> Pending<'a, ()>;
  /// This place cut down to the folder at `top`, which becomes its top: the fence stands there, so
  /// nothing above or beside the folder is reached through it, a link leading out of it included.
  fn beneath<'a>(&'a self, top: &'a Within) -> Pending<'a, Box<dyn Place + 'a>>;
}

/// A place reached through a folder of another by its path alone, for a place where a path can lead
/// nowhere but where it names — object storage, whose prefixes hold no links.
struct Prefixed<'a> {
  place: &'a dyn Place,
  top: Within,
}

impl Place for Prefixed<'_> {
  fn id(&self) -> &str {
    self.place.id()
  }

  fn read_folder<'a>(&'a self, within: &'a Within) -> Pending<'a, Vec<Entry>> {
    Box::pin(async move { self.place.read_folder(&within.under(&self.top)).await })
  }

  fn open<'a>(&'a self, within: &'a Within, span: Span) -> Pending<'a, Opened> {
    Box::pin(async move { self.place.open(&within.under(&self.top), span).await })
  }

  fn make_folder<'a>(&'a self, within: &'a Within) -> Pending<'a, ()> {
    Box::pin(async move { self.place.make_folder(&within.under(&self.top)).await })
  }

  fn write<'a>(&'a self, within: &'a Within, upload: Upload) -> Pending<'a, ()> {
    Box::pin(async move { self.place.write(&within.under(&self.top), upload).await })
  }

  fn rename<'a>(&'a self, within: &'a Within, name: &'a str) -> Pending<'a, ()> {
    Box::pin(async move { self.place.rename(&within.under(&self.top), name).await })
  }

  fn remove<'a>(&'a self, within: &'a Within) -> Pending<'a, ()> {
    Box::pin(async move { self.place.remove(&within.under(&self.top)).await })
  }

  fn copy_here<'a>(&'a self, from: &'a Within, to: &'a Within, moving: bool) -> Pending<'a, ()> {
    Box::pin(
      async move { self.place.copy_here(&from.under(&self.top), &to.under(&self.top), moving).await },
    )
  }

  fn beneath<'a>(&'a self, top: &'a Within) -> Pending<'a, Box<dyn Place + 'a>> {
    Box::pin(async move {
      Ok(Box::new(Prefixed {
        place: self.place,
        top: top.under(&self.top),
      }) as Box<dyn Place + 'a>)
    })
  }
}

/// A machine's places as one reading of it lists them, or an owner's space.
trait Places: Send + Sync {
  /// The place a request names, where the reading lists it; anything else is no place.
  fn place(&self, id: &str) -> Result<&dyn Place, Refused>;

  /// What a place with a quota still takes; none where bytes arrive with no quota judged.
  fn room(&self) -> Option<&Room> {
    None
  }
}

/// A path inside a place: names that each stay where they are, the empty list being the place's top.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Within {
  names: Vec<String>,
}

impl Within {
  /// The names of a path joined by `/`. A name that would climb out or stand for nothing is refused
  /// here, before any place is asked.
  fn read(path: &str) -> Result<Self, Refused> {
    if path.is_empty() {
      return Ok(Self::default());
    }

    let names: Vec<String> = path.split('/').map(str::to_owned).collect();

    if names.iter().any(|name| !fair_name(name)) {
      return Err(outside());
    }

    Ok(Self { names })
  }

  fn joined(&self) -> String {
    self.names.join("/")
  }

  fn last(&self) -> Option<&str> {
    self.names.last().map(String::as_str)
  }

  /// The folder a path stands in and its own name; none at a place's top.
  fn split(&self) -> Option<(Self, &str)> {
    let (last, folder) = self.names.split_last()?;

    Some((
      Self {
        names: folder.to_vec(),
      },
      last,
    ))
  }

  /// The entry `name` inside this folder.
  fn child(&self, name: &str) -> Self {
    let mut names = self.names.clone();

    names.push(name.to_owned());
    Self { names }
  }

  /// This path read from inside the folder at `top`.
  fn under(&self, top: &Self) -> Self {
    Self {
      names: top.names.iter().chain(&self.names).cloned().collect(),
    }
  }

  /// The entry `name` beside this one, in the same folder.
  fn sibling(&self, name: &str) -> Self {
    let mut names = self.names.clone();

    names.pop();
    names.push(name.to_owned());
    Self { names }
  }
}

/// Whether a name may stand in a path at all: never one that climbs out or stands for nothing.
fn fair_name(name: &str) -> bool {
  !(name.is_empty() || name == "." || name == ".." || name.contains(['/', '\0']))
}

/// The code of a path that would leave its place.
const OUTSIDE_PLACE: &str = "outside-place";

fn outside() -> Refused {
  Refused::rule(OUTSIDE_PLACE, "the path would leave its place")
}

/// The code of a path walking back into a folder it already stood in.
const LINK_LOOP: &str = "link-loop";

fn looped() -> Refused {
  Refused::rule(LINK_LOOP, "the path walks back into a folder it already stood in")
}

fn taken() -> Refused {
  Refused::conflict("the name is already taken")
}

fn protected() -> Refused {
  Refused::rule("protected", "a protected location cannot be changed")
}

fn into_itself() -> Refused {
  Refused::rule("into-itself", "a folder cannot be copied or moved into itself")
}

fn cut_short() -> Refused {
  Refused::bad_request("the file did not arrive whole")
}

fn not_found() -> Refused {
  Refused::not_found("nothing stands at that path")
}

fn no_place() -> Refused {
  Refused::not_found("no such place")
}

fn is_folder() -> Refused {
  Refused::rule("is-folder", "a folder cannot be read as a file")
}

fn not_folder() -> Refused {
  Refused::rule("not-folder", "it is not a folder")
}

fn place_top() -> Refused {
  Refused::rule("place-top", "the top of a place cannot be changed")
}

/// The name a file is written under beside its final one until all of it has arrived.
fn scratch_name() -> String {
  format!(".bmo-upload-{:016x}", rand::random::<u64>())
}

/// What a refused reading or change of a file system says, by the kind of what went wrong.
fn io_kind_refusal(kind: io::ErrorKind) -> Refused {
  match kind {
    io::ErrorKind::NotFound => not_found(),
    io::ErrorKind::PermissionDenied => {
      Refused::rule("no-permission", "the account may not read or change it")
    }
    io::ErrorKind::AlreadyExists => taken(),
    other => Refused::rule("failed-io", format!("it could not be done: {other}")),
  }
}

/// What a refused reading of the file system says, by what went wrong.
fn io_refusal(error: &io::Error) -> Refused {
  match error.kind() {
    io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied | io::ErrorKind::AlreadyExists => {
      io_kind_refusal(error.kind())
    }
    _ => Refused::rule("failed-io", format!("it could not be done: {error}")),
  }
}

/// One entry of a folder, as a place reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
  name: String,
  kind: Kind,
  /// A file's length in bytes; a folder has none.
  size: Option<u64>,
  /// When it last changed, in epoch milliseconds.
  modified: Option<i64>,
  /// A link's target as the link writes it; none for anything that is no link.
  link: Option<String>,
}

impl Entry {
  /// The entry as a caller is told of it, standing in the folder at `folder` of the place `place_name`.
  fn wire(&self, place_name: &str, folder: &Within) -> wire::Entry {
    wire::Entry {
      place_name: place_name.to_owned(),
      path: folder.child(&self.name).joined(),
      name: self.name.clone(),
      kind: self.kind,
      byte_count: self.size,
      changed_at: self.modified,
      link_target: self.link.clone(),
    }
  }
}

/// The top of a place, as an entry: a folder with no name, size or time.
fn top_entry(place_name: &str) -> wire::Entry {
  wire::Entry {
    place_name: place_name.to_owned(),
    path: String::new(),
    name: String::new(),
    kind: Kind::Folder,
    byte_count: None,
    changed_at: None,
    link_target: None,
  }
}

/// The entry at `within` as it stands now, read from its folder.
async fn entry_at(place: &dyn Place, within: &Within) -> Result<wire::Entry, Refused> {
  let Some((folder, name)) = within.split() else {
    place.read_folder(within).await?;

    return Ok(top_entry(place.id()));
  };

  place
    .read_folder(&folder)
    .await?
    .into_iter()
    .find(|entry| entry.name == name)
    .map(|entry| entry.wire(place.id(), &folder))
    .ok_or_else(not_found)
}

/// Which of a file's bytes a reading asks for, said before the file's length is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Span {
  /// From `start` on, at most `most` bytes where that is given.
  From { start: u64, most: Option<u64> },
  /// The last so many bytes.
  Last(u64),
}

impl Span {
  const WHOLE: Self = Self::From {
    start: 0,
    most: None,
  };

  /// The first `head` bytes.
  fn head(head: u64) -> Self {
    Self::From {
      start: 0,
      most: Some(head),
    }
  }

  /// The one range a `Range` header asks for. Several ranges at once, or a header that reads as no
  /// range, ask for nothing in particular, and the file is read whole.
  fn ranged(header: &str) -> Option<Self> {
    let parsed = http_range_header::parse_range_header(header).ok()?;
    let [one] = parsed.ranges.as_slice() else {
      return None;
    };

    match (one.start, one.end) {
      (StartPosition::Index(start), EndPosition::LastByte) => Some(Self::From { start, most: None }),
      (StartPosition::Index(start), EndPosition::Index(end)) if end >= start => Some(Self::From {
        start,
        most: Some(end - start + 1),
      }),
      (StartPosition::FromLast(last), _) => Some(Self::Last(last)),
      (StartPosition::Index(_), EndPosition::Index(_)) => None,
    }
  }

  /// Where the bytes it names start in a file of `size` bytes, and how many there are: none where it
  /// names none of them.
  fn within(self, size: u64) -> (u64, u64) {
    match self {
      Self::From { start, most } => {
        let start = start.min(size);
        let left = size - start;

        (start, most.map_or(left, |most| most.min(left)))
      }
      Self::Last(last) => {
        let length = last.min(size);

        (size - length, length)
      }
    }
  }
}

/// A file opened to be streamed.
struct Opened {
  name: String,
  /// The whole file's length.
  size: u64,
  /// Where the body starts in the file.
  start: u64,
  /// How many bytes the body carries: the whole, or the part asked for.
  length: u64,
  body: Body,
}

/// Everything there is to walk on this machine now.
struct Machine {
  drives: Vec<Drive>,
  garage: Option<Garage>,
}

/// The key this machine is known by among the machines.
const LOCAL: &str = "local";

// How long one reading of the machine serves the requests after it. A folder of pictures asks for
// many at once, and each would otherwise list the drives again.
const MACHINE_FOR: Duration = Duration::from_secs(5);

/// A reading of the machine and when it was made.
type Reading = (Instant, Arc<Machine>);

static MACHINE: LazyLock<Mutex<Option<Reading>>> = LazyLock::new(|| Mutex::new(None));

/// This machine's host name.
fn host_name() -> String {
  std::fs::read_to_string("/proc/sys/kernel/hostname")
    .map(|name| name.trim().to_owned())
    .ok()
    .filter(|name| !name.is_empty())
    .unwrap_or_else(|| LOCAL.to_owned())
}

impl Machine {
  /// The machine as read a moment ago, or read now where that is older or `fresh` is asked.
  async fn current(fresh: bool, standing: &Standing) -> Result<Arc<Self>, Refused> {
    if !fresh {
      let kept = MACHINE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

      if let Some((at, machine)) = kept.as_ref()
        && at.elapsed() < MACHINE_FOR
      {
        return Ok(machine.clone());
      }
    }

    let read = Arc::new(Self::read(standing).await?);

    *MACHINE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((Instant::now(), read.clone()));

    Ok(read)
  }

  async fn read(standing: &Standing) -> Result<Self, Refused> {
    let mut drives = drives::this_machine().await.map_err(|why| {
      Refused::new(StatusCode::BAD_GATEWAY, "drives_unread", format!("the drives could not be read: {why}"))
    })?;
    let garage = garage::this_machine(&drives, &standing.settings).await;
    let own = garage.as_ref().map(|garage| garage.own.clone()).unwrap_or_default();
    let guard = Arc::new(Guard::this_machine(&standing.settings.protect, &own));

    for volume in drives.iter_mut().flat_map(|drive| drive.volumes.iter_mut()) {
      volume.guard = guard.clone();
    }

    Ok(Self { drives, garage })
  }

  /// The machine as a caller is told of it.
  fn machine(&self) -> wire::Machine {
    wire::Machine {
      name: host_name(),
      node_name: None,
      drives: self
        .drives
        .iter()
        .map(|drive| {
          drive.wire(|volume| wire::DriveVolume {
            place_name: volume.place.clone(),
            mount: volume.mount.clone(),
            fs_type: volume.fs_type.clone(),
          })
        })
        .collect(),
    }
  }

  /// The machine's places as a caller is told of them: its volumes, each with its free space, then its
  /// Garage where it has one.
  async fn places(&self) -> Vec<wire::Place> {
    let mut places = Vec::new();

    for drive in &self.drives {
      for volume in &drive.volumes {
        places.push(wire::Place {
          name: volume.place.clone(),
          kind: wire::PlaceKind::Volume,
          label: volume.mount.clone(),
          free_bytes: free_bytes(&volume.mount).await,
          node_name: None,
          drive_key: Some(drive.key.clone()),
          mount: Some(volume.mount.clone()),
          reason: None,
          reason_code: None,
        });
      }
    }

    places.extend(self.garage.as_ref().map(Garage::place));
    places
  }
}

// How long a volume may take to say its free space: a share that stopped answering says none.
const FREE_FOR: Duration = Duration::from_secs(2);

/// The bytes free on the volume mounted at `mount`, where it says in time.
async fn free_bytes(mount: &str) -> Option<u64> {
  let mount = mount.to_owned();
  let asked = tokio::task::spawn_blocking(move || {
    rustix::fs::statvfs(mount.as_str())
      .ok()
      .map(|stat| stat.f_bavail.saturating_mul(stat.f_frsize))
  });

  tokio::time::timeout(FREE_FOR, asked).await.ok()?.ok()?
}

impl Places for Machine {
  fn place(&self, id: &str) -> Result<&dyn Place, Refused> {
    self
      .drives
      .iter()
      .flat_map(|drive| drive.volumes.iter())
      .map(|one| one as &dyn Place)
      .chain(self.garage.iter().map(|one| one as &dyn Place))
      .find(|place| place.id() == id)
      .ok_or_else(no_place)
  }
}

/// Nothing where `owner` reaches the place `id`: its own space always, the machine branch as the
/// administrator alone; to anyone else a machine place is no place.
fn reaches(id: &str, owner: &Owner) -> Result<(), Refused> {
  if id == SPACE { Ok(()) } else { owner.administrator_only("no such place") }
}

/// The places a place name reaches for `owner`: its own space, or the reading of the machine the place
/// stands on, the machine branch being the administrator's alone.
async fn places_of(
  id: &str,
  owner: &Owner,
  standing: &Standing,
  fresh: bool,
) -> Result<Arc<dyn Places>, Refused> {
  reaches(id, owner)?;

  if id == SPACE {
    return Ok(Arc::new(Space::of(owner, standing).await?));
  }

  match node::node_of(id) {
    Some(name) => Ok(node::machine(standing, name, fresh).await?),
    None => Ok(Machine::current(fresh, standing).await?),
  }
}

/// The node a query names, where it names one.
fn named_node(node_name: Option<String>) -> Option<String> {
  node_name.filter(|name| !name.is_empty())
}

/// A page cut from a whole list: the members past the `after` already read, at most `limit`.
fn page_of<T>(all: Vec<T>, after: Option<u64>, limit: u64) -> (Vec<T>, bool) {
  let after = usize::try_from(after.unwrap_or(0)).unwrap_or(usize::MAX);
  let limit = usize::try_from(limit).unwrap_or(usize::MAX);
  let more = all.len() > after.saturating_add(limit);

  (all.into_iter().skip(after).take(limit).collect(), more)
}

// The most places a page carries, and the number it carries where none is asked.
const PLACES_MOST: u64 = 1000;

/// The places files live in: this machine's, read now, or a node's volumes.
pub struct PlacesList;

impl Route for PlacesList {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/places";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let owner = call.owner()?;
    let query: PlacesQuery = call.query()?;
    let all = match named_node(query.node_name) {
      Some(name) => {
        owner.administrator_only("no such node")?;
        node::machine(call.standing(), &name, false).await?.places()?
      }
      None => {
        let mut all = vec![space_place(&owner, call.standing()).await?];

        if owner.administrator {
          all.extend(Machine::current(true, call.standing()).await?.places().await);
        }

        all
      }
    };
    let (items, more) = page_of(all, query.after, query.limit.unwrap_or(PLACES_MOST).clamp(1, PLACES_MOST));

    Ok(answer(StatusCode::OK, PlacePage { items, more }))
  }
}

crate::route!(PlacesList);

/// This machine and its drives, or a node and its drives, read now.
pub struct MachineRead;

impl Route for MachineRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/machine";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    call.owner()?.administrator_only("no such machine")?;

    let query: MachineQuery = call.query()?;
    let machine = match named_node(query.node_name) {
      Some(name) => {
        let machine = node::machine(call.standing(), &name, true).await?.machine()?;

        forget_place_prefix(&format!("node:{name}:"));
        machine
      }
      None => Machine::current(true, call.standing()).await?.machine(),
    };

    Ok(answer(StatusCode::OK, machine))
  }
}

crate::route!(MachineRead);

/// The node list as the node service answers it now, the administrator's alone.
pub struct NodesList;

impl Route for NodesList {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/nodes";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    call.owner()?.administrator_only("no such route for this owner")?;

    let query: NodesQuery = call.query()?;
    let all = node::listed(call.standing())
      .await?
      .into_iter()
      .map(|node| ListedNode {
        name: node.name,
        label: node.label,
      })
      .collect();
    let (items, more) = page_of(all, query.after, query.limit.unwrap_or(PLACES_MOST).clamp(1, PLACES_MOST));

    Ok(answer(StatusCode::OK, NodePage { items, more }))
  }
}

crate::route!(NodesList);

// How long a folder's reading is kept for the pages after its first, and how many folders are kept.
const KEPT_FOR: Duration = Duration::from_secs(30);
const KEPT_MOST: usize = 16;

// Pages a request may ask for, and the one it gets where it names none.
const PAGE: u64 = 200;
const PAGE_MOST: u64 = 1000;

struct Kept {
  place: String,
  path: String,
  at: Instant,
  entries: Arc<Vec<Entry>>,
}

static KEPT: LazyLock<Mutex<Vec<Kept>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// The folder's reading kept from a moment ago, where there is one.
fn kept(place: &str, path: &str) -> Option<Arc<Vec<Entry>>> {
  let mut kept = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

  kept.retain(|one| one.at.elapsed() < KEPT_FOR);
  kept
    .iter()
    .find(|one| one.place == place && one.path == path)
    .map(|one| one.entries.clone())
}

fn keep(place: &str, path: &str, entries: Arc<Vec<Entry>>) {
  let mut kept = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

  kept.retain(|one| !(one.place == place && one.path == path));

  if kept.len() >= KEPT_MOST {
    kept.remove(0);
  }

  kept.push(Kept {
    place: place.to_owned(),
    path: path.to_owned(),
    at: Instant::now(),
    entries,
  });
}

/// Every kept folder reading of the places whose name begins with `prefix`: after a change there, or
/// a machine read again.
fn forget_place_prefix(prefix: &str) {
  KEPT
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .retain(|one| !one.place.starts_with(prefix));
}

/// One order over a whole folder: folders first, then the column asked for, names as the tiebreak.
fn sorted(entries: &[Entry], by: EntrySort, descending: bool) -> Vec<Entry> {
  let mut sorted = entries.to_vec();

  sorted.sort_by(|one, other| {
    let folders = (other.kind == Kind::Folder).cmp(&(one.kind == Kind::Folder));
    let column = match by {
      EntrySort::Name => Ordering::Equal,
      EntrySort::ByteCount => one.size.cmp(&other.size),
      EntrySort::ChangedAt => one.modified.cmp(&other.modified),
    };
    let named = column.then_with(|| order::natural(&one.name, &other.name));

    folders.then(if descending { named.reverse() } else { named })
  });

  sorted
}

/// The place a route's path names.
async fn path_place(call: &mut Call) -> Result<String, Refused> {
  call.path_param("placeName").await
}

/// The place and the entry a route's path names.
async fn path_entry(call: &mut Call) -> Result<(String, Within), Refused> {
  let mut params = call.path_params().await?;
  let id = params.remove("placeName").unwrap_or_default();
  let within = Within::read(&params.remove("entryPath").unwrap_or_default())?;

  Ok((id, within))
}

/// One page of a folder, cut from one order over the whole of it.
pub struct EntriesList;

impl Route for EntriesList {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/places/{placeName}/entries";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let owner = call.owner()?;
    let id = path_place(&mut call).await?;
    let query: EntriesQuery = call.query()?;
    let within = Within::read(query.path.as_deref().unwrap_or_default())?;
    let by = query.sort.unwrap_or_default();
    let descending = query.order.unwrap_or_default() == SortOrder::Desc;
    let limit = query.limit.unwrap_or(PAGE).clamp(1, PAGE_MOST);
    let folders_only = query.folders == Some(true);
    let path = within.joined();
    let kept_as = kept_name(&id, &owner);

    // A kept reading is handed out only to an owner who reaches the place at all.
    reaches(&id, &owner)?;

    let fresh = query.fresh == Some(true);
    let entries = match kept(&kept_as, &path).filter(|_| !fresh) {
      Some(entries) => entries,
      None => {
        let places = places_of(&id, &owner, call.standing(), false).await?;
        let read = Arc::new(places.place(&id)?.read_folder(&within).await?);

        keep(&kept_as, &path, read.clone());
        read
      }
    };

    let shown: Vec<Entry> = if folders_only {
      entries.iter().filter(|entry| entry.kind == Kind::Folder).cloned().collect()
    } else {
      entries.to_vec()
    };
    let total = shown.len() as u64;
    let (page, more) = page_of(sorted(&shown, by, descending), query.after, limit);

    Ok(answer(
      StatusCode::OK,
      EntryPage {
        items: page.iter().map(|entry| entry.wire(&id, &within)).collect(),
        more,
        total_count: total,
      },
    ))
  }
}

crate::route!(EntriesList);

/// One entry, as it stands now.
pub struct EntryRead;

impl Route for EntryRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/places/{placeName}/entries/{entryPath}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let owner = call.owner()?;
    let (id, within) = path_entry(&mut call).await?;
    let places = places_of(&id, &owner, call.standing(), false).await?;

    Ok(answer(StatusCode::OK, entry_at(places.place(&id)?, &within).await?))
  }
}

crate::route!(EntryRead);

/// The type a file is handed over as, guessed from its name; text is said to be UTF-8.
fn content_type(name: &str) -> String {
  let guessed = mime_guess::from_path(name).first_or_octet_stream();

  if guessed.type_() == mime_guess::mime::TEXT
    || matches!(guessed.essence_str(), "application/json" | "application/javascript" | "application/xml")
  {
    format!("{}; charset=utf-8", guessed.essence_str())
  } else {
    guessed.essence_str().to_owned()
  }
}

/// How a file is handed over: shown where the browser can, or saved, under its own name. The plain
/// name is for a browser that reads no other; every other character it cannot carry is a `_`.
fn disposition(name: &str, download: bool) -> String {
  let plain: String = name
    .chars()
    .map(|one| {
      if one.is_ascii_graphic() && one != '"' && one != '\\' || one == ' ' {
        one
      } else {
        '_'
      }
    })
    .collect();
  let how = if download { "attachment" } else { "inline" };

  format!(
    "{how}; filename=\"{plain}\"; filename*=UTF-8''{}",
    utf8_percent_encode(name, NON_ALPHANUMERIC)
  )
}

fn header_value(value: &str) -> HeaderValue {
  HeaderValue::from_str(value).unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"))
}

// The sandbox a file shown in the browser stands in: nothing on it runs, and it reaches nothing of the
// console it is served from.
const SANDBOX: &str =
  "sandbox; default-src 'none'; img-src 'self' data: blob:; media-src 'self'; style-src 'unsafe-inline'";

/// The headers every streamed answer goes to the browser with, beside its own type, length and name
/// (`docs/files.md`, 「Reading a file」): its type fixed, kept from any cache, and in a sandbox, but for a
/// PDF, which the browser's own viewer refuses to draw in one.
fn shown(headers: &mut HeaderMap) {
  let pdf = headers
    .get(header::CONTENT_TYPE)
    .and_then(|value| value.to_str().ok())
    .and_then(|value| value.split(';').next())
    .is_some_and(|essence| essence.trim().eq_ignore_ascii_case("application/pdf"));

  headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
  headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));

  if !pdf {
    headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(SANDBOX));
  }
}

/// The status and the range headers a reading answers with: the whole file, the part a range asked
/// for, or a refusal where the range names no byte of it.
fn ranged_answer(opened: &Opened, ranged: bool) -> (StatusCode, Option<String>) {
  match (ranged, opened.length) {
    (false, _) => (StatusCode::OK, None),
    (true, 0) => (StatusCode::RANGE_NOT_SATISFIABLE, Some(format!("bytes */{}", opened.size))),
    (true, length) => (
      StatusCode::PARTIAL_CONTENT,
      Some(format!("bytes {}-{}/{}", opened.start, opened.start + length - 1, opened.size)),
    ),
  }
}

/// A file's bytes, streamed as they are read: the whole, its head, or the one range asked for.
pub struct ContentRead;

impl Route for ContentRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/places/{placeName}/entries/{entryPath}/content";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let owner = call.owner()?;
    let (id, within) = path_entry(&mut call).await?;
    let query: ContentQuery = call.query()?;
    let range = asked_range(&call);

    if within.last().is_none() {
      return Err(is_folder());
    }

    let span = range.or(query.head.map(Span::head)).unwrap_or(Span::WHOLE);
    let places = places_of(&id, &owner, call.standing(), false).await?;

    read_answer(places.place(&id)?, &within, span, range.is_some(), query.download == Some(true)).await
  }
}

crate::route!(ContentRead);

/// The one byte range the request's `Range` header asks for, where it asks for one.
fn asked_range(call: &Call) -> Option<Span> {
  call.header(header::RANGE).as_deref().and_then(Span::ranged)
}

/// The file at `within` answered as a reading: the bytes `span` names, under the range headers where
/// `ranged` says a `Range` header asked for them, shown or saved as `download` says.
async fn read_answer(
  place: &dyn Place,
  within: &Within,
  span: Span,
  ranged: bool,
  download: bool,
) -> Result<Response, Refused> {
  let opened = place.open(within, span).await?;
  let (status, content_range) = ranged_answer(&opened, ranged);
  let length = if status == StatusCode::RANGE_NOT_SATISFIABLE { 0 } else { opened.length };
  let mut response = Response::new(if length == 0 { Body::empty() } else { opened.body });

  *response.status_mut() = status;

  let headers = response.headers_mut();

  headers.insert(header::CONTENT_TYPE, header_value(&content_type(&opened.name)));
  headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
  headers.insert(header::CONTENT_DISPOSITION, header_value(&disposition(&opened.name, download)));
  headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
  headers.insert("x-file-size", HeaderValue::from(opened.size));

  if let Some(content_range) = content_range {
    headers.insert(header::CONTENT_RANGE, header_value(&content_range));
  }

  shown(headers);

  Ok(response)
}
