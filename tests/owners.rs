//! The two branches and the owners (docs/files.md, 「Owners and the two branches」, 「The BMO drive」,
//! 「Share links」): each owner's drive its own alone, the quota judged before bytes arrive, the machines the
//! administrator's alone, and share links that open in both branches. The service runs in this process on a
//! loopback port and is asked through its own client.

use std::path::{Path, PathBuf};

use bmo_drive::client::{ClientFailure, DriveClient};
use bmo_drive::http::{Standing, router};
use bmo_drive::settings::Settings;
use bmo_drive::store::Store;
use bmo_drive::wire::{
  CopyRequest, EntriesQuery, EventsQuery, MachineQuery, PlaceKind, PlacesQuery, QuotaRequest, ShareKind,
  ShareRequest, SharesQuery, WriteQuery,
};

const TOKEN: &str = "drive-token";
const ADMINISTRATOR: &str = "qa-admin";
const DEFAULT_QUOTA: u64 = 5 * 1024 * 1024 * 1024;

/// A folder of its own for one test, on the disk the build stands on.
fn scratch(name: &str) -> PathBuf {
  let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("owners-{}-{name}", std::process::id()));
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
administrator = "{ADMINISTRATOR}"

[storage]
kind = "local"
root = "{root}"
backup = "A test's storage, kept nowhere."
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

/// The status a refused call answered.
fn status(failure: ClientFailure) -> (u16, String) {
  let code = failure.code().unwrap_or_default().to_owned();

  match failure {
    ClientFailure::Answered { status, .. } => (status, code),
    other => panic!("the service did not answer: {other}"),
  }
}

async fn names(drive: &DriveClient, place: &str) -> Vec<String> {
  drive
    .entries(
      place,
      &EntriesQuery {
        fresh: Some(true),
        ..Default::default()
      },
    )
    .await
    .expect("the folder is listed")
    .items
    .into_iter()
    .map(|entry| entry.name)
    .collect()
}

#[tokio::test]
async fn each_owner_sees_only_its_own_drive() {
  let folder = scratch("isolation");
  let address = serve(&folder).await;
  let (one, other) = (client(&address, "1"), client(&address, "2"));

  one
    .write_content("drive", "notes.txt", &WriteQuery::default(), b"mine".to_vec())
    .await
    .expect("the first owner writes");

  assert_eq!(names(&one, "drive").await, ["notes.txt"]);
  assert_eq!(names(&other, "drive").await, Vec::<String>::new());
  assert_eq!(status(other.entry("drive", "notes.txt").await.unwrap_err()).0, 404);

  let places = other.places(&PlacesQuery::default()).await.expect("the places are listed");
  assert_eq!(places.items.len(), 1);
  assert_eq!(places.items[0].name, "drive");
  assert_eq!(places.items[0].kind, PlaceKind::Drive);

  let theirs = one.events(&EventsQuery::default()).await.expect("the first owner's events are read");
  assert!(theirs.items.iter().any(|event| event.kind == "entry.created"));
  let mine = other.events(&EventsQuery::default()).await.expect("the second owner's events are read");
  assert!(mine.items.is_empty());

  let none = DriveClient::new(&address, TOKEN);
  assert_eq!(
    status(none.places(&PlacesQuery::default()).await.unwrap_err()),
    (403, "forbidden".into())
  );
}

#[tokio::test]
async fn a_write_past_the_quota_is_refused_before_anything_changes() {
  let folder = scratch("quota");
  let address = serve(&folder).await;
  let (administrator, owner) = (client(&address, ADMINISTRATOR), client(&address, "7"));

  let fresh = client(&address, "8").owner().await.expect("an owner is read");
  assert!(!fresh.administrator);
  assert_eq!(fresh.quota_bytes, Some(DEFAULT_QUOTA));
  assert_eq!(fresh.used_bytes, Some(0));

  let set = administrator
    .set_quota(
      "7",
      &QuotaRequest {
        quota_bytes: Some(10),
      },
    )
    .await
    .expect("the quota is set");
  assert_eq!(set.quota_bytes, Some(10));
  assert!(!set.defaulted);

  owner
    .write_content("drive", "eight.bin", &WriteQuery::default(), vec![1; 8])
    .await
    .expect("a write within the quota lands");

  let refused = owner
    .write_content("drive", "five.bin", &WriteQuery::default(), vec![2; 5])
    .await
    .unwrap_err();
  assert_eq!(status(refused), (413, "quota-exceeded".into()));
  assert_eq!(names(&owner, "drive").await, ["eight.bin"]);

  owner
    .write_content("drive", "eight.bin", &WriteQuery::default(), vec![3; 9])
    .await
    .expect("a file written over gives its bytes back");

  let copied = owner
    .copy(
      "drive",
      &CopyRequest {
        paths: vec!["eight.bin".into()],
        to: String::new(),
        to_place_name: None,
        conflict: Some(bmo_drive::wire::Conflict::Rename),
      },
    )
    .await
    .unwrap_err();
  assert_eq!(status(copied), (413, "quota-exceeded".into()));

  let read = administrator.quota("7").await.expect("the quota is read");
  assert_eq!((read.quota_bytes, read.used_bytes), (Some(10), Some(9)));

  let back = administrator
    .set_quota("7", &QuotaRequest { quota_bytes: None })
    .await
    .expect("the quota returns to the default");
  assert_eq!((back.quota_bytes, back.defaulted), (Some(DEFAULT_QUOTA), true));

  let own = administrator.owner().await.expect("the administrator is read");
  assert!(own.administrator);
  assert_eq!(own.quota_bytes, None);
  assert_eq!(
    status(
      administrator
        .set_quota(
          ADMINISTRATOR,
          &QuotaRequest {
            quota_bytes: Some(1)
          }
        )
        .await
        .unwrap_err()
    ),
    (422, "no-quota".into())
  );
  assert_eq!(status(owner.quota("7").await.unwrap_err()).0, 404);
}

