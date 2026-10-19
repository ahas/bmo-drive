//! The file viewer's changes (`docs/files.md`, 「Changing files」): making a folder, uploading, renaming,
//! copying, moving and deleting, on any place. A name already taken is settled here, for every place
//! alike, before the place is asked to change anything; copying between two places streams each file
//! from the one into the other.

use std::collections::HashMap;
use std::io;

use axum::http::{Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::{StreamExt, TryStreamExt};
use serde_json::{Value, json};

use super::{
  Kind, Pending, Place, Span, Upload, Within, fair_name, forget_place_prefix, into_itself, missing_place,
  not_found, places_of,
};
use crate::http::{Call, Refused, Route, answer};

/// How a name already taken in the folder is settled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Conflict {
  /// Nothing is changed, and the colliding names are said.
  Refuse,
  /// The entry standing there gives way.
  Replace,
  /// The arriving entry takes the first free name with a number added.
  Rename,
  /// The colliding entries are left out.
  Skip,
}

impl Conflict {
  fn read(value: Option<&str>) -> Result<Self, Refused> {
    match value {
      None | Some("" | "refuse") => Ok(Self::Refuse),
      Some("replace") => Ok(Self::Replace),
      Some("rename") => Ok(Self::Rename),
      Some("skip") => Ok(Self::Skip),
      Some(other) => Err(Refused::new(
        StatusCode::BAD_REQUEST,
        "bad_conflict",
        format!("{other} is not a way to settle a taken name"),
      )),
    }
  }
}

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

fn text<'a>(body: &'a Value, field: &str) -> Result<&'a str, Refused> {
  body
    .get(field)
    .and_then(Value::as_str)
    .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "missing_field", format!("{field} was not sent")))
}

fn outside_top() -> Refused {
  Refused::new(StatusCode::BAD_REQUEST, "place_top", "the top of a place cannot be changed")
}

/// A path that names an entry, never a place's top.
fn entry_path(path: &str) -> Result<Within, Refused> {
  let within = Within::read(path)?;

  if within.names.is_empty() {
    return Err(outside_top());
  }

  Ok(within)
}

/// The kind of the entry at `within`, read from its folder.
pub(super) async fn kind_of(place: &dyn Place, within: &Within) -> Result<Kind, Refused> {
  let (folder, name) = within.split().ok_or_else(outside_top)?;

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

/// A new folder.
pub struct FilesFolder;

impl Route for FilesFolder {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/api/files/folder";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let id = text(&body, "place")?;
    let within = entry_path(text(&body, "path")?)?;
    let conflict = Conflict::read(body.get("conflict").and_then(Value::as_str))?;
    let (folder, name) = within.split().ok_or_else(outside_top)?;
    let places = places_of(id, call.standing(), false).await?;
    let place = places.place(id)?;
    let landings = match settle(standing_in(place, &folder).await?, &[(name.to_owned(), true)], conflict) {
      Ok(landings) => landings,
      Err(names) => return Ok(conflict_answer(names)),
    };
    let landed = match landings.into_iter().next() {
      Some(Landing::New(name)) => {
        place.make_folder(&folder.child(&name)).await?;
        Some(name)
      }
      Some(Landing::Replacing(name, _)) => {
        place.remove(&folder.child(&name)).await?;
        place.make_folder(&folder.child(&name)).await?;
        Some(name)
      }
      Some(Landing::Skipped) | None => None,
    };

    forget_place_prefix(id);

    Ok(answer(StatusCode::OK, json!({ "name": landed })))
  }
}

crate::route!(FilesFolder);

/// One file, its bytes the request's body, written as they stream in.
pub struct FilesUpload;

impl Route for FilesUpload {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/api/files/upload";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let id = call.query("place").filter(|place| !place.is_empty()).ok_or_else(missing_place)?;
    let within = entry_path(&call.query("path").unwrap_or_default())?;
    let conflict = Conflict::read(call.query("conflict").as_deref())?;
    let length = call
      .header(header::CONTENT_LENGTH)
      .and_then(|value| value.parse::<u64>().ok())
      .ok_or_else(|| {
        Refused::new(StatusCode::LENGTH_REQUIRED, "length_required", "the upload carries no Content-Length")
      })?;
    let (folder, name) = within.split().ok_or_else(outside_top)?;
    let places = places_of(&id, call.standing(), false).await?;
    let place = places.place(&id)?;
    let landings = match settle(standing_in(place, &folder).await?, &[(name.to_owned(), false)], conflict) {
      Ok(landings) => landings,
      Err(names) => return Ok(conflict_answer(names)),
    };
    let landed = match landings.into_iter().next() {
      Some(Landing::New(name)) => name,
      Some(Landing::Replacing(name, kind)) => {
        // A file gives way as the new one is put in its place; a folder must go first.
        if kind != Kind::File {
          place.remove(&folder.child(&name)).await?;
        }

        name
      }
      Some(Landing::Skipped) | None => return Ok(answer(StatusCode::OK, json!({ "name": null }))),
    };
    let body = std::mem::take(call.request.body_mut());
    let upload = Upload {
      length,
      body: body.into_data_stream().map_err(io::Error::other).boxed(),
    };

