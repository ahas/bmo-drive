//! Sharing entries with members (`docs/files.md`, 「Sharing with members」): the rows of the store's member
//! share table, which this module alone reads and writes, the owner's granting and taking off, and a
//! member's readings of what is shared with them.
//!
//! The row, never the request, names the owner, the place and the path, so a member's reading reaches the
//! one entry it was written for, cut down to it, in the places of the owner who shared it, and only for the
//! key the row names as its member. Reading is all a member share opens.

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use sqlx::sqlite::{SqliteArguments, SqliteRow};
use sqlx::{Row, Sqlite, SqlitePool};

use super::change::kind_of;
use super::shares::{kind_read, kind_text, standing_entry};
use super::zip::entry_name;
use super::{
  Kind, Places, Span, Within, asked_range, is_folder, listed_page, not_folder, not_found, places_of,
  read_answer,
};
use crate::events::now_ms;
use crate::http::{Call, Refused, Route, Standing, answer};
use crate::owner::Owner;
use crate::wire::{
  EntriesQuery, MemberShare, MemberShareChange, MemberSharePage, MemberShareRequest, MemberSharesQuery,
  ShareKind, SharePermission, Shared, SharedContentQuery, SharedPage, SharedQuery,
};

// How many member shares of one entry a page carries where none is asked, how many shared with an owner,
// and the most either may.
const ENTRY_PAGE: u64 = 1000;
const SHARED_PAGE: u64 = 100;
const PAGE_MOST: u64 = 1000;

// The most members one asking shares with, and the longest key it takes, in characters, as an owner key.
const USERS_MOST: usize = 1000;
const KEY_MOST: usize = 200;

/// One row of the member share table.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Grant {
  id: i64,
  /// The owner key it is shared by.
  owner: String,
  place: String,
  path: String,
  kind: ShareKind,
  /// The member's owner key.
  member: String,
  created_at: i64,
}

impl Grant {
  fn of_row(row: &SqliteRow) -> Result<Self, sqlx::Error> {
    let kind: String = row.try_get("kind")?;

    Ok(Self {
      id: row.try_get("id")?,
      owner: row.try_get("owner")?,
      place: row.try_get("place")?,
      path: row.try_get("path")?,
      kind: kind_read(&kind)
        .ok_or_else(|| sqlx::Error::Decode(format!("no kind of member share: {kind}").into()))?,
      member: row.try_get("member")?,
      created_at: row.try_get("created_at")?,
    })
  }

  /// The share as its owner is told of it.
  fn member_share(&self) -> MemberShare {
    MemberShare {
      id: self.id,
      user: self.member.clone(),
      permission: SharePermission::Read,
      shared_at: self.created_at,
    }
  }

  /// The share as its member is told of it.
  fn shared(&self) -> Shared {
    Shared {
      id: self.id,
      owner_key: self.owner.clone(),
      name: entry_name(&self.place, &Within::read(&self.path).unwrap_or_default()),
      kind: self.kind,
      permission: SharePermission::Read,
      shared_at: self.created_at,
    }
  }
}

/// A statement reading member shares, its conditions and order after the table.
macro_rules! select {
  ($rest:literal) => {
    concat!("SELECT id, owner, place, path, kind, member, created_at FROM member_shares ", $rest)
  };
}

/// A statement over the store with its values bound.
type Statement = sqlx::query::Query<'static, Sqlite, SqliteArguments<'static>>;

fn store_failed(error: sqlx::Error) -> Refused {
  Refused::failed(format!("the share store could not be read or written: {error}"))
}

/// The member shares a statement reads.
async fn read(store: &SqlitePool, statement: Statement) -> Result<Vec<Grant>, Refused> {
  let rows = statement.fetch_all(store).await.map_err(store_failed)?;

  rows.iter().map(Grant::of_row).collect::<Result<_, _>>().map_err(store_failed)
}

fn no_share() -> Refused {
  Refused::not_found("no such member share")
}

/// Nothing where the permission is one the service serves; reading is all a member share opens.
fn permitted(permission: SharePermission) -> Result<(), Refused> {
  match permission {
    SharePermission::Read => Ok(()),
    SharePermission::Edit => Err(Refused::rule("read-only", "a member share opens reading alone")),
  }
}

/// The member keys an asking names, each once: refused where it names none or too many, or a key that is
/// empty, too long or the asking owner's own.
fn members_asked(users: Vec<String>, owner: &Owner) -> Result<BTreeSet<String>, Refused> {
  if users.is_empty() || users.len() > USERS_MOST {
    return Err(Refused::bad_request(format!("users names 1 to {USERS_MOST} members")));
  }

  if users.iter().any(|user| user.is_empty() || user.chars().count() > KEY_MOST) {
    return Err(Refused::bad_request(format!("a member key is 1 to {KEY_MOST} characters")));
  }

  if users.contains(&owner.key) {
    return Err(Refused::rule("own-share", "an owner shares with others alone"));
  }

  Ok(users.into_iter().collect())
}

