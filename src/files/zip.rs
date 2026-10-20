//! Entries handed over as one zip (`docs/files.md`, 「Reading a file」): everything under them walked by
//! the place's own readings and streamed into the archive as it is read, so neither a file nor the zip is
//! ever held whole.

use std::io;
use std::sync::Arc;

use async_zip::base::write::ZipFileWriter;
use async_zip::{AttributeCompatibility, Compression, ZipDateTime, ZipDateTimeBuilder, ZipEntryBuilder};
use axum::body::Body;
use axum::http::{HeaderValue, Method, header};
use axum::response::Response;
use chrono::{DateTime, Datelike, Local, Timelike, Utc};
use futures_util::{AsyncWriteExt, StreamExt, future, stream};
use tokio::io::DuplexStream;
use tokio::sync::oneshot;
use tokio_util::io::ReaderStream;

use super::change::kind_of;
use super::{
  Kind, OUTSIDE_PLACE, Opened, Place, Places, Span, Within, disposition, header_value, order, path_place,
  places_of, shown,
};
use crate::http::{Call, Refused, Route, Standing};
use crate::wire::ArchiveRequest;

// How much of the zip stands written ahead of the receiver. Enough to keep a file's reading moving
// while the answer is sent; small enough that a slow receiver holds back the reading rather than
// memory filling with what it has not taken.
const PIPE: usize = 256 * 1024;

/// The name an entry is handed out under: its own, and at a place's top the place's last name.
pub(super) fn entry_name(place: &str, within: &Within) -> String {
  within.last().map(str::to_owned).unwrap_or_else(|| {
    place
      .split(['/', ':', '\\'])
      .rfind(|part| !part.is_empty())
      .unwrap_or("files")
      .to_owned()
  })
}

/// The name the zip is saved under: the one entry's own name, or the name of the folder the first of
/// several stands in, as it is handed out.
fn zip_name(place: &str, roots: &[Root]) -> String {
  let named = match roots {
    [one] => one.within.clone(),
    _ => roots
      .first()
      .and_then(|first| first.within.split())
      .map(|(folder, _)| folder)
      .unwrap_or_default(),
  };

  format!("{}.zip", entry_name(place, &named))
}

/// One entry the zip is asked for, and the path it takes in the zip: a folder asked alone stands as its
/// entries, with no path of its own.
struct Root {
  within: Within,
  kind: Kind,
  /// Its own entry's path in the zip; none for a folder asked alone.
  path: Option<String>,
}

// The modes an unpacked entry is given: a file anyone may read and its owner write, a folder anyone
// may enter. A zip carrying none leaves what it unpacks readable by nobody.
const FILE_MODE: u16 = 0o100_644;
const FOLDER_MODE: u16 = 0o040_755;

/// One entry of the zip, stored as it stands, with its mode, and stamped with when it last changed
/// where that is known.
fn zip_entry(path: &str, kind: Kind, modified: Option<i64>) -> ZipEntryBuilder {
  let entry = ZipEntryBuilder::new(path.to_owned().into(), Compression::Stored)
    .attribute_compatibility(AttributeCompatibility::Unix)
    .unix_permissions(if kind == Kind::Folder { FOLDER_MODE } else { FILE_MODE });

  match modified.and_then(DateTime::from_timestamp_millis) {
    Some(at) => entry.last_modification_date(local_stamp(at)),
    None => entry,
  }
}

/// A moment as a zip writes it: the wall clock of this machine, with no zone, which is how every
/// unpacker reads it back.
fn local_stamp(at: DateTime<Utc>) -> ZipDateTime {
  let local = at.with_timezone(&Local);

  ZipDateTimeBuilder::new()
    .year(local.year())
    .month(local.month())
    .day(local.day())
    .hour(local.hour())
    .minute(local.minute())
    .second(local.second())
    .build()
}

/// Whether a reading was refused for leading out of the folder: a link to somewhere else, which the
/// zip leaves out as the fence leaves it out, rather than a failure that cuts the zip short.
fn fenced_out(refused: &Refused) -> bool {
  refused.code == OUTSIDE_PLACE
}

/// A folder still to be walked: where it stands, the path its entries take in the zip, and when it
/// last changed. The top has no entry of its own.
struct Unwalked {
  within: Within,
  prefix: String,
  modified: Option<i64>,
}

/// Every root written into `pipe`: a file stored as it stands under its own name, a folder walked under
/// its path. The pipe is let go as this ends, so an error here leaves the receiver with an archive that
/// never closed.
async fn write_zip(
  places: Arc<dyn Places>,
  id: String,
  roots: Vec<Root>,
  pipe: DuplexStream,
) -> Result<(), String> {
  let place = places.place(&id).map_err(|refused| refused.error)?;
  let mut zip = ZipFileWriter::with_tokio(pipe);

  for root in roots {
    match (root.kind, root.path) {
      (Kind::File, Some(path)) => {
        let opened = place
          .open(&root.within, Span::WHOLE)
          .await
          .map_err(|refused| format!("{path}: {}", refused.error))?;

        write_file(&mut zip, opened, &path, None).await?;
      }
      (Kind::Folder, path) => {
        let prefix = path.map(|path| format!("{path}/")).unwrap_or_default();

        write_folder(&mut zip, place, &root.within, prefix).await?;
      }
      _ => {}
    }
  }

  zip.close().await.map_err(|why| why.to_string())?;

  Ok(())
}

