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

/// The store's migrations, from the 1.0.0 baseline (`docs/files.md`, 「The store」).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// The record of migrations an install from before 1.0.0 holds once every migration of its last release ran,
/// each by its version and description. Its schema is the baseline's.
pub const BEFORE: [(i64, &str); 4] = [
  (1, "file shares"),
  (2, "events and milliseconds"),
  (3, "owners and quotas"),
  (4, "member shares"),
];

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

    adopt(&pool).await?;
    MIGRATOR.run(&pool).await?;

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

/// A record naming every migration of the last release before 1.0.0, and nothing else, rewritten to name the
/// baseline alone, touching no table and no row; one naming only part of them refused.
async fn adopt(pool: &SqlitePool) -> Result<(), StoreError> {
  let mut tx = pool.begin().await?;
  let recorded: bool = sqlx::query_scalar(
    "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations')",
  )
  .fetch_one(&mut *tx)
  .await?;
  if !recorded {
    return Ok(());
  }
  let held: Vec<(i64, String, bool)> =
    sqlx::query_as("SELECT version, description, success FROM _sqlx_migrations ORDER BY version")
      .fetch_all(&mut *tx)
      .await?;
  let before = |(version, description, success): &(i64, String, bool)| {
    *success && BEFORE.iter().any(|(old, named)| old == version && named == description)
  };
  if held.is_empty() || !held.iter().all(before) {
    return Ok(());
  }
  if held.len() != BEFORE.len() {
    return Err(StoreError::Before(held.last().map_or(0, |(version, _, _)| *version)));
  }
  let baseline = MIGRATOR.iter().next().expect("the baseline migration is built in");
  sqlx::query("DELETE FROM _sqlx_migrations").execute(&mut *tx).await?;
  sqlx::query(
    "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) \
     VALUES ($1, $2, TRUE, $3, 0)",
  )
  .bind(baseline.version)
  .bind(baseline.description.as_ref())
  .bind(baseline.checksum.as_ref())
  .execute(&mut *tx)
  .await?;
  tx.commit().await?;
  Ok(())
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
  #[error(
    "the store holds the migrations of a release before 1.0.0 only through version {0}; that release's last \
     migration runs first"
  )]
  Before(i64),
}