    place.write(&folder.child(&landed), upload).await?;
    forget_place_prefix(&id);

    Ok(answer(StatusCode::OK, json!({ "name": landed })))
  }
}

crate::route!(FilesUpload);

/// One entry given a new name in its own folder.
pub struct FilesRename;

impl Route for FilesRename {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/api/files/rename";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let id = text(&body, "place")?;
    let within = entry_path(text(&body, "path")?)?;
    let name = text(&body, "name")?;

    if !fair_name(name) {
      return Err(Refused::new(StatusCode::BAD_REQUEST, "bad_name", "the name cannot stand in a path"));
    }

    if within.last() == Some(name) {
      return Ok(answer(StatusCode::OK, json!({ "name": name })));
    }

    let (folder, _) = within.split().ok_or_else(outside_top)?;
    let places = places_of(id, call.standing(), false).await?;
    let place = places.place(id)?;

    if let Err(names) =
      settle(standing_in(place, &folder).await?, &[(name.to_owned(), false)], Conflict::Refuse)
    {
      return Ok(conflict_answer(names));
    }

    place.rename(&within, name).await?;
    forget_place_prefix(id);

    Ok(answer(StatusCode::OK, json!({ "name": name })))
  }
}

crate::route!(FilesRename);

/// Entries of one place copied or moved into a folder of the same place or of another.
pub struct FilesCopy;

impl Route for FilesCopy {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/api/files/copy";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let empty = Value::Null;
    let from = body.get("from").unwrap_or(&empty);
    let to = body.get("to").unwrap_or(&empty);
    let (from_id, to_id) = (text(from, "place")?, text(to, "place")?);
    let target = Within::read(text(to, "path")?)?;
    let moving = body.get("move").and_then(Value::as_bool).unwrap_or(false);
    let conflict = Conflict::read(body.get("conflict").and_then(Value::as_str))?;
    let same = from_id == to_id;
    let mut sources = Vec::new();

    for path in from.get("paths").and_then(Value::as_array).into_iter().flatten() {
      let source = entry_path(path.as_str().unwrap_or_default())?;

      if same && target.names.starts_with(&source.names) {
        return Err(into_itself());
      }

      // Moved onto the folder it stands in, an entry stays as it is.
      if !(same && moving && source.split().is_some_and(|(folder, _)| folder == target)) {
        sources.push(source);
      }
    }

    let from_places = places_of(from_id, call.standing(), false).await?;
    let to_places = places_of(to_id, call.standing(), false).await?;
    let (from_place, to_place) = (from_places.place(from_id)?, to_places.place(to_id)?);
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
    let mut landed = Vec::new();

    for ((source, kind), landing) in sources.into_iter().zip(kinds).zip(landings) {
      let name = match landing {
        Landing::Skipped => continue,
        Landing::New(name) => name,
        Landing::Replacing(name, _) => {
          to_place.remove(&target.child(&name)).await?;
          name
        }
      };
      let arrived = target.child(&name);

      if same {
        from_place.copy_here(&source, &arrived, moving).await?;
      } else {
        carry(from_place, source.clone(), kind, to_place, arrived).await?;

        if moving {
          from_place.remove(&source).await?;
        }
      }

      landed.push(name);
    }

    forget_place_prefix(from_id);
    forget_place_prefix(to_id);

    Ok(answer(StatusCode::OK, json!({ "names": landed })))
  }
}

crate::route!(FilesCopy);

/// Entries of one place deleted, a folder with everything in it.
pub struct FilesDelete;

impl Route for FilesDelete {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/api/files/delete";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let id = text(&body, "place")?;
    let mut entries = Vec::new();

    for path in body.get("paths").and_then(Value::as_array).into_iter().flatten() {
      entries.push(entry_path(path.as_str().unwrap_or_default())?);
    }

    let places = places_of(id, call.standing(), false).await?;
    let place = places.place(id)?;

    for within in &entries {
      place.remove(within).await?;
    }

    forget_place_prefix(id);

    Ok(answer(StatusCode::OK, json!({})))
  }
}

crate::route!(FilesDelete);
