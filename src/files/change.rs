//! The file viewer's changes (`docs/files.md`, 「Changing files」): making a folder, writing a file,
//! renaming, moving, copying and deleting, on any place. A name already taken is settled here, for every
//! place alike, before the place is asked to change anything; copying between two places streams each file
//! from the one into the other. Bytes arriving in a space with a quota are judged against it before
//! anything is changed. Every change made is an event of the service's stream, the asking owner's.

use std::collections::HashMap;
use std::io;

use axum::http::{Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::{StreamExt, TryStreamExt};

use super::{
  Entry, Kind, Pending, Place, Span, Upload, Within, entry_at, fair_name, forget_place_prefix, into_itself,
  not_found, path_entry, path_place, place_top, places_of,
};
use crate::events::Events;
use crate::http::{Call, Refused, Route, answer};
use crate::owner::Owner;
use crate::wire::{self, Conflict, CopyRequest, EntryKey, EntryMove, EntryPage, FolderRequest, WriteQuery};

/// What becomes of one arriving name.
#[derive(Debug, PartialEq, Eq)]
enum Landing {
  /// It lands under this name, free in the folder.
  New(String),
  /// It lands under its own name, where an entry of this kind gives way.
  Replacing(String, Kind),
  Skipped,
}

/// The first free name for `name` among `taken`, a number added before a file's ending.
fn free_name(name: &str, folder: bool, taken: &HashMap<String, Kind>) -> String {
  let (stem, ending) = match name.rfind('.') {
    Some(dot) if dot > 0 && !folder => name.split_at(dot),
    _ => (name, ""),
  };

  (2..)
    .map(|number| format!("{stem} ({number}){ending}"))
    .find(|candidate| !taken.contains_key(candidate))
    .unwrap_or_else(|| name.to_owned())
}

/// The names arriving in a folder, each settled against what stands there: where `conflict` refuses,
/// the colliding names instead.
fn settle(
  standing: HashMap<String, Kind>,
  arriving: &[(String, bool)],
  conflict: Conflict,
) -> Result<Vec<Landing>, Vec<String>> {
  let mut taken = standing.clone();
  let mut landings = Vec::new();
  let mut colliding = Vec::new();

  for (name, folder) in arriving {
    let Some(kind) = taken.get(name).copied() else {
      taken.insert(name.clone(), Kind::Other);
      landings.push(Landing::New(name.clone()));
      continue;
    };

    match conflict {
      Conflict::Refuse => colliding.push(name.clone()),
      Conflict::Replace if standing.contains_key(name) => {
        landings.push(Landing::Replacing(name.clone(), kind))
      }
      Conflict::Skip => landings.push(Landing::Skipped),
      Conflict::Rename | Conflict::Replace => {
        let free = free_name(name, *folder, &taken);

        taken.insert(free.clone(), Kind::Other);
        landings.push(Landing::New(free));
      }
    }
  }

  if colliding.is_empty() { Ok(landings) } else { Err(colliding) }
}

/// What stands in a folder now, by name.
async fn standing_in(place: &dyn Place, folder: &Within) -> Result<HashMap<String, Kind>, Refused> {
  Ok(
    place
      .read_folder(folder)
      .await?
      .into_iter()
      .map(|entry| (entry.name, entry.kind))
      .collect(),
  )
}

/// A change refused before anything is changed, naming every entry that would collide.
fn conflict_answer(names: Vec<String>) -> Response {
  super::taken().with("names", names).into_response()
}

/// A path that names an entry, never a place's top.
fn entry_path(path: &str) -> Result<Within, Refused> {
  let within = Within::read(path)?;

  if within.names.is_empty() {
    return Err(place_top());
  }

  Ok(within)
}

/// The bytes of the files the entry at `source` of a place holds, as a copy into another place carries
/// them: every file under a folder, read by the place's own readings.
fn carried_bytes<'a>(place: &'a dyn Place, source: Within, kind: Kind) -> Pending<'a, u64> {
  Box::pin(async move {
    match kind {
      Kind::File => Ok(entry_at(place, &source).await?.byte_count.unwrap_or(0)),
      Kind::Folder => {
        let mut bytes: u64 = 0;

        for entry in place.read_folder(&source).await? {
          bytes = bytes.saturating_add(carried_bytes(place, source.child(&entry.name), entry.kind).await?);
        }

        Ok(bytes)
      }
      Kind::Other => Ok(0),
    }
  })
}

