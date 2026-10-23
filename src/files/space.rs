//! The BMO drive (`docs/files.md`, 「The BMO drive」): one personal space for every owner under the storage
//! location the settings name, each owner reaching its own alone as the place `drive`, its quota judged
//! before bytes arrive; and the routes that say what an owner may reach, where the drive stands, and each
//! owner's quota (`docs/api.md`, 「Owners」).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::http::{Method, StatusCode};
use axum::response::Response;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use sqlx::Row;

use super::drives::Volume;
use super::guard::Guard;
use super::{Place, Places, Within, free_bytes, io_refusal, no_place};
use crate::events::now_ms;
use crate::http::{Call, Refused, Route, Standing, answer};
use crate::owner::Owner;
use crate::settings::{Settings, StorageKind};
use crate::wire::{self, Quota, QuotaRequest};

/// The name every owner reaches its own space by.
pub(super) const SPACE: &str = "drive";

/// The code of a drive whose storage cannot be opened.
const UNREADY: &str = "storage-unready";

// Every byte of an owner key but an ASCII letter, a digit, `-` and `_` is written as `%` and two hex digits
// in its space's folder name, so the name is the key's alone and never climbs out.
const FOLDER: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'_');

// The longest name a folder may carry.
const FOLDER_MOST: usize = 255;

/// The name of the folder an owner's space stands in under the root.
fn folder_name(key: &str) -> Result<String, Refused> {
  let name = utf8_percent_encode(key, FOLDER).to_string();

  if name.len() > FOLDER_MOST {
    return Err(Refused::bad_request("the owner key is too long to name a space's folder"));
  }

  Ok(name)
}

/// The root folder of the spaces, or why the storage cannot be opened.
fn root(settings: &Settings) -> Result<PathBuf, String> {
  let storage = &settings.storage;

  match storage.kind {
    StorageKind::Local if storage.root.is_empty() => {
      Err("no root folder is set for the drive's storage".into())
    }
    StorageKind::Local => Ok(PathBuf::from(&storage.root)),
    StorageKind::Node => Err("a node location for the drive's storage has no adapter yet".into()),
    StorageKind::S3 => Err("an s3 location for the drive's storage has no adapter yet".into()),
  }
}

fn unready(reason: &str) -> Refused {
  Refused::rule(UNREADY, reason)
}

/// The folder an owner's space stands in, or why the storage cannot be opened.
fn space_folder(settings: &Settings, key: &str) -> Result<Result<PathBuf, String>, Refused> {
  let name = folder_name(key)?;

  Ok(root(settings).map(|root| root.join(name)))
}

/// The bytes of the files under `path`, as it stands on the disk: no link is followed, and a folder that is
/// not there holds none.
fn bytes_under(path: &Path) -> u64 {
  let Ok(meta) = fs::symlink_metadata(path) else {
    return 0;
  };

  if meta.is_file() {
    return meta.len();
  }

  if !meta.is_dir() {
    return 0;
  }

  fs::read_dir(path)
    .into_iter()
    .flatten()
    .flatten()
    .map(|one| bytes_under(&one.path()))
    .sum()
}

/// The bytes of the files in the space at `folder`, counted off the answering threads.
async fn usage(folder: PathBuf) -> Result<u64, Refused> {
  tokio::task::spawn_blocking(move || bytes_under(&folder)).await.map_err(Refused::failed)
}

/// The quota set for `key`, where one was set.
async fn quota_set(standing: &Standing, key: &str) -> Result<Option<u64>, Refused> {
  let row = sqlx::query("SELECT quota_bytes FROM quotas WHERE owner = $1")
    .bind(key)
    .fetch_optional(standing.store.pool())
    .await
    .map_err(|why| Refused::failed(format!("the quotas could not be read: {why}")))?;

  row
    .map(|row| row.try_get::<i64, _>("quota_bytes"))
    .transpose()
    .map(|bytes| bytes.map(|bytes| u64::try_from(bytes).unwrap_or_default()))
    .map_err(|why| Refused::failed(format!("the quotas could not be read: {why}")))
}

/// An owner's quota, null for the administrator, and whether the default stands for it.
async fn quota_of(standing: &Standing, owner: &Owner) -> Result<(Option<u64>, bool), Refused> {
  if owner.administrator {
    return Ok((None, false));
  }

  Ok(match quota_set(standing, &owner.key).await? {
    Some(bytes) => (Some(bytes), false),
    None => (Some(standing.settings.quota.default_bytes), true),
  })
}

