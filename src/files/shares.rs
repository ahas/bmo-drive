//! The share links of the file viewer's entries (`docs/files.md`, 「Share links」): the rows of the store's
//! share table, which this module alone reads and writes, the expiry judged at every opening, and what a
//! link hands out.
//!
//! A link is a row rather than a signature: it is listed, read back and taken away before its time.
//! The row, never the request, names the place and the path, so a token reaches the one entry it was
//! written for.

use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use sqlx::sqlite::{SqliteArguments, SqliteRow};
use sqlx::{Row, Sqlite, SqlitePool};

use super::change::kind_of;
use super::zip::{entry_name, zip_answer};
use super::{Kind, Place, Span, Within, asked_range, is_folder, places_of, read_answer};
use crate::events::now_ms;
use crate::http::{Call, Refused, Route, Standing, answer};
use crate::wire::{Share, ShareKey, ShareKind, SharePage, ShareRequest, SharesQuery};

// How many random bytes a token carries.
const TOKEN_BYTES: usize = 32;

// How many links a page carries where none is asked, and the most it may.
const PAGE: u64 = 100;
const PAGE_MOST: u64 = 1000;

fn kind_text(kind: ShareKind) -> &'static str {
  match kind {
    ShareKind::File => "file",
    ShareKind::Folder => "folder",
  }
}

fn kind_read(text: &str) -> Option<ShareKind> {
  match text {
    "file" => Some(ShareKind::File),
    "folder" => Some(ShareKind::Folder),
    _ => None,
  }
}

/// One row of the share table.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Link {
  token: String,
  place: String,
  path: String,
  kind: ShareKind,
  /// Who made it; empty where none was named.
  actor: String,
  created_at: i64,
  expires_at: Option<i64>,
}

impl Link {
  fn of_row(row: &SqliteRow) -> Result<Self, sqlx::Error> {
    let kind: String = row.try_get("kind")?;

    Ok(Self {
      token: row.try_get("token")?,
      place: row.try_get("place")?,
      path: row.try_get("path")?,
      kind: kind_read(&kind)
        .ok_or_else(|| sqlx::Error::Decode(format!("no kind of link: {kind}").into()))?,
      actor: row.try_get("created_by")?,
      created_at: row.try_get("created_at")?,
      expires_at: row.try_get("expires_at")?,
    })
  }

  /// Whether its time has passed at `now`.
  fn expired(&self, now: i64) -> bool {
    self.expires_at.is_some_and(|at| at <= now)
  }

