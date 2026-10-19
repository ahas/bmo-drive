//! The file viewer (`docs/files.md`): this machine's drives, each a place the console walks and changes as
//! a file explorer does, and the routes that read them (`docs/api.md`, 「The file viewer」).

mod change;
mod drives;
mod garage;
mod guard;
mod order;
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
use serde::Serialize;
use serde_json::{Value, json};

use crate::http::{Call, Refused, Route, Standing, answer};
use drives::Drive;
use garage::Garage;
use guard::Guard;

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

/// A machine's places as one reading of it lists them.
trait Places: Send + Sync {
  /// The place a request names, where the reading lists it; anything else is no place.
  fn place(&self, id: &str) -> Result<&dyn Place, Refused>;
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
const OUTSIDE_PLACE: &str = "outside_place";

fn outside() -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, OUTSIDE_PLACE, "the path would leave its place")
}

fn taken() -> Refused {
  Refused::new(StatusCode::CONFLICT, "name_taken", "the name is already taken")
}

fn protected() -> Refused {
  Refused::new(StatusCode::FORBIDDEN, "protected", "a protected location cannot be changed")
}

fn into_itself() -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, "into_itself", "a folder cannot be copied or moved into itself")
}

fn cut_short() -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, "cut_short", "the file did not arrive whole")
}

fn not_found() -> Refused {
  Refused::new(StatusCode::NOT_FOUND, "not_found", "nothing stands at that path")
}

fn no_place() -> Refused {
  Refused::new(StatusCode::NOT_FOUND, "no_place", "no such place")
}

fn is_folder() -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, "is_folder", "a folder cannot be read as a file")
}

fn not_folder() -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, "not_folder", "it is not a folder")
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
      Refused::new(StatusCode::FORBIDDEN, "no_permission", "the account may not read or change it")
    }
    io::ErrorKind::AlreadyExists => taken(),
    other => Refused::new(StatusCode::BAD_REQUEST, "failed_io", format!("it could not be done: {other}")),
  }
}

/// What a refused reading of the file system says, by what went wrong.
fn io_refusal(error: &io::Error) -> Refused {
  match error.kind() {
    io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied | io::ErrorKind::AlreadyExists => {
      io_kind_refusal(error.kind())
    }
    _ => Refused::new(StatusCode::BAD_REQUEST, "failed_io", format!("it could not be done: {error}")),
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
  Folder,
  File,
  /// A device, a socket, a pipe, or a link that leads nowhere.
  Other,
}

/// One entry of a folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Entry {
  name: String,
  kind: Kind,
  /// A file's length in bytes; a folder has none.
  size: Option<u64>,
  /// When it last changed, in Unix milliseconds.
  modified: Option<i64>,
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

  /// The machine as the console is told of it.
  fn view(&self) -> Value {
    json!({
      "key": LOCAL,
      "name": host_name(),
      "local": true,
      "disks": self.drives,
      "garage": self.garage.as_ref().map(Garage::view),
      "error": null,
      "errorCode": null,
    })
  }
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

/// The reading of the machine a place stands on.
async fn places_of(_id: &str, standing: &Standing, fresh: bool) -> Result<Arc<dyn Places>, Refused> {
  Ok(Machine::current(fresh, standing).await?)
}

/// A query parameter read as a count, where it is given.
fn count(call: &Call, name: &str) -> Result<Option<u64>, Refused> {
  call
    .query(name)
    .filter(|value| !value.is_empty())
    .map(|value| {
      value.parse::<u64>().map_err(|_| {
        Refused::new(StatusCode::BAD_REQUEST, "bad_count", format!("{name} must be a whole number from 0"))
      })
    })
    .transpose()
}

fn missing_place() -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, "missing_place", "no place was sent")
}

fn named_place(call: &Call) -> Result<String, Refused> {
  call.query("place").filter(|place| !place.is_empty()).ok_or_else(missing_place)
}

/// The machines: this one read with its drives.
pub struct FilesPlaces;

impl Route for FilesPlaces {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/api/files/places";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let machine = Machine::current(true, call.standing()).await?;
    let machines = vec![machine.view()];

    Ok(answer(StatusCode::OK, json!({ "machines": machines })))
  }
}

crate::route!(FilesPlaces);

/// One machine's drives, read now.
pub struct FilesMachine;