/// The kind of the entry at `within`, read from its folder; a place's top is a folder.
pub(super) async fn kind_of(place: &dyn Place, within: &Within) -> Result<Kind, Refused> {
  let Some((folder, name)) = within.split() else {
    return Ok(Kind::Folder);
  };

  standing_in(place, &folder).await?.get(name).copied().ok_or_else(not_found)
}

/// The entry at `source` of one place carried into another at `target`: every folder made again and
/// every file streamed across. What is neither is left behind.
fn carry<'a>(
  from: &'a dyn Place,
  source: Within,
  kind: Kind,
  to: &'a dyn Place,
  target: Within,
) -> Pending<'a, ()> {
  Box::pin(async move {
    match kind {
      Kind::Folder => {
        to.make_folder(&target).await?;

        for entry in from.read_folder(&source).await? {
          carry(from, source.child(&entry.name), entry.kind, to, target.child(&entry.name)).await?;
        }

        Ok(())
      }
      Kind::File => {
        let opened = from.open(&source, Span::WHOLE).await?;
        let upload = Upload {
          length: opened.length,
          body: opened.body.into_data_stream().map_err(io::Error::other).boxed(),
        };

        to.write(&target, upload).await
      }
      Kind::Other => Ok(()),
    }
  })
}

/// The event of an entry gone from its path, the owner's it was made for.
async fn deleted(events: &Events, owner: &Owner, place_name: &str, within: &Within) {
  let key = EntryKey {
    place_name: place_name.to_owned(),
    path: within.joined(),
  };

  events.publish(&owner.key, "entry.deleted", key).await;
}

/// The entry now standing at `within`, told as an event of `kind`, the owner's it was made for.
async fn arrived(
  events: &Events,
  owner: &Owner,
  place: &dyn Place,
  within: &Within,
  kind: &str,
) -> Result<wire::Entry, Refused> {
  let entry = entry_at(place, within).await?;

  events.publish(&owner.key, kind, &entry).await;

  Ok(entry)
}

/// An entry given a new path in its own place: a new name in its folder, or a folder of its own.
pub struct EntryMoveRoute;

impl Route for EntryMoveRoute {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/places/{placeName}/entries/{entryPath}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let owner = call.owner()?;
    let (id, within) = path_entry(&mut call).await?;
    let moved: EntryMove = call.body().await?;
    let conflict = moved.conflict.unwrap_or(Conflict::Refuse);

    if conflict == Conflict::Skip {
      return Err(Refused::bad_request("a move is refused, replaced or renamed, never skipped"));
    }

    let source = entry_path(&within.joined())?;
    let target = entry_path(&moved.path)?;
    let places = places_of(&id, &owner, call.standing(), false).await?;
    let place = places.place(&id)?;

    // Moved onto itself, an entry stays as it is.
    if target == source {
      return Ok(answer(StatusCode::OK, entry_at(place, &source).await?));
    }

    if target.names.starts_with(&source.names) {
      return Err(into_itself());
    }

    let (to_folder, name) = target.split().ok_or_else(place_top)?;
    let (from_folder, _) = source.split().ok_or_else(place_top)?;

    if !fair_name(name) {
      return Err(Refused::rule("bad-name", "the name cannot stand in a path"));
    }

    let folder = kind_of(place, &source).await? == Kind::Folder;
    let landing =
      match settle(standing_in(place, &to_folder).await?, &[(name.to_owned(), folder)], conflict) {
        Ok(landings) => landings.into_iter().next(),
        Err(names) => return Ok(conflict_answer(names)),
      };
    let events = &call.standing().events;
    let landed = match landing {
      Some(Landing::New(name)) => name,
      Some(Landing::Replacing(name, _)) => {
        place.remove(&to_folder.child(&name)).await?;
        deleted(events, &owner, &id, &to_folder.child(&name)).await;
        name
      }
      Some(Landing::Skipped) | None => return Err(Refused::failed("the move landed nowhere")),
    };
    let at = to_folder.child(&landed);

