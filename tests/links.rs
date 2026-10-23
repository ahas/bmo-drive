//! Links on a volume (docs/files.md, 「Paths and the fence」, 「Listing a folder」): a path walking back into
//! itself is refused `link-loop`, a link to a folder beside is walked once, a listing carries each link's
//! target, and a folder's zip ends with a link to its own top inside it. The service runs in this process on
//! a loopback port over an owner's drive, the links made in its space on the disk.

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bmo_drive::client::{ClientFailure, DriveClient};
use bmo_drive::http::{Standing, router};
use bmo_drive::settings::Settings;
use bmo_drive::store::Store;
use bmo_drive::wire::{ArchiveRequest, EntriesQuery, EntryKind};

const TOKEN: &str = "drive-token";
const OWNER: &str = "1";

/// A folder of its own for one test, on the disk the build stands on.
fn scratch(name: &str) -> PathBuf {
  let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("links-{}-{name}", std::process::id()));
  let _ = std::fs::remove_dir_all(&folder);
  std::fs::create_dir_all(&folder).expect("the scratch folder is made");

  folder
}

/// The service, its store and its spaces under `folder`, answering on a loopback port; its address.
async fn serve(folder: &Path) -> String {
  let settings = format!(
    r#"
[server]
listen = "127.0.0.1:0"
token = "{TOKEN}"

[store]
path = "{store}"

[node]
address = "http://127.0.0.1:9"

[owners]
administrator = "qa-admin"

[storage]
kind = "local"
root = "{root}"
"#,
    store = folder.join("drive.db").display(),
    root = folder.join("spaces").display(),
  );
  std::fs::write(folder.join("settings.local.toml"), settings).expect("the settings are written");

  let settings = Settings::read(folder, Some(Default::default())).expect("the settings are read");
  let store = Store::open(&settings.store.path).await.expect("the store opens");
  let router = router(Standing::new(settings, store)).expect("the routes stand");
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a port is taken");
  let address = format!("http://{}", listener.local_addr().expect("the port is known"));

  tokio::spawn(async move { axum::serve(listener, router).await });

  address
}

/// The owner's drive with two folders side by side, each holding a link to the other, a link to its own
/// top and a link to the space's top: `left/right` and `right/left` cross over, `left/here` leads to
/// `left`, `left/top` to the space's top.
async fn linked(folder: &Path) -> DriveClient {
  let address = serve(folder).await;
  let drive = DriveClient::new(&address, TOKEN).with_owner(OWNER);

  // The space is made the first time its owner reaches it.
  drive.entries("drive", &EntriesQuery::default()).await.expect("the drive is listed");

  let space = folder.join("spaces").join(OWNER);

  std::fs::create_dir_all(space.join("left")).expect("left is made");
  std::fs::create_dir_all(space.join("right")).expect("right is made");
  std::fs::write(space.join("right/note.txt"), b"right").expect("a file is written");
  symlink("../right", space.join("left/right")).expect("left/right is linked");
  symlink("../left", space.join("right/left")).expect("right/left is linked");
  symlink(".", space.join("left/here")).expect("left/here is linked");
  symlink("..", space.join("left/top")).expect("left/top is linked");

  drive
}

async fn listed(
  drive: &DriveClient,
  path: &str,
) -> Result<Vec<(String, EntryKind, Option<String>)>, ClientFailure> {
  let page = drive
    .entries(
      "drive",
      &EntriesQuery {
        path: Some(path.to_owned()),
        fresh: Some(true),
        ..Default::default()
      },
    )
    .await?;

  Ok(
    page
      .items
      .into_iter()
      .map(|entry| (entry.name, entry.kind, entry.link_target))
      .collect(),
  )
}

/// The status and code a refused call answered.
fn refusal(failure: ClientFailure) -> (u16, String) {
  let code = failure.code().unwrap_or_default().to_owned();

  match failure {
    ClientFailure::Answered { status, .. } => (status, code),
    other => panic!("the service did not answer: {other}"),
  }
}

#[tokio::test]
async fn a_link_to_an_ancestor_is_refused() {
  let folder = scratch("ancestor");
  let drive = linked(&folder).await;

  for path in ["left/here", "left/top", "left/top/left"] {
    let refused = listed(&drive, path).await.expect_err("a path walking back is refused");

    assert_eq!(refusal(refused), (422, "link-loop".to_owned()), "{path}");
  }
}

#[tokio::test]
async fn a_link_to_a_sibling_is_entered_once() {
  let folder = scratch("sibling");
  let drive = linked(&folder).await;

  let through = listed(&drive, "left/right").await.expect("the sibling is entered through the link");
  assert_eq!(through.len(), 2);
  assert!(through.iter().any(|(name, kind, _)| name == "note.txt" && *kind == EntryKind::File));

  let back = listed(&drive, "left/right/left").await.expect_err("the link back is refused");
  assert_eq!(refusal(back), (422, "link-loop".to_owned()));
}

#[tokio::test]
async fn a_listing_marks_each_link_with_its_target() {
  let folder = scratch("marks");
  let drive = linked(&folder).await;

  let mut left = listed(&drive, "left").await.expect("left is listed");
  left.sort_by(|one, other| one.0.cmp(&other.0));

  assert_eq!(
    left,
    [
      ("here".to_owned(), EntryKind::Folder, Some(".".to_owned())),
      ("right".to_owned(), EntryKind::Folder, Some("../right".to_owned())),
      ("top".to_owned(), EntryKind::Folder, Some("..".to_owned())),
    ]
  );

  let top = listed(&drive, "").await.expect("the top is listed");
  assert!(top.iter().all(|(_, kind, link)| *kind == EntryKind::Folder && link.is_none()));
}

#[tokio::test]
async fn a_zip_ends_over_links_walking_back() {
  let folder = scratch("zip");
  let drive = linked(&folder).await;

  let answered = drive
    .archive(
      "drive",
      &ArchiveRequest {
        paths: vec!["left".to_owned()],
      },
    )
    .await
    .expect("the zip is answered");
  let bytes = tokio::time::timeout(Duration::from_secs(20), answered.bytes())
    .await
    .expect("the zip ends")
    .expect("the zip is read whole");

  // The zip's end record closes the archive, so a zip cut short has none.
  assert!(bytes.windows(4).any(|four| four == b"PK\x05\x06"));
}
