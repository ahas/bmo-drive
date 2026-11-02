//! The share links' store: one SQLite file, opened once as the service or the command starts and shared by
//! every request (`docs/files.md`, 「Share links」).

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

// A statement waits for another writer to give the file back, since the service and the command may write
// it at once. SQLite takes no wait without an end, so the longest it takes, about 24 days, stands for one.
const BUSY: Duration = Duration::from_millis(i32::MAX as u64);

// A request waits in line for a connection of the pool until one is given back; the pool takes no wait
// without an end, so a span past any run stands for one.
const IN_LINE: Duration = Duration::from_secs(u32::MAX as u64);

/// The store, open.
#[derive(Clone, Debug)]
pub struct Store {
  pool: SqlitePool,
}

impl Store {
  /// The store at `path`, made where it does not stand yet, its folder with it, and migrated.
  pub async fn open(path: &Path) -> Result<Self, StoreError> {
    if let Some(folder) = path.parent().filter(|folder| !folder.as_os_str().is_empty()) {
      std::fs::create_dir_all(folder)
        .map_err(|why| StoreError::Folder(folder.display().to_string(), why))?;
    }

    let options = SqliteConnectOptions::from_str("sqlite:")?
      .filename(path)
      .create_if_missing(true)
      .journal_mode(SqliteJournalMode::Wal)
      .busy_timeout(BUSY);
    let pool = SqlitePoolOptions::new().acquire_timeout(IN_LINE).connect_with(options).await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(Self { pool })
  }

  /// The connection pool the store is read and written through.
  pub fn pool(&self) -> &SqlitePool {
    &self.pool
  }

  /// Every link and kept event that names no owner given to `owner`, the administrator's key
  /// (`docs/files.md`, 「What a link is」).
  pub async fn adopt(&self, owner: &str) -> Result<(), StoreError> {
    for statement in [
      "UPDATE file_shares SET owner = $1 WHERE owner = ''",
      "UPDATE events SET owner = $1 WHERE owner = ''",
    ] {
      sqlx::query(statement).bind(owner).execute(&self.pool).await?;
    }

    Ok(())
  }
}

/// Why the store could not be opened.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
  #[error("the store's folder {0} could not be made: {1}")]
  Folder(String, std::io::Error),
  #[error("the store could not be opened: {0}")]
  Open(#[from] sqlx::Error),
  #[error("the store could not be migrated: {0}")]
  Migrate(#[from] sqlx::migrate::MigrateError),
}
