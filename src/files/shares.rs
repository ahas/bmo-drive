//! The share links of the file viewer's entries (`docs/files.md`, 「Share links」): the rows of the store's
//! share table, which this module alone reads and writes for the routes and the command alike, the expiry
//! judged at every opening, and what a link hands out.
//!
//! A link is a row rather than a signature: it is listed, read back and taken away before its time.
//! The row, never the request, names the place and the path, so a token reaches the one entry it was
//! written for.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::Response;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::sqlite::{SqliteArguments, SqliteRow};
use sqlx::{Row, Sqlite, SqlitePool};

use super::change::kind_of;
use super::zip::{entry_name, zip_answer};
use super::{
  Kind, Place, Span, Within, asked_range, is_folder, missing_place, not_found, places_of, read_answer,
};
use crate::http::{Call, Refused, Route, Standing, answer};

// How many random bytes a token carries.
const TOKEN_BYTES: usize = 32;

/// What a link hands out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ShareKind {
  File,
  Folder,
}

impl ShareKind {
  fn text(self) -> &'static str {
    match self {
      Self::File => "file",
      Self::Folder => "folder",
    }
  }

  fn read(text: &str) -> Option<Self> {
    match text {
      "file" => Some(Self::File),
      "folder" => Some(Self::Folder),
      _ => None,
    }
  }
}

/// One live link, as the administrator's screen and the command read it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Share {
  pub token: String,
  pub place: String,
  pub path: String,
  pub kind: ShareKind,
  pub created_by: String,
  /// Unix seconds.
  pub created_at: i64,
  /// When the link stops opening, and none where it never does.
  pub expires_at: Option<i64>,
}

impl Share {
  fn of_row(row: &SqliteRow) -> Result<Self, sqlx::Error> {
    let kind: String = row.try_get("kind")?;

    Ok(Self {
      token: row.try_get("token")?,
      place: row.try_get("place")?,
      path: row.try_get("path")?,
      kind: ShareKind::read(&kind)
        .ok_or_else(|| sqlx::Error::Decode(format!("no kind of link: {kind}").into()))?,
      created_by: row.try_get("created_by")?,
      created_at: row.try_get("created_at")?,
      expires_at: row.try_get("expires_at")?,
    })
  }

  /// Whether its time has passed at `now`.
  fn expired(&self, now: i64) -> bool {
    self.expires_at.is_some_and(|at| at <= now)
  }

  /// The name its entry is handed out under.
  pub fn name(&self) -> String {
    entry_name(&self.place, &Within::read(&self.path).unwrap_or_default())
  }

  /// The link as a screen lists it, with the name its entry is handed out under.
  pub fn view(&self) -> Value {
    let mut view = json!(self);

    view["name"] = json!(self.name());
    view
  }
}

/// A statement reading links, its conditions and order after the table.
macro_rules! select {
  ($rest:literal) => {
    concat!(
      "SELECT share_token AS token, place, path, kind, created_by, created_at, expires_at FROM file_shares ",
      $rest
    )
  };
}

/// A statement over the store with its values bound.
type Statement = sqlx::query::Query<'static, Sqlite, SqliteArguments<'static>>;

fn store_failed(error: sqlx::Error) -> Refused {
  Refused::new(
    StatusCode::SERVICE_UNAVAILABLE,
    "store_failed",
    format!("the share store could not be read or written: {error}"),
  )
}

/// The links a statement reads.
async fn read(store: &SqlitePool, statement: Statement) -> Result<Vec<Share>, Refused> {
  let rows = statement.fetch_all(store).await.map_err(store_failed)?;

  rows.iter().map(Share::of_row).collect::<Result<_, _>>().map_err(store_failed)
}

/// How many rows a statement changed.
async fn write(store: &SqlitePool, statement: Statement) -> Result<u64, Refused> {
  Ok(statement.execute(store).await.map_err(store_failed)?.rows_affected())
}

fn now() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|since| since.as_secs() as i64)
    .unwrap_or_default()
}

/// A token drawn again until it opens on anything but a dash, which a command line handed the token
/// reads as an option (`docs/files.md`, 「Share links」); one draw in sixty-four is redrawn.
fn new_token() -> String {
  let mut bytes = [0_u8; TOKEN_BYTES];

  loop {
    rand::rng().fill_bytes(&mut bytes);

    let token = URL_SAFE_NO_PAD.encode(bytes);

    if !token.starts_with('-') {
      return token;
    }
  }
}

/// Links whose time has passed, gone. Every listing and making sweeps; nothing relies on it having
/// run, since a link is refused by its own expiry being read.
async fn sweep(store: &SqlitePool, now: i64) -> Result<(), Refused> {
  let statement =
    sqlx::query("DELETE FROM file_shares WHERE expires_at IS NOT NULL AND expires_at <= $1").bind(now);

  write(store, statement).await.map(|_| ())
}