  /// The link as a caller is told of it, its page under the share address `address`.
  fn share(&self, address: &str) -> Share {
    Share {
      token: self.token.clone(),
      place_name: self.place.clone(),
      path: self.path.clone(),
      name: entry_name(&self.place, &Within::read(&self.path).unwrap_or_default()),
      kind: self.kind,
      url: format!("{}/{}", address.trim_end_matches('/'), self.token),
      actor_name: Some(self.actor.clone()).filter(|actor| !actor.is_empty()),
      created_at: self.created_at,
      expires_at: self.expires_at,
    }
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
  Refused::failed(format!("the share store could not be read or written: {error}"))
}

/// The links a statement reads.
async fn read(store: &SqlitePool, statement: Statement) -> Result<Vec<Link>, Refused> {
  let rows = statement.fetch_all(store).await.map_err(store_failed)?;

  rows.iter().map(Link::of_row).collect::<Result<_, _>>().map_err(store_failed)
}

/// How many rows a statement changed.
async fn write(store: &SqlitePool, statement: Statement) -> Result<u64, Refused> {
  Ok(statement.execute(store).await.map_err(store_failed)?.rows_affected())
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

/// Links whose time has passed, gone, each told as an event. Every listing and making sweeps; nothing
/// relies on it having run, since a link is refused by its own expiry being read.
async fn sweep(standing: &Standing, now: i64) -> Result<(), Refused> {
  let store = standing.store.pool();
  let passed = read(
    store,
    sqlx::query(select!("WHERE expires_at IS NOT NULL AND expires_at <= $1")).bind(now),
  )
  .await?;

  for link in passed {
    if write(
      store,
      sqlx::query("DELETE FROM file_shares WHERE share_token = $1").bind(link.token.clone()),
    )
    .await?
      > 0
    {
      standing.events.publish("share.deleted", ShareKey { token: link.token }).await;
    }
  }

  Ok(())
}

async fn of_entry(store: &SqlitePool, place: String, path: String) -> Result<Option<Link>, Refused> {
  let statement = sqlx::query(select!("WHERE place = $1 AND path = $2")).bind(place).bind(path);

  Ok(read(store, statement).await?.pop())
}

/// The row one token names, as it stands: not swept first, so a link whose time has passed is told
/// apart from a token that never named one.
async fn of_token(store: &SqlitePool, token: String) -> Result<Option<Link>, Refused> {
  Ok(read(store, sqlx::query(select!("WHERE share_token = $1")).bind(token)).await?.pop())
}

/// The link of one entry: the one that stands, kept as it was made, or this one, and whether it was made
/// now. The table holds one row per entry, so the writing and the looking are one step each and two askings
/// at once hand back one link; a link is never quietly replaced while somebody may still be handing it out.
async fn kept_or_made(store: &SqlitePool, link: Link) -> Result<(Link, bool), Refused> {
  let statement = sqlx::query(
    "INSERT INTO file_shares (share_token, place, path, kind, created_by, created_at, expires_at)
     VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (place, path) DO NOTHING",
  )
  .bind(link.token)
  .bind(link.place.clone())
  .bind(link.path.clone())
  .bind(kind_text(link.kind))
  .bind(link.actor)
  .bind(link.created_at)
  .bind(link.expires_at);
  let made = write(store, statement).await? > 0;
  let standing = of_entry(store, link.place, link.path)
    .await?
    .ok_or_else(|| Refused::failed("the link was neither kept nor made"))?;

  Ok((standing, made))
}

/// The link a holder's token names, refused where it opens nothing: never named, or its time passed.
async fn opened(store: &SqlitePool, token: String) -> Result<Link, Refused> {
  let link = if token.is_empty() { None } else { of_token(store, token).await? };

  match link {
    None => Err(Refused::not_found("no such share link")),
    Some(link) if link.expired(now_ms()) => Err(Refused::gone("the share link's time has passed")),
    Some(link) => Ok(link),
  }
}

/// What the entry at `within` is now: a file or a folder. Anything else, or nothing there, is refused as
/// the file routes refuse it.
async fn standing_entry(place: &dyn Place, within: &Within) -> Result<ShareKind, Refused> {
  match kind_of(place, within).await? {
    Kind::File => {
      place.open(within, Span::head(0)).await?;

      Ok(ShareKind::File)
    }
    Kind::Folder => {
      place.read_folder(within).await?;

      Ok(ShareKind::Folder)
    }
    Kind::Other => Err(Refused::rule("not-shareable", "only a file or a folder can be shared")),
  }
}

/// The link a holder's token names, with its entry still standing as the kind it was shared as.
async fn holding(standing: &Standing, token: String) -> Result<Link, Refused> {
  let link = opened(standing.store.pool(), token).await?;
  let within = Within::read(&link.path)?;
  let places = places_of(&link.place, standing, false).await?;

  if standing_entry(places.place(&link.place)?, &within).await? != link.kind {
    return Err(Refused::not_found("the shared entry no longer stands at its path"));
  }

  Ok(link)
}

/// What nothing may keep, since a link runs out.
fn uncached(mut response: Response) -> Response {
  response
    .headers_mut()
    .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
  response
}

/// The live links, the newest first, links whose time has passed swept first.
pub struct SharesList;

impl Route for SharesList {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/shares";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let query: SharesQuery = call.query()?;
    let limit = query.limit.unwrap_or(PAGE).clamp(1, PAGE_MOST);
    let standing = call.standing();

    sweep(standing, now_ms()).await?;

    let statement =
      sqlx::query(select!("WHERE created_at < $1 ORDER BY created_at DESC, place, path LIMIT $2"))
        .bind(query.after.unwrap_or(i64::MAX))
        .bind(i64::try_from(limit + 1).unwrap_or(i64::MAX));
    let mut links = read(standing.store.pool(), statement).await?;
    let more = links.len() as u64 > limit;

    links.truncate(usize::try_from(limit).unwrap_or(usize::MAX));

    let address = &standing.settings.share.address;
    let items = links.iter().map(|link| link.share(address)).collect();

    Ok(answer(StatusCode::OK, SharePage { items, more }))
  }
}

crate::route!(SharesList);

/// The link of one entry, until the time asked or with no end: made now, or the live one already standing.
/// The entry is read first and judged a file or a folder there, before any row is written.
pub struct SharesCreate;

impl Route for SharesCreate {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/shares";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let asked: ShareRequest = call.body().await?;
    let at = now_ms();