/// What a space with a quota still takes: its folder and the bytes its files may hold together.
pub(super) struct Room {
  folder: PathBuf,
  quota: u64,
}

impl Room {
  /// Nothing where `adding` bytes may arrive while `freeing` give way; else 413 `quota-exceeded`, before
  /// anything is changed (`docs/files.md`, 「Quotas」).
  pub(super) async fn take(&self, adding: u64, freeing: u64) -> Result<(), Refused> {
    // A write that adds nothing on the whole is never refused, even past a quota set below the usage.
    if adding <= freeing {
      return Ok(());
    }

    let used = usage(self.folder.clone()).await?;

    if used.saturating_sub(freeing).saturating_add(adding) <= self.quota {
      return Ok(());
    }

    Err(
      Refused::new(
        StatusCode::PAYLOAD_TOO_LARGE,
        "quota-exceeded",
        "the write would take the drive past its quota",
      )
      .with("usedBytes", used)
      .with("quotaBytes", self.quota),
    )
  }

  /// The bytes of the files at `within` of this space, as a copy inside it carries them: no link followed,
  /// and nothing counted outside the space.
  pub(super) async fn bytes_at(&self, within: &Within) -> Result<u64, Refused> {
    let (folder, within) = (self.folder.clone(), within.clone());

    tokio::task::spawn_blocking(move || {
      let Ok(top) = fs::canonicalize(&folder) else {
        return 0;
      };
      let joined = within.names.iter().fold(top.clone(), |path, name| path.join(name));
      let (Some(parent), Some(name)) = (joined.parent(), joined.file_name()) else {
        return 0;
      };

      match fs::canonicalize(parent) {
        Ok(real) if real.starts_with(&top) => bytes_under(&real.join(name)),
        _ => 0,
      }
    })
    .await
    .map_err(Refused::failed)
  }
}

/// One owner's space as the place `drive`, or why its storage cannot be opened.
pub(super) struct Space {
  volume: Result<Volume, Refused>,
  room: Option<Room>,
}

impl Space {
  /// The space of `owner`, its folder made the first time it is reached.
  pub(super) async fn of(owner: &Owner, standing: &Standing) -> Result<Self, Refused> {
    let folder = match space_folder(&standing.settings, &owner.key)? {
      Ok(folder) => folder,
      Err(reason) => {
        return Ok(Self {
          volume: Err(unready(&reason)),
          room: None,
        });
      }
    };
    let made = folder.clone();

    tokio::task::spawn_blocking(move || fs::create_dir_all(&made))
      .await
      .map_err(Refused::failed)?
      .map_err(|why| io_refusal(&why))?;

    let (quota, _) = quota_of(standing, owner).await?;

    Ok(Self {
      volume: Ok(Volume {
        place: SPACE.to_owned(),
        mount: folder.to_string_lossy().into_owned(),
        fs_type: String::new(),
        // Nothing in an owner's space is protected from its owner (`docs/files.md`, 「The BMO drive」).
        guard: Arc::new(Guard::default()),
      }),
      room: quota.map(|quota| Room { folder, quota }),
    })
  }
}

impl Places for Space {
  fn place(&self, id: &str) -> Result<&dyn Place, Refused> {
    if id != SPACE {
      return Err(no_place());
    }

    self.volume.as_ref().map(|volume| volume as &dyn Place).map_err(Clone::clone)
  }

  fn room(&self) -> Option<&Room> {
    self.room.as_ref()
  }
}

/// The name a folder of `id` is kept under for `owner`: the drive's for each owner apart, so one owner's
/// reading is never another's.
pub(super) fn kept_name(id: &str, owner: &Owner) -> String {
  if id == SPACE { format!("{SPACE}\n{}", owner.key) } else { id.to_owned() }
}

/// The usage of `key`'s space, null where the storage cannot be opened.
async fn usage_of(settings: &Settings, key: &str) -> Result<Option<u64>, Refused> {
  match space_folder(settings, key)? {
    Ok(folder) => Ok(Some(usage(folder).await?)),
    Err(_) => Ok(None),
  }
}

/// The asking owner's drive as a place of the places list: what is left of its quota, or for the
/// administrator what is free where the root stands.
pub(super) async fn space_place(owner: &Owner, standing: &Standing) -> Result<wire::Place, Refused> {
  let settings = &standing.settings;
  let (free_bytes, reason) = match root(settings) {
    Err(reason) => (None, Some(reason)),
    Ok(root) => {
      let free = match quota_of(standing, owner).await?.0 {
        Some(quota) => usage_of(settings, &owner.key).await?.map(|used| quota.saturating_sub(used)),
        None => free_bytes(&root.to_string_lossy()).await,
      };

      (free, None)
    }
  };

  Ok(wire::Place {
    name: SPACE.to_owned(),
    kind: wire::PlaceKind::Drive,
    label: SPACE.to_owned(),
    free_bytes,
    node_name: None,
    drive_key: None,
    mount: None,
    reason_code: reason.as_ref().map(|_| UNREADY.to_owned()),
    reason,
  })
}