#[tokio::test]
async fn the_machines_are_the_administrators_alone() {
  let folder = scratch("machines");
  let address = serve(&folder).await;
  let (administrator, owner) = (client(&address, ADMINISTRATOR), client(&address, "3"));

  let places = administrator
    .places(&PlacesQuery::default())
    .await
    .expect("the administrator lists the places");
  assert_eq!(places.items[0].kind, PlaceKind::Drive);
  let volume = places
    .items
    .iter()
    .find(|place| place.kind == PlaceKind::Volume)
    .map(|place| place.name.clone())
    .expect("this machine has a volume");
  administrator
    .machine(&MachineQuery::default())
    .await
    .expect("the administrator reads this machine");
  administrator
    .entries(&volume, &EntriesQuery::default())
    .await
    .expect("the administrator walks a volume");
  let storage = administrator.storage().await.expect("the administrator reads the storage");
  assert_eq!(storage.default_quota_bytes, DEFAULT_QUOTA);
  assert!(storage.reason.is_none());

  assert_eq!(status(owner.machine(&MachineQuery::default()).await.unwrap_err()).0, 404);
  assert_eq!(status(owner.entries(&volume, &EntriesQuery::default()).await.unwrap_err()).0, 404);
  assert_eq!(
    status(
      owner
        .places(&PlacesQuery {
          node_name: Some("node-a".into()),
          ..Default::default()
        })
        .await
        .unwrap_err()
    )
    .0,
    404
  );
  assert_eq!(status(owner.storage().await.unwrap_err()).0, 404);
}

#[tokio::test]
async fn share_links_open_in_both_branches() {
  let folder = scratch("shares");
  let address = serve(&folder).await;
  let (administrator, owner, other) =
    (client(&address, ADMINISTRATOR), client(&address, "4"), client(&address, "5"));
  let holder = DriveClient::new(&address, TOKEN);

  owner
    .write_content("drive", "shared.txt", &WriteQuery::default(), b"from the drive".to_vec())
    .await
    .expect("the owner writes");
  let link = owner
    .create_share(&ShareRequest {
      place_name: "drive".into(),
      path: "shared.txt".into(),
      expires_at: None,
      actor_name: None,
    })
    .await
    .expect("the owner shares a file of its drive");
  assert_eq!(link.kind, ShareKind::File);
  let bytes = holder
    .share_content(&link.token, None)
    .await
    .expect("the link opens")
    .bytes()
    .await
    .expect("read");
  assert_eq!(&bytes[..], b"from the drive");

  assert!(other.shares(&SharesQuery::default()).await.expect("listed").items.is_empty());
  assert_eq!(status(other.delete_share(&link.token).await.unwrap_err()).0, 404);
  assert_eq!(owner.shares(&SharesQuery::default()).await.expect("listed").items.len(), 1);

  // A file of this machine, shared from the volume it lies on.
  let file = folder.join("machine.txt");
  std::fs::write(&file, b"from the machine").expect("the machine's file is written");
  let real = std::fs::canonicalize(&file).expect("the file is there");
  let real = real.to_string_lossy();
  let machine = administrator.machine(&MachineQuery::default()).await.expect("this machine is read");
  let volume = machine
    .drives
    .iter()
    .flat_map(|drive| drive.volumes.iter())
    .filter(|volume| volume.mount == "/" || real.starts_with(&format!("{}/", volume.mount)))
    .max_by_key(|volume| volume.mount.len())
    .expect("a volume holds the file");
  let path = real[volume.mount.len()..].trim_start_matches('/').to_owned();
  let machine_link = administrator
    .create_share(&ShareRequest {
      place_name: volume.place_name.clone(),
      path,
      expires_at: None,
      actor_name: None,
    })
    .await
    .expect("the administrator shares a file of this machine");
  let bytes = holder
    .share_content(&machine_link.token, None)
    .await
    .expect("the link opens")
    .bytes()
    .await
    .expect("read");
  assert_eq!(&bytes[..], b"from the machine");

  let refused = owner
    .create_share(&ShareRequest {
      place_name: volume.place_name.clone(),
      path: String::new(),
      expires_at: None,
      actor_name: None,
    })
    .await
    .unwrap_err();
  assert_eq!(status(refused).0, 404);
}