/// The number a route's path names; a number that is none names no share.
async fn path_id(call: &mut Call) -> Result<i64, Refused> {
  call.path_param("id").await?.parse().map_err(|_| no_share())
}

/// The member share `id` of `owner`, refused as absent where it is none of theirs.
async fn owned(store: &SqlitePool, id: i64, owner: &Owner) -> Result<Grant, Refused> {
  let statement = sqlx::query(select!("WHERE id = $1 AND owner = $2")).bind(id).bind(owner.key.clone());

  read(store, statement).await?.pop().ok_or_else(no_share)
}

/// The members of one entry of an owner, in the order they were shared with.
pub struct MemberSharesList;

impl Route for MemberSharesList {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/member-shares";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let owner = call.owner()?;
    let query: MemberSharesQuery = call.query()?;
    let within = Within::read(&query.path)?;
    let limit = query.limit.unwrap_or(ENTRY_PAGE).clamp(1, PAGE_MOST);
    let statement = sqlx::query(select!(
      "WHERE owner = $1 AND place = $2 AND path = $3 AND id > $4 ORDER BY id LIMIT $5"
    ))
    .bind(owner.key)
    .bind(query.place_name)
    .bind(within.joined())
    .bind(query.after.unwrap_or(0))
    .bind(i64::try_from(limit + 1).unwrap_or(i64::MAX));
    let mut grants = read(call.standing().store.pool(), statement).await?;
    let more = grants.len() as u64 > limit;

    grants.truncate(usize::try_from(limit).unwrap_or(usize::MAX));

    let items = grants.iter().map(Grant::member_share).collect();

    Ok(answer(StatusCode::OK, MemberSharePage { items, more }))
  }
}

crate::route!(MemberSharesList);

/// One entry shared with members, read first and judged a file or a folder there before any row is
/// written; a member already standing keeps the row.
pub struct MemberSharesCreate;

impl Route for MemberSharesCreate {
  const METHOD: Method = Method::POST;
  const PATH: &'static str = "/member-shares";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let owner = call.owner()?;
    let asked: MemberShareRequest = call.body().await?;

    permitted(asked.permission)?;

    let members = members_asked(asked.users, &owner)?;
    let standing = call.standing();
    let within = Within::read(&asked.path)?;
    let places = places_of(&asked.place_name, &owner, standing, false).await?;
    let kind = standing_entry(places.place(&asked.place_name)?, &within).await?;
    let path = within.joined();
    let at = now_ms();
    let store = standing.store.pool();
    let mut writing = store.begin().await.map_err(store_failed)?;

    for member in &members {
      sqlx::query(
        "INSERT INTO member_shares (owner, place, path, kind, member, created_at)
         VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (owner, place, path, member) DO NOTHING",
      )
      .bind(owner.key.clone())
      .bind(asked.place_name.clone())
      .bind(path.clone())
      .bind(kind_text(kind))
      .bind(member.clone())
      .bind(at)
      .execute(&mut *writing)
      .await
      .map_err(store_failed)?;
    }

    writing.commit().await.map_err(store_failed)?;

    let statement = sqlx::query(select!("WHERE owner = $1 AND place = $2 AND path = $3 ORDER BY id"))
      .bind(owner.key)
      .bind(asked.place_name)
      .bind(path);
    let items = read(store, statement)
      .await?
      .iter()
      .filter(|grant| members.contains(&grant.member))
      .map(Grant::member_share)
      .collect();

    Ok(answer(StatusCode::OK, MemberSharePage { items, more: false }))
  }
}

crate::route!(MemberSharesCreate);

/// One member share's permission set: reading keeps it as it stands, and nothing else is served.
pub struct MemberShareUpdate;

impl Route for MemberShareUpdate {
  const METHOD: Method = Method::PATCH;
  const PATH: &'static str = "/member-shares/{id}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let owner = call.owner()?;
    let id = path_id(&mut call).await?;
    let asked: MemberShareChange = call.body().await?;
    let grant = owned(call.standing().store.pool(), id, &owner).await?;

    permitted(asked.permission)?;

    Ok(answer(StatusCode::OK, grant.member_share()))
  }
}

crate::route!(MemberShareUpdate);

/// One member taken off an entry of the asking owner; their reading stops at once.
pub struct MemberShareDelete;