    if from_folder == to_folder {
      place.rename(&source, &landed).await?;
    } else {
      place.copy_here(&source, &at, true).await?;
    }

    forget_place_prefix(&id);
    deleted(events, &owner, &id, &source).await;

    Ok(answer(StatusCode::OK, arrived(events, &owner, place, &at, "entry.created").await?))
  }
}

crate::route!(EntryMoveRoute);

/// An entry deleted, a folder with everything in it.
pub struct EntryDelete;

impl Route for EntryDelete {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/places/{placeName}/entries/{entryPath}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let owner = call.owner()?;
    let (id, within) = path_entry(&mut call).await?;
    let within = entry_path(&within.joined())?;
    let places = places_of(&id, &owner, call.standing(), false).await?;

    places.place(&id)?.remove(&within).await?;
    forget_place_prefix(&id);
    deleted(&call.standing().events, &owner, &id, &within).await;

    Ok(StatusCode::NO_CONTENT.into_response())
  }
}

crate::route!(EntryDelete);

/// One file, its bytes the request's body, written as they stream in.
pub struct ContentWrite;

impl Route for ContentWrite {
  const METHOD: Method = Method::PUT;
  const PATH: &'static str = "/places/{placeName}/entries/{entryPath}/content";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let owner = call.owner()?;
    let (id, within) = path_entry(&mut call).await?;
    let within = entry_path(&within.joined())?;
    let query: WriteQuery = call.query()?;
    let conflict = query.conflict.unwrap_or(Conflict::Replace);
    let length = call
      .header(header::CONTENT_LENGTH)
      .and_then(|value| value.parse::<u64>().ok())
      .ok_or_else(|| Refused::bad_request("the file's bytes carry no Content-Length"))?;
    let (folder, name) = within.split().ok_or_else(place_top)?;
    let places = places_of(&id, &owner, call.standing(), false).await?;
    let place = places.place(&id)?;
    let standing: Vec<Entry> = place.read_folder(&folder).await?;
    let names = standing.iter().map(|entry| (entry.name.clone(), entry.kind)).collect();
    let landing = match settle(names, &[(name.to_owned(), false)], conflict) {
      Ok(landings) => landings.into_iter().next(),
      Err(names) => return Ok(conflict_answer(names)),
    };

    if let Some(room) = places.room() {
      // A file written over gives its bytes back as the new one takes its place.
      let freeing = match &landing {
        Some(Landing::Replacing(name, Kind::File)) => standing
          .iter()
          .find(|entry| entry.name == *name)
          .and_then(|entry| entry.size)
          .unwrap_or(0),
        _ => 0,
      };

      if !matches!(landing, Some(Landing::Skipped) | None) {
        room.take(length, freeing).await?;
      }
    }

    let (landed, made) = match landing {
      Some(Landing::New(name)) => (name, true),
      Some(Landing::Replacing(name, kind)) => {
        // A file gives way as the new one is put in its place; a folder must go first.
        if kind != Kind::File {
          place.remove(&folder.child(&name)).await?;
        }

        (name, false)
      }
      Some(Landing::Skipped) | None => return Ok(answer(StatusCode::OK, entry_at(place, &within).await?)),
    };
    let body = std::mem::take(call.request.body_mut());
    let upload = Upload {
      length,
      body: body.into_data_stream().map_err(io::Error::other).boxed(),
    };
    let at = folder.child(&landed);

    place.write(&at, upload).await?;
    forget_place_prefix(&id);

    let (status, kind) = if made {
      (StatusCode::CREATED, "entry.created")
    } else {
      (StatusCode::OK, "entry.changed")
    };

    Ok(answer(status, arrived(&call.standing().events, &owner, place, &at, kind).await?))
  }
}

crate::route!(ContentWrite);

/// A new folder.
pub struct FoldersCreate;