impl Route for FilesMachine {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/api/files/machine";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let key = call
      .query("machine")
      .filter(|key| !key.is_empty())
      .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "missing_machine", "no machine was sent"))?;
    let view = match key.as_str() {
      LOCAL => Machine::current(true, call.standing()).await?.view(),
      _ => return Err(Refused::new(StatusCode::NOT_FOUND, "no_machine", "no such machine")),
    };

    Ok(answer(StatusCode::OK, view))
  }
}

crate::route!(FilesMachine);

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

/// The column a folder is ordered by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortBy {
  Name,
  Size,
  Modified,
}

impl SortBy {
  fn read(value: Option<String>) -> Result<Self, Refused> {
    match value.as_deref() {
      None | Some("" | "name") => Ok(Self::Name),
      Some("size") => Ok(Self::Size),
      Some("modified") => Ok(Self::Modified),
      Some(other) => Err(Refused::new(
        StatusCode::BAD_REQUEST,
        "bad_sort",
        format!("a folder cannot be sorted by {other}"),
      )),
    }
  }
}

/// One order over a whole folder: folders first, then the column asked for, names as the tiebreak.
fn sorted(entries: &[Entry], by: SortBy, descending: bool) -> Vec<Entry> {
  let mut sorted = entries.to_vec();

  sorted.sort_by(|one, other| {
    let folders = (other.kind == Kind::Folder).cmp(&(one.kind == Kind::Folder));
    let column = match by {
      SortBy::Name => Ordering::Equal,
      SortBy::Size => one.size.cmp(&other.size),
      SortBy::Modified => one.modified.cmp(&other.modified),
    };
    let named = column.then_with(|| order::natural(&one.name, &other.name));

    folders.then(if descending { named.reverse() } else { named })
  });

  sorted
}

/// One page of a folder, cut from one order over the whole of it.
pub struct FilesList;

impl Route for FilesList {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/api/files/list";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let id = named_place(&call)?;
    let within = Within::read(&call.query("path").unwrap_or_default())?;
    let by = SortBy::read(call.query("sort"))?;
    let descending = match call.query("order").as_deref() {
      None | Some("" | "asc") => false,
      Some("desc") => true,
      Some(_) => {
        return Err(Refused::new(StatusCode::BAD_REQUEST, "bad_order", "order is asc or desc"));
      }
    };
    let offset = count(&call, "offset")?.unwrap_or(0);
    let limit = count(&call, "limit")?.unwrap_or(PAGE).clamp(1, PAGE_MOST);
    let folders_only = call.query("folders").as_deref() == Some("1");
    let path = within.joined();

    let fresh = call.query("fresh").as_deref() == Some("1");
    let entries = match kept(&id, &path).filter(|_| !fresh) {
      Some(entries) => entries,
      None => {
        let places = places_of(&id, call.standing(), false).await?;
        let read = Arc::new(places.place(&id)?.read_folder(&within).await?);

        keep(&id, &path, read.clone());
        read
      }
    };

    let shown: Vec<Entry> = if folders_only {
      entries.iter().filter(|entry| entry.kind == Kind::Folder).cloned().collect()
    } else {
      entries.to_vec()
    };
    let total = shown.len();
    let page: Vec<Entry> = sorted(&shown, by, descending)
      .into_iter()
      .skip(usize::try_from(offset).unwrap_or(usize::MAX))
      .take(usize::try_from(limit).unwrap_or(usize::MAX))
      .collect();

    Ok(answer(
      StatusCode::OK,
      json!({ "place": id, "path": path, "total": total, "offset": offset, "entries": page }),
    ))
  }
}

crate::route!(FilesList);

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
pub struct FilesRead;

impl Route for FilesRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/api/files/read";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let id = named_place(&call)?;
    let within = Within::read(&call.query("path").unwrap_or_default())?;
    let head = count(&call, "head")?;
    let download = call.query("download").as_deref() == Some("1");
    let range = asked_range(&call);

    if within.last().is_none() {
      return Err(is_folder());
    }

    let span = range.or(head.map(Span::head)).unwrap_or(Span::WHOLE);
    let places = places_of(&id, call.standing(), false).await?;

    read_answer(places.place(&id)?, &within, span, range.is_some(), download).await
  }
}

crate::route!(FilesRead);

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