/// Every live link, the newest first, links whose time has passed swept first.
pub async fn listed(store: &SqlitePool) -> Result<Vec<Share>, Refused> {
  sweep(store, now()).await?;
  read(store, sqlx::query(select!("ORDER BY created_at DESC, place, path"))).await
}

async fn of_entry(store: &SqlitePool, place: String, path: String) -> Result<Option<Share>, Refused> {
  let statement = sqlx::query(select!("WHERE place = $1 AND path = $2")).bind(place).bind(path);

  Ok(read(store, statement).await?.pop())
}

/// The row one token names, as it stands: not swept first, so a link whose time has passed is told
/// apart from an address that never named one.
async fn of_token(store: &SqlitePool, token: String) -> Result<Option<Share>, Refused> {
  Ok(read(store, sqlx::query(select!("WHERE share_token = $1")).bind(token)).await?.pop())
}

/// The link of one entry: the one that stands, kept as it was made, or this one. The table holds one
/// row per entry, so the writing and the looking are one step each and two askings at once hand back
/// one link; a link is never quietly replaced while somebody may still be handing it out.
async fn kept_or_made(store: &SqlitePool, share: Share) -> Result<Share, Refused> {
  let statement = sqlx::query(
    "INSERT INTO file_shares (share_token, place, path, kind, created_by, created_at, expires_at)
     VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (place, path) DO NOTHING",
  )
  .bind(share.token)
  .bind(share.place.clone())
  .bind(share.path.clone())
  .bind(share.kind.text())
  .bind(share.created_by)
  .bind(share.created_at)
  .bind(share.expires_at);

  write(store, statement).await?;
  of_entry(store, share.place, share.path)
    .await?
    .ok_or_else(|| Refused::failed("the link was neither kept nor made"))
}

/// One link taken away by its token, at once; whether there was one.
pub async fn remove(store: &SqlitePool, token: String) -> Result<bool, Refused> {
  let statement = sqlx::query("DELETE FROM file_shares WHERE share_token = $1").bind(token);

  Ok(write(store, statement).await? > 0)
}

/// The link a holder's token names, refused where it opens nothing: never named, or its time passed.
async fn opened(store: &SqlitePool, token: String) -> Result<Share, Refused> {
  let share = if token.is_empty() { None } else { of_token(store, token).await? };

  match share {
    None => Err(Refused::new(StatusCode::NOT_FOUND, "share_not_found", "no such share link")),
    Some(share) if share.expired(now()) => {
      Err(Refused::new(StatusCode::GONE, "share_expired", "the share link's time has passed"))
    }
    Some(share) => Ok(share),
  }
}

/// The entry at `within` as it stands now: a file or a folder, and a file's length. Anything else, or
/// nothing there, is refused as the file routes refuse it.
async fn standing_entry(place: &dyn Place, within: &Within) -> Result<(ShareKind, Option<u64>), Refused> {
  let kind = if within.last().is_none() { Kind::Folder } else { kind_of(place, within).await? };

  match kind {
    Kind::File => Ok((ShareKind::File, Some(place.open(within, Span::head(0)).await?.size))),
    Kind::Folder => {
      place.read_folder(within).await?;

      Ok((ShareKind::Folder, None))
    }
    Kind::Other => Err(Refused::new(
      StatusCode::BAD_REQUEST,
      "not_shareable",
      "only a file or a folder can be shared",
    )),
  }
}

/// The link of the entry at `path` of `place`: the one standing for it, or one made now for `seconds`,
/// or with no end. The entry is read first and judged a file or a folder there, before any row is
/// written.
pub async fn make(
  standing: &Standing,
  place: String,
  path: &str,
  seconds: Option<i64>,
  by: String,
) -> Result<Share, Refused> {
  let within = Within::read(path)?;
  let places = places_of(&place, standing, false).await?;
  let (kind, _) = standing_entry(places.place(&place)?, &within).await?;
  let at = now();
  let store = standing.store.pool();

  sweep(store, at).await?;
  kept_or_made(
    store,
    Share {
      token: new_token(),
      place,
      path: within.joined(),
      kind,
      created_by: by,
      created_at: at,
      expires_at: seconds.map(|seconds| at.saturating_add(seconds)),
    },
  )
  .await
}

/// How long a span key lets a link live, in seconds, and none for no end; refused for a key that is
/// no span.
fn span_seconds(span: Option<&str>) -> Result<Option<i64>, Refused> {
  const HOUR: i64 = 60 * 60;

  match span {
    Some("1h") => Ok(Some(HOUR)),
    Some("6h") => Ok(Some(6 * HOUR)),
    Some("24h") => Ok(Some(24 * HOUR)),
    Some("7d") => Ok(Some(7 * 24 * HOUR)),
    Some("30d") => Ok(Some(30 * 24 * HOUR)),
    Some("forever") => Ok(None),
    _ => Err(Refused::new(StatusCode::BAD_REQUEST, "bad_expiry", "the span is not one of the six")),
  }
}

