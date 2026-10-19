//! A folder handed over as one zip (`docs/files.md`, 「Reading a file」): everything under it walked by
//! the place's own readings and streamed into the archive as it is read, so neither a file nor the
//! zip is ever held whole.

use std::io;
use std::sync::Arc;

use async_zip::base::write::ZipFileWriter;
use async_zip::{AttributeCompatibility, Compression, ZipDateTime, ZipDateTimeBuilder, ZipEntryBuilder};
use axum::body::Body;
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::Response;
use chrono::{DateTime, Datelike, Local, Timelike, Utc};
use futures_util::{AsyncWriteExt, StreamExt, future, stream};
use tokio::io::DuplexStream;
use tokio::sync::oneshot;
use tokio_util::io::ReaderStream;

use super::change::kind_of;
use super::{
  Kind, OUTSIDE_PLACE, Places, Span, Within, disposition, header_value, named_place, order, places_of,
  shown,
};
use crate::http::{Call, Refused, Route, Standing};

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

/// The name the zip is saved under: the folder's own name, as it is handed out.
fn zip_name(place: &str, within: &Within) -> String {
  format!("{}.zip", entry_name(place, within))
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

/// Every entry under the folder at `top` written into `pipe`, depth first, each folder's own entry
/// before what it holds and each file stored as it stands under its path from the folder. The folder
/// is walked as a place of its own, cut down to it, so nothing outside it is read into the zip however
/// a link inside it leads. The pipe is let go as this ends, so an error here leaves the receiver with
/// an archive that never closed.
async fn write_zip(
  places: Arc<dyn Places>,
  id: String,
  top: Within,
  pipe: DuplexStream,
) -> Result<(), String> {
  let place = places
    .place(&id)
    .map_err(|refused| refused.error)?
    .beneath(&top)
    .await
    .map_err(|refused| refused.error)?;
  let mut zip = ZipFileWriter::with_tokio(pipe);
  let mut folders = vec![Unwalked {
    within: Within::default(),
    prefix: String::new(),
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
          let mut writing = zip
            .write_entry_stream(zip_entry(&path, Kind::File, entry.modified))
            .await
            .map_err(|why| why.to_string())?;
          let mut body = opened.body.into_data_stream();

          while let Some(chunk) = body.next().await {
            let chunk = chunk.map_err(|why| format!("{path}: {why}"))?;

            writing.write_all(&chunk).await.map_err(|why| why.to_string())?;
          }

          writing.close().await.map_err(|why| why.to_string())?;
        }
        Kind::Other => {}
      }
    }

    folders.extend(inner.into_iter().rev());
  }

  zip.close().await.map_err(|why| why.to_string())?;

  Ok(())
}

/// A folder's zip, streamed as its entries are read.
pub struct FilesZip;

impl Route for FilesZip {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/api/files/zip";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let id = named_place(&call)?;
    let within = Within::read(&call.query("path").unwrap_or_default())?;

    zip_answer(call.standing(), id, within).await
  }
}

crate::route!(FilesZip);

/// The folder at `within` of the place `id` answered as its zip, streamed as it is written.
pub(super) async fn zip_answer(
  standing: &Standing,
  id: String,
  within: Within,
) -> Result<Response, Refused> {
  let places = places_of(&id, standing, false).await?;

  // What the path names is settled before the answer starts, so a file, a missing path or a place
  // that cannot be read is refused as any reading is rather than as a zip cut short.
  {
    let place = places.place(&id)?;

    if within.last().is_some() && kind_of(place, &within).await? != Kind::Folder {
      return Err(Refused::new(StatusCode::BAD_REQUEST, "zip_of_file", "a file cannot be made a zip"));
    }
  }

  let name = zip_name(&id, &within);
  let (pipe, out) = tokio::io::duplex(PIPE);
  let (ended, ending) = oneshot::channel();

  tokio::spawn(async move {
    let _ = ended.send(write_zip(places, id, within, pipe).await);
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
