//! Sharing with members (docs/files.md, 「Sharing with members」): a file or a folder shared with a member read
//! by that member alone, cut down to it, the share ended and the reading refused again, the links beside it
//! untouched, and the permissions and keys the service refuses. The service runs in this process on a loopback
//! port and is asked through its own client.

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use bmo_drive::client::{ClientFailure, DriveClient};
use bmo_drive::http::{Standing, router};
use bmo_drive::settings::Settings;
use bmo_drive::store::Store;
use bmo_drive::wire::{
  ContentQuery, EntriesQuery, FolderRequest, MemberShareChange, MemberShareRequest, MemberSharesQuery,
  ShareKind, SharePermission, ShareRequest, SharedContentQuery, SharedQuery, WriteQuery,
};

const TOKEN: &str = "drive-token";
const OWNER: &str = "4";
const MEMBER: &str = "6";
const STRANGER: &str = "7";

/// A folder of its own for one test, on the disk the build stands on.
fn scratch(name: &str) -> PathBuf {
  let folder =
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("members-{}-{name}", std::process::id()));
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
address = "http://[::1]:9"

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

fn client(address: &str, owner: &str) -> DriveClient {
  DriveClient::new(address, TOKEN).with_owner(owner)
}

/// The status and the code a refused call answered.
fn status(failure: ClientFailure) -> (u16, String) {
  let code = failure.code().unwrap_or_default().to_owned();

  match failure {
    ClientFailure::Answered { status, .. } => (status, code),
    other => panic!("the service did not answer: {other}"),
  }
}

fn asked(path: &str, users: &[&str], permission: SharePermission) -> MemberShareRequest {
  MemberShareRequest {
    place_name: "drive".into(),
    path: path.into(),
    users: users.iter().map(|user| (*user).to_owned()).collect(),
    permission,
  }
}

/// The bytes a member reads at `path` of the share `id`, or why it was refused.
async fn read(reader: &DriveClient, id: i64, path: &str) -> Result<Vec<u8>, (u16, String)> {
  let query = SharedContentQuery {
    path: Some(path.to_owned()).filter(|path| !path.is_empty()),
    ..Default::default()
  };

  match reader.shared_content(id, &query, None).await {
    Ok(content) => Ok(content.bytes().await.expect("the bytes arrive").to_vec()),
    Err(failure) => Err(status(failure)),
  }
}

#[tokio::test]
async fn a_file_shared_is_read_by_its_member_alone_until_the_share_ends() {
  let folder = scratch("file");
  let address = serve(&folder).await;
  let (owner, member, stranger) =
    (client(&address, OWNER), client(&address, MEMBER), client(&address, STRANGER));
  let holder = DriveClient::new(&address, TOKEN);

  owner
    .write_content("drive", "plan.txt", &WriteQuery::default(), b"the plan".to_vec())
    .await
    .expect("the owner writes");
  let link = owner
    .create_share(&ShareRequest {
      place_name: "drive".into(),
      path: "plan.txt".into(),
      expires_at: None,
      actor_name: None,
    })
    .await
    .expect("the owner shares the file by link");

  let made = owner
    .create_member_shares(&asked("plan.txt", &[MEMBER], SharePermission::Read))
    .await
    .expect("the owner shares the file with a member");
  let [share] = made.items.as_slice() else {
    panic!("one member share is made: {made:?}")
  };
  assert_eq!((share.user.as_str(), share.permission), (MEMBER, SharePermission::Read));

  let listed = owner
    .member_shares(&MemberSharesQuery {
      place_name: "drive".into(),
      path: "plan.txt".into(),
      ..Default::default()
    })
    .await
    .expect("the owner lists the file's members");
  assert_eq!(listed.items, made.items);

  let shared = member.shared(&SharedQuery::default()).await.expect("the member lists what is shared");
  let [seen] = shared.items.as_slice() else {
    panic!("one entry is shared with the member: {shared:?}")
  };
  assert_eq!(
    (seen.id, seen.owner_key.as_str(), seen.name.as_str(), seen.kind),
    (share.id, OWNER, "plan.txt", ShareKind::File)
  );

  assert_eq!(read(&member, share.id, "").await, Ok(b"the plan".to_vec()));

  // Nobody but the member reads it, the owner through the share included, and the member's own drive
  // stays its own.
  assert_eq!(read(&stranger, share.id, "").await, Err((404, "not-found".into())));
  assert_eq!(read(&owner, share.id, "").await, Err((404, "not-found".into())));
  assert!(stranger.shared(&SharedQuery::default()).await.expect("listed").items.is_empty());
  let own = member.content("drive", "plan.txt", &ContentQuery::default(), None).await;
  assert_eq!(own.err().map(status).map(|(code, _)| code), Some(404));

  owner.delete_member_share(share.id).await.expect("the owner takes the member off");

  assert_eq!(read(&member, share.id, "").await, Err((404, "not-found".into())));
  assert!(member.shared(&SharedQuery::default()).await.expect("listed").items.is_empty());

  // The link beside it opens as it did.
  let bytes = holder
    .share_content(&link.token, None)
    .await
    .expect("the link opens")
    .bytes()
    .await
    .expect("read");
  assert_eq!(&bytes[..], b"the plan");
}