impl Route for MemberShareDelete {
  const METHOD: Method = Method::DELETE;
  const PATH: &'static str = "/member-shares/{id}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let owner = call.owner()?;
    let id = path_id(&mut call).await?;
    let statement = sqlx::query("DELETE FROM member_shares WHERE id = $1 AND owner = $2")
      .bind(id)
      .bind(owner.key);
    let removed = statement.execute(call.standing().store.pool()).await.map_err(store_failed)?;

    if removed.rows_affected() == 0 {
      return Err(no_share());
    }

    Ok(StatusCode::NO_CONTENT.into_response())
  }
}

crate::route!(MemberShareDelete);

/// What is shared with the asking owner, the newest first.
pub struct SharedList;

impl Route for SharedList {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/shared";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let owner = call.owner()?;
    let query: SharedQuery = call.query()?;
    let limit = query.limit.unwrap_or(SHARED_PAGE).clamp(1, PAGE_MOST);
    let statement = sqlx::query(select!("WHERE member = $1 AND id < $2 ORDER BY id DESC LIMIT $3"))
      .bind(owner.key)
      .bind(query.after.unwrap_or(i64::MAX))
      .bind(i64::try_from(limit + 1).unwrap_or(i64::MAX));
    let mut grants = read(call.standing().store.pool(), statement).await?;
    let more = grants.len() as u64 > limit;

    grants.truncate(usize::try_from(limit).unwrap_or(usize::MAX));

    let items = grants.iter().map(Grant::shared).collect();

    Ok(answer(StatusCode::OK, SharedPage { items, more }))
  }
}

crate::route!(SharedList);

/// The member share `id` whose member is `member`, with the places of the owner who shared it and its entry's
/// path there, the entry still standing as the kind it was shared as. To any other key it is absent.
async fn granted(
  standing: &Standing,
  id: i64,
  member: &Owner,
) -> Result<(Grant, Arc<dyn Places>, Within), Refused> {
  let statement = sqlx::query(select!("WHERE id = $1 AND member = $2")).bind(id).bind(member.key.clone());
  let grant = read(standing.store.pool(), statement).await?.pop().ok_or_else(no_share)?;
  let sharer = Owner::of_key(grant.owner.clone(), &standing.settings);
  let top = Within::read(&grant.path)?;
  let places = places_of(&grant.place, &sharer, standing, false).await?;
  let kind = match kind_of(places.place(&grant.place)?, &top).await? {
    Kind::File => Some(ShareKind::File),
    Kind::Folder => Some(ShareKind::Folder),
    Kind::Other => None,
  };

  if kind != Some(grant.kind) {
    return Err(Refused::not_found("the shared entry no longer stands at its path"));
  }

  Ok((grant, places, top))
}

/// A folder under a shared folder, listed from the shared folder cut down to it, read now every time.
pub struct SharedEntries;

impl Route for SharedEntries {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/shared/{id}/entries";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let owner = call.owner()?;
    let id = path_id(&mut call).await?;
    let query: EntriesQuery = call.query()?;
    let within = Within::read(query.path.as_deref().unwrap_or_default())?;
    let (grant, places, top) = granted(call.standing(), id, &owner).await?;

    if grant.kind == ShareKind::File {
      return Err(not_folder());
    }

    let folder = places.place(&grant.place)?.beneath(&top).await?;
    let entries = folder.read_folder(&within).await?;

    Ok(answer(StatusCode::OK, listed_page(&entries, &query, &format!("shared:{id}"), &within)))
  }
}

crate::route!(SharedEntries);

/// A shared file, or a file under a shared folder read from the folder cut down to it, streamed as any
/// file's reading is.
pub struct SharedContent;

impl Route for SharedContent {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/shared/{id}/content";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let owner = call.owner()?;
    let id = path_id(&mut call).await?;
    let query: SharedContentQuery = call.query()?;
    let within = Within::read(query.path.as_deref().unwrap_or_default())?;
    let range = asked_range(&call);
    let span = range.or(query.head.map(Span::head)).unwrap_or(Span::WHOLE);
    let download = query.download == Some(true);
    let (grant, places, top) = granted(call.standing(), id, &owner).await?;
    let place = places.place(&grant.place)?;

    match grant.kind {
      ShareKind::File if within.last().is_some() => Err(not_found()),
      ShareKind::File => read_answer(place, &top, span, range.is_some(), download).await,
      ShareKind::Folder if within.last().is_none() => Err(is_folder()),
      ShareKind::Folder => {
        let folder = place.beneath(&top).await?;

        read_answer(&*folder, &within, span, range.is_some(), download).await
      }
    }
  }
}

crate::route!(SharedContent);
