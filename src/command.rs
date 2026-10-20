//! The command `bmo-drive` answers beside its service (`docs/api.md`, 「The command」): the nouns it
//! declares, and `file share`, which makes, lists and takes away share links through the service's own
//! client.

use clap::Args;

use crate::client::{ClientFailure, DriveClient};
use crate::events::now_ms;
use crate::settings::Settings;
use crate::wire::{MachineQuery, Share, ShareKind, ShareRequest, SharesQuery};

/// What `bmo-drive nouns` prints: each noun it carries, marked to run on the machine the service
/// stands on, since its links are of that machine's files.
pub const NOUNS: &[&str] = &["file share\thome", "file-share\thome"];

const SHARE_USAGE: &str = "usage: bmo-drive file share <path> [--days <n> | --forever] [--json]
       bmo-drive file share --place <name> <path> [--days <n> | --forever] [--json]
       bmo-drive file share --list [--json]
       bmo-drive file share --remove <token>";

// A link lives a week where no span is named.
const WEEK_MS: i64 = 7 * 24 * 60 * 60 * 1000;

// The most links one page of the list asks for.
const LIST_PAGE: u64 = 1000;

/// A share link for a file or folder of the file viewer, a path on this machine or one inside a place;
/// the links standing with --list, and one taken away with --remove.
#[derive(Args, Debug)]
pub struct ShareArgs {
  path: Option<String>,
  #[arg(long)]
  place: Option<String>,
  #[arg(long, conflicts_with = "forever")]
  days: Option<f64>,
  #[arg(long)]
  forever: bool,
  #[arg(long, conflicts_with_all = ["path", "remove"])]
  list: bool,
  #[arg(long, conflicts_with = "path")]
  remove: Option<String>,
  /// The link or the page printed exactly as the service answers it.
  #[arg(long)]
  json: bool,
}

/// How the command ended where it did not end well.
#[derive(Debug)]
pub enum Failure {
  /// Refused, saying why on the standard error.
  Refused(String),
  /// Ended with lines on the standard output and nothing done.
  Partly(Vec<String>),
}

/// What one link prints: its kind, place, path and end, its page and its token.
fn lines_of(share: &Share) -> Vec<String> {
  let path = if share.path.is_empty() { "(top)" } else { &share.path };
  let expiry = share
    .expires_at
    .and_then(chrono::DateTime::from_timestamp_millis)
    .map_or_else(|| "no expiry".to_owned(), |at| format!("until {}", at.format("%Y-%m-%dT%H:%M:%S%.3fZ")));
  let kind = match share.kind {
    ShareKind::File => "file",
    ShareKind::Folder => "folder",
  };

  vec![
    format!("file-share: {kind} {} {path}, {expiry}", share.place_name),
    format!("page: {}", share.url),
    format!("token: {}", share.token),
  ]
}

/// A member or a page as the service answered it, on one line.
fn json_line(value: &impl serde::Serialize) -> Result<Vec<String>, Failure> {
  serde_json::to_string(value)
    .map(|line| vec![line])
    .map_err(|why| Failure::Refused(format!("file-share: the answer could not be printed: {why}")))
}

/// The volume of this machine a real path lies on, as the service names its places, and the path inside
/// it: the deepest mount holding it.
async fn volume_of(client: &DriveClient, real: &str) -> Result<Option<(String, String)>, ClientFailure> {
  let machine = client.machine(&MachineQuery::default()).await?;
  let holding = machine
    .drives
    .iter()
    .flat_map(|drive| drive.volumes.iter())
    .filter(|volume| {
      let mount = &volume.mount;

      real == mount || real.starts_with(&if mount == "/" { "/".to_owned() } else { format!("{mount}/") })
    })
    .max_by_key(|volume| volume.mount.len());

  Ok(holding.map(|volume| {
    (volume.place_name.clone(), real[volume.mount.len()..].trim_start_matches('/').to_owned())
  }))
}

/// `file share` as its words ask: the links listed, one taken away, or one made for a path. A path alone
/// is a path on this machine given to the volume it lies on; one with a place is read inside that place.
pub async fn file_share(settings: Settings, asked: ShareArgs) -> Result<Vec<String>, Failure> {
  let refused = |why: String| Failure::Refused(format!("file-share: {why}"));
  let client = DriveClient::of_settings(&settings);

  if asked.list {
    let query = SharesQuery {
      after: None,
      limit: Some(LIST_PAGE),
    };
    let page = client
      .shares(&query)
      .await
      .map_err(|why| refused(format!("the links could not be read: {why}")))?;

    if asked.json {
      return json_line(&page);
    }

    if page.items.is_empty() {
      return Ok(vec!["file-share: no live links.".to_owned()]);
    }

    return Ok(page.items.iter().flat_map(lines_of).collect());
  }

  if let Some(token) = asked.remove {
    return match client.delete_share(&token).await {
      Ok(()) if asked.json => Ok(Vec::new()),
      Ok(()) => Ok(vec!["file-share: the link is taken away.".to_owned()]),
      Err(failure) if failure.code() == Some("not-found") => {
        Err(Failure::Partly(vec!["file-share: no such link.".to_owned()]))
      }
      Err(failure) => Err(refused(format!("the link could not be taken away: {failure}"))),
    };
  }

  let Some(path) = asked.path else {
    return Err(Failure::Refused(SHARE_USAGE.to_owned()));
  };

  // A span that is not some time at all, a number that is no number among them.
  if asked
    .days
    .is_some_and(|days| days.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater))
  {
    return Err(Failure::Refused(SHARE_USAGE.to_owned()));
  }

  let (place, path) = match asked.place {
    Some(place) => (place, path.split('/').filter(|name| !name.is_empty()).collect::<Vec<_>>().join("/")),
    None => {
      let real = std::fs::canonicalize(&path)
        .map_err(|_| refused(format!("{path} is not there on this machine.")))?;
      let real = real.to_string_lossy();

      volume_of(&client, &real)
        .await
        .map_err(|why| refused(format!("this machine's volumes could not be read: {why}")))?
        .ok_or_else(|| refused(format!("{real} lies on no volume the file viewer walks.")))?
    }
  };
  let expires_at = if asked.forever {
    None
  } else {
    Some(now_ms().saturating_add(
      asked.days.map_or(WEEK_MS, |days| (days * 24.0 * 60.0 * 60.0 * 1000.0).round() as i64),
    ))
  };
  let request = ShareRequest {
    place_name: place.clone(),
    path: path.clone(),
    expires_at,
    actor_name: Some(settings.share.actor.clone()).filter(|actor| !actor.is_empty()),
  };
  let made = client
    .create_share(&request)
    .await
    .map_err(|why| refused(format!("{place} {path} cannot be shared: {why}")))?;

  if asked.json { json_line(&made) } else { Ok(lines_of(&made)) }
}