#[tokio::test]
async fn a_folder_shared_is_read_beneath_it_alone() {
  let folder = scratch("folder");
  let address = serve(&folder).await;
  let (owner, member) = (client(&address, OWNER), client(&address, MEMBER));

  owner
    .create_folder(
      "drive",
      &FolderRequest {
        path: "team".into(),
        conflict: None,
      },
    )
    .await
    .expect("the owner makes a folder");
  for (path, bytes) in [("team/notes.txt", &b"team notes"[..]), ("private.txt", &b"the owner's alone"[..])]
  {
    owner
      .write_content("drive", path, &WriteQuery::default(), bytes.to_vec())
      .await
      .expect("the owner writes");
  }
  symlink("../private.txt", folder.join("spaces").join(OWNER).join("team").join("out.txt"))
    .expect("a link out of the folder is made");

  let made = owner
    .create_member_shares(&asked("team", &[MEMBER], SharePermission::Read))
    .await
    .expect("the owner shares the folder with a member");
  let id = made.items[0].id;

  let listing = member
    .shared_entries(id, &EntriesQuery::default())
    .await
    .expect("the member lists the folder");
  let notes = listing
    .items
    .iter()
    .find(|entry| entry.name == "notes.txt")
    .expect("the folder's file is listed");
  assert_eq!(
    (notes.place_name.as_str(), notes.path.as_str()),
    (format!("shared:{id}").as_str(), "notes.txt")
  );

  assert_eq!(read(&member, id, "notes.txt").await, Ok(b"team notes".to_vec()));
  assert_eq!(read(&member, id, "").await, Err((422, "is-folder".into())));
  assert_eq!(read(&member, id, "../private.txt").await, Err((422, "outside-place".into())));
  assert!(
    read(&member, id, "out.txt").await.is_err(),
    "a link leading out of the shared folder opens nothing"
  );
}

#[tokio::test]
async fn a_share_opens_reading_alone_and_never_to_its_owner() {
  let folder = scratch("refused");
  let address = serve(&folder).await;
  let (owner, stranger) = (client(&address, OWNER), client(&address, STRANGER));

  owner
    .write_content("drive", "plan.txt", &WriteQuery::default(), b"the plan".to_vec())
    .await
    .expect("the owner writes");

  let refused = |failure: Result<_, ClientFailure>| {
    status(failure.map(|_: bmo_drive::wire::MemberSharePage| ()).unwrap_err())
  };
  assert_eq!(
    refused(owner.create_member_shares(&asked("plan.txt", &[MEMBER], SharePermission::Edit)).await),
    (422, "read-only".into())
  );
  assert_eq!(
    refused(owner.create_member_shares(&asked("plan.txt", &[OWNER], SharePermission::Read)).await),
    (422, "own-share".into())
  );
  assert_eq!(
    refused(
      owner
        .create_member_shares(&asked("missing.txt", &[MEMBER], SharePermission::Read))
        .await
    ),
    (404, "not-found".into())
  );

  let first = owner
    .create_member_shares(&asked("plan.txt", &[MEMBER], SharePermission::Read))
    .await
    .expect("the owner shares");
  let again = owner
    .create_member_shares(&asked("plan.txt", &[MEMBER], SharePermission::Read))
    .await
    .expect("the owner shares again");
  assert_eq!(again.items, first.items, "sharing again keeps the row");
  let id = first.items[0].id;

  let edit = MemberShareChange {
    permission: SharePermission::Edit,
  };
  assert_eq!(
    status(owner.update_member_share(id, &edit).await.unwrap_err()),
    (422, "read-only".into())
  );
  assert_eq!(status(stranger.delete_member_share(id).await.unwrap_err()).0, 404);
}
