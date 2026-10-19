//! The command `bmo-drive` answers beside its service (`docs/api.md`, 「The command」): the nouns it
//! declares to `bmo`, and `file share`, which makes, lists and takes away share links in its own process
//! through the same module the routes use.

use clap::Args;

use crate::files::shares::{self, Share};
use crate::http::Standing;
use crate::settings::Settings;
use crate::store::Store;

/// What `bmo-drive nouns` prints: each noun it carries, marked to run on the machine the service
/// stands on, since its links are of that machine's files.
pub const NOUNS: &[&str] = &["file share\thome", "file-share\thome"];

const SHARE_USAGE: &str = "usage: bmo file share <path> [--days <n> | --forever]
       bmo file share --place <id> <path> [--days <n> | --forever]
       bmo file share --list
       bmo file share --remove <token>";

// A link lives a week where no span is named.
const WEEK_SECONDS: i64 = 7 * 24 * 60 * 60;

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
}

/// How the command ended where it did not end well.
#[derive(Debug)]
pub enum Failure {
  /// Refused, saying why on the standard error.
  Refused(String),
  /// Ended with lines on the standard output and nothing done.
  Partly(Vec<String>),
}

/// What one link prints: its kind, place, path and end, its page, its download and its token.
fn lines_of(console: &str, share: &Share) -> Vec<String> {
  let path = if share.path.is_empty() { "(top)" } else { &share.path };
  let expiry = share
    .expires_at
    .and_then(|at| chrono::DateTime::from_timestamp(at, 0))
    .map_or_else(|| "no expiry".to_owned(), |at| format!("until {}", at.format("%Y-%m-%dT%H:%M:%S%.3fZ")));
  let kind = match share.kind {
    shares::ShareKind::File => "file",
    shares::ShareKind::Folder => "folder",
  };
  let token = &share.token;

  vec![
    format!("file-share: {kind} {} {path}, {expiry}", share.place),
    format!("page: {console}/share/files/{token}"),
    format!("download: {console}/api/share/files/{token}/download"),
    format!("token: {token}"),
  ]
}

/// `file share` as its words ask: the links listed, one taken away, or one made for a path. A path alone
/// is a path on this machine given to the volume it lies on; one with a place is read inside that place.
pub async fn file_share(settings: Settings, asked: ShareArgs) -> Result<Vec<String>, Failure> {
  let refused = |why: String| Failure::Refused(format!("file-share: {why}"));
  let console = settings.console.address.trim_end_matches('/').to_owned();
  let maker = settings.share.maker.clone();
  let store = Store::open(&settings.store.path).await.map_err(|why| refused(why.to_string()))?;
  let standing = Standing::new(settings, store);
  let pool = standing.store.pool();

  if asked.list {
    let listed = shares::listed(pool)
      .await
      .map_err(|why| refused(format!("the links could not be read: {}", why.error)))?;

    if listed.is_empty() {
      return Ok(vec!["file-share: no live links.".to_owned()]);
    }

    return Ok(listed.iter().flat_map(|share| lines_of(&console, share)).collect());
  }

  if let Some(token) = asked.remove {
    let removed = shares::remove(pool, token)
      .await
      .map_err(|why| refused(format!("the link could not be taken away: {}", why.error)))?;

    return if removed {
      Ok(vec!["file-share: the link is taken away.".to_owned()])
    } else {
      Err(Failure::Partly(vec!["file-share: no such link.".to_owned()]))
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

      shares::volume_of(&standing, &real)
        .await
        .map_err(|_| refused(format!("{real} lies on no volume the file viewer walks.")))?
    }
  };
  let seconds = if asked.forever {
    None
  } else {
    Some(asked.days.map_or(WEEK_SECONDS, |days| (days * 24.0 * 60.0 * 60.0).round() as i64))
  };
  let made = shares::make(&standing, place.clone(), &path, seconds, maker)
    .await
    .map_err(|why| refused(format!("{place} {path} cannot be shared: {}", why.error)))?;

  Ok(lines_of(&console, &made))
}