    if asked.expires_at.is_some_and(|expires| expires <= at) {
      return Err(Refused::rule("bad-expiry", "the link's end has already passed"));
    }

    let standing = call.standing();
    let within = Within::read(&asked.path)?;
    let places = places_of(&asked.place_name, standing, false).await?;
    let kind = standing_entry(places.place(&asked.place_name)?, &within).await?;

    sweep(standing, at).await?;

    let (link, made) = kept_or_made(
      standing.store.pool(),
      Link {
        token: new_token(),
        place: asked.place_name,
        path: within.joined(),
        kind,
        actor: asked.actor_name.unwrap_or_default(),
        created_at: at,
        expires_at: asked.expires_at,
      },
    )
    .await?;
    let share = link.share(&standing.settings.share.address);

    if made {
      standing.events.publish("share.created", &share).await;
    }

    Ok(answer(if made { StatusCode::CREATED } else { StatusCode::OK }, share))
  }
}

crate::route!(SharesCreate);

/// A link and what it opens, read from its entry as it stands now.
pub struct ShareRead;

impl Route for ShareRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/shares/{token}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let token = call.path_param("token").await?;
    let link = holding(call.standing(), token).await?;

    Ok(uncached(answer(StatusCode::OK, link.share(&call.standing().settings.share.address))))
  }
}

crate::route!(ShareRead);

/// One link taken away by its token; it stops at once.
pub struct ShareDelete;

impl Route for ShareDelete {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/shares/{token}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let token = call.path_param("token").await?;
    let statement = sqlx::query("DELETE FROM file_shares WHERE share_token = $1").bind(token.clone());

    if write(call.standing().store.pool(), statement).await? == 0 {
      return Err(Refused::not_found("no such share link"));
    }

    call.standing().events.publish("share.deleted", ShareKey { token }).await;

    Ok(StatusCode::NO_CONTENT.into_response())
  }
}

crate::route!(ShareDelete);

/// What a link hands out: the file as a download with its byte range answered, so a broken download
/// resumes, or the folder as its zip.
pub struct ShareContent;

impl Route for ShareContent {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/shares/{token}/content";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let token = call.path_param("token").await?;
    let link = holding(call.standing(), token).await?;
    let within = Within::read(&link.path)?;
    let response = match link.kind {
      ShareKind::Folder => zip_answer(call.standing(), link.place, vec![within]).await?,
      ShareKind::File => {
        if within.last().is_none() {
          return Err(is_folder());
        }

        let range = asked_range(&call);
        let places = places_of(&link.place, call.standing(), false).await?;

        read_answer(
          places.place(&link.place)?,
          &within,
          range.unwrap_or(Span::WHOLE),
          range.is_some(),
          true,
        )
        .await?
      }
    };

    Ok(uncached(response))
  }
}

crate::route!(ShareContent);