impl Route for FoldersCreate {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/places/{placeName}/folders";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let owner = call.owner()?;
    let id = path_place(&mut call).await?;
    let asked: FolderRequest = call.body().await?;
    let within = entry_path(&asked.path)?;
    let conflict = asked.conflict.unwrap_or(Conflict::Refuse);
    let (folder, name) = within.split().ok_or_else(place_top)?;
    let places = places_of(&id, &owner, call.standing(), false).await?;
    let place = places.place(&id)?;
    let landing = match settle(standing_in(place, &folder).await?, &[(name.to_owned(), true)], conflict) {
      Ok(landings) => landings.into_iter().next(),
      Err(names) => return Ok(conflict_answer(names)),
    };
    let events = &call.standing().events;
    let landed = match landing {
      Some(Landing::New(name)) => name,
      Some(Landing::Replacing(name, _)) => {
        place.remove(&folder.child(&name)).await?;
        deleted(events, &owner, &id, &folder.child(&name)).await;
        name
      }
      Some(Landing::Skipped) | None => return Ok(answer(StatusCode::OK, entry_at(place, &within).await?)),
    };
    let at = folder.child(&landed);

    place.make_folder(&at).await?;
    forget_place_prefix(&id);

    Ok(answer(StatusCode::CREATED, arrived(events, &owner, place, &at, "entry.created").await?))
  }
}

crate::route!(FoldersCreate);

/// Entries of one place copied into a folder of the same place or of another.
pub struct CopiesCreate;

impl Route for CopiesCreate {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/places/{placeName}/copies";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let owner = call.owner()?;
    let from_id = path_place(&mut call).await?;
    let asked: CopyRequest = call.body().await?;
    let to_id = asked.to_place_name.filter(|name| !name.is_empty()).unwrap_or_else(|| from_id.clone());
    let target = Within::read(&asked.to)?;
    let conflict = asked.conflict.unwrap_or(Conflict::Refuse);
    let same = from_id == to_id;
    let mut sources = Vec::new();

    for path in &asked.paths {
      let source = entry_path(path)?;

      if same && target.names.starts_with(&source.names) {
        return Err(into_itself());
      }

      sources.push(source);
    }

    let from_places = places_of(&from_id, &owner, call.standing(), false).await?;
    let to_places = places_of(&to_id, &owner, call.standing(), false).await?;
    let (from_place, to_place) = (from_places.place(&from_id)?, to_places.place(&to_id)?);
    let mut kinds = Vec::new();

    for source in &sources {
      kinds.push(kind_of(from_place, source).await?);
    }

    let arriving: Vec<(String, bool)> = sources
      .iter()
      .zip(&kinds)
      .map(|(source, kind)| (source.last().unwrap_or_default().to_owned(), *kind == Kind::Folder))
      .collect();
    let landings = match settle(standing_in(to_place, &target).await?, &arriving, conflict) {
      Ok(landings) => landings,
      Err(names) => return Ok(conflict_answer(names)),
    };

    if let Some(room) = to_places.room() {
      // A copy counts every file it carries and frees nothing it replaces.
      let mut adding: u64 = 0;

      for ((source, kind), landing) in sources.iter().zip(&kinds).zip(&landings) {
        if *landing == Landing::Skipped {
          continue;
        }

        let bytes = if same {
          room.bytes_at(source).await?
        } else {
          carried_bytes(from_place, source.clone(), *kind).await?
        };

        adding = adding.saturating_add(bytes);
      }

      room.take(adding, 0).await?;
    }

    let events = &call.standing().events;
    let mut landed = Vec::new();

    for ((source, kind), landing) in sources.into_iter().zip(kinds).zip(landings) {
      let name = match landing {
        Landing::Skipped => continue,
        Landing::New(name) => name,
        Landing::Replacing(name, _) => {
          to_place.remove(&target.child(&name)).await?;
          deleted(events, &owner, &to_id, &target.child(&name)).await;
          name
        }
      };
      let at = target.child(&name);

      if same {
        from_place.copy_here(&source, &at, false).await?;
      } else {
        carry(from_place, source, kind, to_place, at.clone()).await?;
      }

      landed.push(at);
    }

    forget_place_prefix(&to_id);

    let mut items = Vec::new();

    for at in &landed {
      items.push(arrived(events, &owner, to_place, at, "entry.created").await?);
    }

    let total_count = items.len() as u64;

    Ok(answer(
      StatusCode::CREATED,
      EntryPage {
        items,
        more: false,
        total_count,
      },
    ))
  }
}

crate::route!(CopiesCreate);