/// A file opened whole streamed into the zip under `path`.
async fn write_file<W: futures_util::AsyncWrite + Unpin>(
  zip: &mut ZipFileWriter<W>,
  opened: Opened,
  path: &str,
  modified: Option<i64>,
) -> Result<(), String> {
  let mut writing = zip
    .write_entry_stream(zip_entry(path, Kind::File, modified))
    .await
    .map_err(|why| why.to_string())?;
  let mut body = opened.body.into_data_stream();

  while let Some(chunk) = body.next().await {
    let chunk = chunk.map_err(|why| format!("{path}: {why}"))?;

    writing.write_all(&chunk).await.map_err(|why| why.to_string())?;
  }

  writing.close().await.map_err(|why| why.to_string())
}

/// Every entry under the folder at `top` written into the zip, depth first, each folder's own entry before
/// what it holds and each file stored as it stands under its path from the folder, after `prefix`. The
/// folder is walked as a place of its own, cut down to it, so nothing outside it is read into the zip
/// however a link inside it leads.
async fn write_folder<W: futures_util::AsyncWrite + Unpin>(
  zip: &mut ZipFileWriter<W>,
  place: &dyn Place,
  top: &Within,
  prefix: String,
) -> Result<(), String> {
  let place = place.beneath(top).await.map_err(|refused| refused.error)?;
  let mut folders = vec![Unwalked {
    within: Within::default(),
    prefix,
    modified: None,
  }];

  while let Some(pending) = folders.pop() {
    let mut entries = match place.read_folder(&pending.within).await {
      Ok(entries) => entries,
      Err(refused) if fenced_out(&refused) => continue,
      Err(refused) => return Err(format!("{}: {}", pending.prefix, refused.error)),
    };

    if !pending.prefix.is_empty() {
      zip
        .write_entry_whole(zip_entry(&pending.prefix, Kind::Folder, pending.modified), &[])
        .await
        .map_err(|why| why.to_string())?;
    }

    entries.sort_by(|one, other| order::natural(&one.name, &other.name));

    // The stack takes the last pushed first, so the folders go on it backwards to be walked in order.
    let mut inner = Vec::new();

    for entry in entries {
      let path = format!("{}{}", pending.prefix, entry.name);

      match entry.kind {
        Kind::Folder => inner.push(Unwalked {
          within: pending.within.child(&entry.name),
          prefix: format!("{path}/"),
          modified: entry.modified,
        }),
        Kind::File => {
          let opened = match place.open(&pending.within.child(&entry.name), Span::WHOLE).await {
            Ok(opened) => opened,
            Err(refused) if fenced_out(&refused) => continue,
            Err(refused) => return Err(format!("{path}: {}", refused.error)),
          };

          write_file(zip, opened, &path, entry.modified).await?;
        }
        Kind::Other => {}
      }
    }

    folders.extend(inner.into_iter().rev());
  }

  Ok(())
}

/// The named entries as one zip, streamed as they are read and kept nowhere.
pub struct ArchivesCreate;

impl Route for ArchivesCreate {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/places/{placeName}/archives";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let id = path_place(&mut call).await?;
    let asked: ArchiveRequest = call.body().await?;
    let mut paths = Vec::new();

    for path in &asked.paths {
      paths.push(Within::read(path)?);
    }

    zip_answer(call.standing(), id, paths).await
  }
}

crate::route!(ArchivesCreate);

/// The entries at `paths` of the place `id` answered as one zip, streamed as it is written.
pub(super) async fn zip_answer(
  standing: &Standing,
  id: String,
  paths: Vec<Within>,
) -> Result<Response, Refused> {
  let places = places_of(&id, standing, false).await?;

  if paths.is_empty() {
    return Err(Refused::bad_request("no entry was named"));
  }

  // What the paths name is settled before the answer starts, so a missing path or a place that cannot be
  // read is refused as any reading is rather than as a zip cut short.
  let mut roots = Vec::new();

  {
    let place = places.place(&id)?;
    let alone = paths.len() == 1;

    for within in paths {
      let kind = kind_of(place, &within).await?;
      let path = (!alone || kind != Kind::Folder).then(|| entry_name(&id, &within));

      roots.push(Root { within, kind, path });
    }

    let mut named = std::collections::BTreeSet::new();

    if roots
      .iter()
      .filter_map(|root| root.path.as_ref())
      .any(|path| !named.insert(path.clone()))
    {
      return Err(Refused::rule("name-repeated", "two of the named entries share a name"));
    }
  }

  let name = zip_name(&id, &roots);
  let (pipe, out) = tokio::io::duplex(PIPE);
  let (ended, ending) = oneshot::channel();

  tokio::spawn(async move {
    let _ = ended.send(write_zip(places, id, roots, pipe).await);
  });

  // The zip is read off the pipe until the writer lets it go; one that ended on an error turns the
  // end of the answer into an error, so the receiver holds a broken download rather than a zip
  // quietly missing what failed.
  let failure = stream::once(async move {
    match ending.await {
      Ok(Ok(())) => None,
      Ok(Err(why)) => Some(Err(io::Error::other(why))),
      Err(_) => Some(Err(io::Error::other("the zip stopped being written"))),
    }
  })
  .filter_map(future::ready);
  let mut response = Response::new(Body::from_stream(ReaderStream::new(out).chain(failure)));
  let headers = response.headers_mut();

  headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/zip"));
  headers.insert(header::CONTENT_DISPOSITION, header_value(&disposition(&name, true)));
  headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("none"));
  shown(headers);

  Ok(response)
}
