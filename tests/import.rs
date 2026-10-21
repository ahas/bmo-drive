//! The old archive's share links moved into the store (TODO #1923): the export read, the mapping, the
//! refusal of a store already holding rows, the dry run, and the count-and-sample compare.

use std::path::{Path, PathBuf};

use bmo_drive::import::{self, Cell, TABLES};
use bmo_drive::store::Store;

fn scratch(name: &str) -> PathBuf {
  let folder = std::env::temp_dir().join(format!("bmo-drive-import-{}-{name}", std::process::id()));
  let _ = std::fs::remove_dir_all(&folder);
  std::fs::create_dir_all(&folder).expect("the scratch folder is made");

  folder
}

fn manifest(folder: &Path, count: u64) {
  let said = format!(
    r#"{{"snapshot":"1:2:","readAt":"2026-10-21T17:00:00+09:00","tables":[{{"source":"web_file_shares","file":"web_file_shares.ndjson","count":{count},"key":["share_token"]}}]}}"#
  );
  std::fs::write(folder.join("manifest.json"), said).expect("the manifest is written");
}

fn rows(folder: &Path, lines: &[&str]) {
  std::fs::write(folder.join("web_file_shares.ndjson"), lines.join("\n") + "\n")
    .expect("the rows are written");
}

const SHARE: &str = r#"{"share_token":"abc123","place":"volume:/","path":"people/one.md","kind":"file","created_by":"BMO","created_at":1791866256631,"expires_at":1792466256631,"rowid":1,"user_id":1}"#;

#[tokio::test]
async fn a_share_is_moved_and_compared() {
  let folder = scratch("samples");
  manifest(&folder, 1);
  rows(&folder, &[SHARE]);

  let store = folder.join("drive.db");
  let report = import::run(&store, &folder, false).await.expect("the import runs");

  assert_eq!(report.tables.len(), 1);
  assert_eq!(report.tables[0].exported, 1);
  assert_eq!(report.tables[0].written, 1);
  assert_eq!(report.sample.checked, 1);
  assert_eq!(report.sample.failures, Vec::<String>::new());

  let opened = Store::open(&store).await.expect("the store opens");
  let held: (String, String, String, String) =
    sqlx::query_as("SELECT share_token, place, path, created_by FROM file_shares")
      .fetch_one(opened.pool())
      .await
      .expect("the share is read");
  assert_eq!(
    held,
    ("abc123".to_owned(), "volume:/".to_owned(), "people/one.md".to_owned(), "BMO".to_owned())
  );
}

#[tokio::test]
async fn the_mapping_takes_only_the_targets_columns() {
  let table = &TABLES[0];
  let said: serde_json::Value = serde_json::from_str(SHARE).expect("the row is JSON");
  let mapped = import::row(table, &said);

  let columns: Vec<&str> = mapped.iter().map(|(column, _)| *column).collect();
  assert_eq!(
    columns,
    ["share_token", "place", "path", "kind", "created_by", "created_at", "expires_at"]
  );
  assert_eq!(mapped[5].1, Cell::Text("1791866256631".to_owned()));
}

#[tokio::test]
async fn a_dry_run_writes_nothing() {
  let folder = scratch("dry");
  manifest(&folder, 1);
  rows(&folder, &[SHARE]);

  let store = folder.join("drive.db");
  let report = import::run(&store, &folder, true).await.expect("the dry run runs");
  assert!(report.dry_run);
  assert_eq!(report.tables[0].written, 1);

  let opened = Store::open(&store).await.expect("the store opens");
  let held: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_shares")
    .fetch_one(opened.pool())
    .await
    .expect("the count is read");
  assert_eq!(held, 0);
}

#[tokio::test]
async fn a_store_holding_rows_is_refused_and_nothing_is_written() {
  let folder = scratch("refuse");
  manifest(&folder, 1);
  rows(&folder, &[SHARE]);

  let store = folder.join("drive.db");
  import::run(&store, &folder, false).await.expect("the first import runs");
  let refused = import::run(&store, &folder, false).await.expect_err("the second is refused");

  assert!(refused.contains("already holds rows"), "{refused}");
  assert!(refused.contains("file_shares holds 1"), "{refused}");
}

#[tokio::test]
async fn an_export_shorter_than_its_manifest_is_refused() {
  let folder = scratch("short");
  manifest(&folder, 2);
  rows(&folder, &[SHARE]);

  let store = folder.join("drive.db");
  let refused = import::run(&store, &folder, false).await.expect_err("the export is refused");

  assert!(refused.contains("the manifest says 2 rows and the file holds 1"), "{refused}");
}