/// The token a route's path names.
async fn path_token(call: &mut Call) -> Result<String, Refused> {
  Ok(call.path_params().await?.remove("token").unwrap_or_default())
}

/// Every live link, the newest first.
pub struct FileShares;

impl Route for FileShares {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/api/files/shares";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let shares = listed(call.standing().store.pool()).await?;

    Ok(answer(
      StatusCode::OK,
      json!({ "shares": shares.iter().map(Share::view).collect::<Vec<_>>() }),
    ))
  }
}

crate::route!(FileShares);

/// The link of one entry, living as long as the span says, made by whom the console names.
pub struct FileShareMake;

impl Route for FileShareMake {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/api/files/shares";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let body = call.body().await?;
    let text = |name: &str| body.get(name).and_then(Value::as_str).map(str::to_owned);
    let seconds = span_seconds(text("span").as_deref())?;
    let place = text("place").filter(|place| !place.is_empty()).ok_or_else(missing_place)?;
    let by = text("by")
      .filter(|name| !name.trim().is_empty())
      .ok_or_else(|| Refused::new(StatusCode::BAD_REQUEST, "bad_maker", "no maker was sent"))?;
    let share = make(call.standing(), place, &text("path").unwrap_or_default(), seconds, by).await?;

    Ok(answer(StatusCode::OK, json!({ "share": share.view() })))
  }
}

crate::route!(FileShareMake);

/// One link taken away by its token; it stops at once.
pub struct FileShareRemove;

impl Route for FileShareRemove {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/api/files/shares/{token}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let token = path_token(&mut call).await?;
    let removed = remove(call.standing().store.pool(), token).await?;

    Ok(answer(StatusCode::OK, json!({ "removed": removed })))
  }
}

crate::route!(FileShareRemove);

/// What a link's holder is shown: the entry's name and kind, a file's size and when the link ends,
/// read from the entry as it stands now. A link runs out, so no cache keeps the answer.
pub struct FileShared;

impl Route for FileShared {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/api/share/files/{token}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let token = path_token(&mut call).await?;
    let share = opened(call.standing().store.pool(), token).await?;
    let within = Within::read(&share.path)?;
    let places = places_of(&share.place, call.standing(), false).await?;
    let (kind, size) = standing_entry(places.place(&share.place)?, &within).await?;

    if kind != share.kind {
      return Err(Refused::new(
        StatusCode::NOT_FOUND,
        "shared_entry_gone",
        "the shared entry no longer stands at its path",
      ));
    }

    let mut response = answer(
      StatusCode::OK,
      json!({ "name": share.name(), "kind": share.kind, "size": size, "expiresAt": share.expires_at }),
    );

    response
      .headers_mut()
      .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));

    Ok(response)
  }
}

crate::route!(FileShared);

/// What a link hands out: the file as a download with its byte range answered, so a broken download
/// resumes, or the folder as its zip.
pub struct FileSharedDownload;

impl Route for FileSharedDownload {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/api/share/files/{token}/download";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let token = path_token(&mut call).await?;
    let share = opened(call.standing().store.pool(), token).await?;
    let within = Within::read(&share.path)?;
    let mut response = match share.kind {
      ShareKind::Folder => zip_answer(call.standing(), share.place, within).await?,
      ShareKind::File => {
        if within.last().is_none() {
          return Err(is_folder());
        }

        let range = asked_range(&call);
        let places = places_of(&share.place, call.standing(), false).await?;

        read_answer(
          places.place(&share.place)?,
          &within,
          range.unwrap_or(Span::WHOLE),
          range.is_some(),
          true,
        )
        .await?
      }
    };

    response
      .headers_mut()
      .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));

    Ok(response)
  }
}

crate::route!(FileSharedDownload);

/// The volume of this machine a real path lies on, as the file viewer walks it, and the path inside it:
/// the deepest mount holding it.
pub async fn volume_of(standing: &Standing, real: &str) -> Result<(String, String), Refused> {
  let machine = super::Machine::current(true, standing).await?;
  let holding = machine
    .drives
    .iter()
    .flat_map(|drive| drive.volumes.iter())
    .filter(|volume| {
      let mount = &volume.mount;

      real == mount || real.starts_with(&if mount == "/" { "/".to_owned() } else { format!("{mount}/") })
    })
    .max_by_key(|volume| volume.mount.len())
    .ok_or_else(not_found)?;

  Ok((holding.place.clone(), real[holding.mount.len()..].trim_start_matches('/').to_owned()))
}