/// What the asking owner may reach: whether the machine branch is its, and its drive's usage and quota.
pub struct OwnerRead;

impl Route for OwnerRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/owner";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let owner = call.owner()?;
    let standing = call.standing();
    let (quota_bytes, _) = quota_of(standing, &owner).await?;

    Ok(answer(
      StatusCode::OK,
      wire::Owner {
        owner_key: owner.key.clone(),
        administrator: owner.administrator,
        used_bytes: usage_of(&standing.settings, &owner.key).await?,
        quota_bytes,
      },
    ))
  }
}

crate::route!(OwnerRead);

/// Where the BMO drive stands and how it is kept, as the settings state it; the administrator's alone.
pub struct StorageRead;

impl Route for StorageRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/storage";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    call.owner()?.administrator_only("no such route for this owner")?;

    let settings = &call.standing().settings;
    let storage = &settings.storage;
    let (kind, location) = match storage.kind {
      StorageKind::Local => (wire::StorageKind::Local, storage.root.clone()),
      StorageKind::Node => (wire::StorageKind::Node, format!("{}:{}", storage.node, storage.root)),
      StorageKind::S3 => (wire::StorageKind::S3, format!("s3://{}/{}", storage.bucket, storage.root)),
    };
    let reason = root(settings).err();

    Ok(answer(
      StatusCode::OK,
      wire::Storage {
        kind,
        location,
        backup: Some(storage.backup.clone()).filter(|backup| !backup.is_empty()),
        default_quota_bytes: settings.quota.default_bytes,
        reason_code: reason.as_ref().map(|_| UNREADY.to_owned()),
        reason,
      },
    ))
  }
}

crate::route!(StorageRead);

/// The owner a quota route's path names, asked by the administrator alone.
async fn quota_owner(call: &mut Call) -> Result<Owner, Refused> {
  call.owner()?.administrator_only("no such route for this owner")?;

  let key = call.path_param("ownerKey").await?;

  Owner::keyed(Some(&key), &call.standing().settings)
}

/// One owner's quota as it stands, with its usage.
async fn quota_answer(standing: &Standing, owner: &Owner) -> Result<Quota, Refused> {
  let (quota_bytes, defaulted) = quota_of(standing, owner).await?;

  Ok(Quota {
    owner_key: owner.key.clone(),
    quota_bytes,
    used_bytes: usage_of(&standing.settings, &owner.key).await?,
    defaulted,
  })
}

/// One owner's quota and usage.
pub struct QuotaRead;

impl Route for QuotaRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/quotas/{ownerKey}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let owner = quota_owner(&mut call).await?;

    Ok(answer(StatusCode::OK, quota_answer(call.standing(), &owner).await?))
  }
}

crate::route!(QuotaRead);

/// One owner's quota set, or returned to the default.
pub struct QuotaSet;

impl Route for QuotaSet {
  const METHOD: Method = Method::PUT;
  const PATH: &'static str = "/quotas/{ownerKey}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let owner = quota_owner(&mut call).await?;
    let asked: QuotaRequest = call.body().await?;

    if owner.administrator {
      return Err(Refused::rule("no-quota", "the administrator's own space has no quota"));
    }

    let pool = call.standing().store.pool();
    let written = match asked.quota_bytes {
      Some(bytes) => {
        sqlx::query(
          "INSERT INTO quotas (owner, quota_bytes, set_at) VALUES ($1, $2, $3)
         ON CONFLICT (owner) DO UPDATE SET quota_bytes = excluded.quota_bytes, set_at = excluded.set_at",
        )
        .bind(&owner.key)
        .bind(i64::try_from(bytes).unwrap_or(i64::MAX))
        .bind(now_ms())
        .execute(pool)
        .await
      }
      None => sqlx::query("DELETE FROM quotas WHERE owner = $1").bind(&owner.key).execute(pool).await,
    };

    written.map_err(|why| Refused::failed(format!("the quotas could not be written: {why}")))?;

    Ok(answer(StatusCode::OK, quota_answer(call.standing(), &owner).await?))
  }
}

crate::route!(QuotaSet);
