//! The console bundle (docs/api.md, 「The console bundle」, 「Owners」): the manifest as bmo-ui's own reading
//! of it reads it, the guard's refusals on the bundle's files and on the screen's routes, and the module's
//! path following its bytes. The service runs in this process on a loopback port over a bundle folder of the
//! test's own.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use bmo_drive::client::{ClientFailure, DriveClient};
use bmo_drive::http::{Standing, router};
use bmo_drive::settings::Settings;
use bmo_drive::store::Store;
use bmo_drive::wire::{ConsoleManifest, ConsoleRole};
use serde_json::{Value, json};

const TOKEN: &str = "drive-token";
const ADMINISTRATOR: &str = "qa-admin";

/// A folder of its own for one test, on the disk the build stands on.
fn scratch(name: &str) -> PathBuf {
  let folder =
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("console-{}-{name}", std::process::id()));
  let _ = std::fs::remove_dir_all(&folder);
  std::fs::create_dir_all(&folder).expect("the scratch folder is made");

  folder
}

/// A built bundle folder under `folder`, its module carrying `module`.
fn built(folder: &Path, module: &str) -> PathBuf {
  let dist = folder.join("dist");

  std::fs::create_dir_all(&dist).expect("the bundle folder is made");
  std::fs::write(dist.join("bundle.js"), module).expect("the module is written");
  std::fs::write(dist.join("bundle.css"), ".drive{display:block}").expect("the stylesheet is written");
  std::fs::write(
    dist.join("words.json"),
    json!({ "en": { "drive": { "title": "Files" } }, "ko": { "drive": { "title": "Files" } } }).to_string(),
  )
  .expect("the words are written");

  dist
}

/// The service, its store and its spaces under `folder`, serving the bundle folder `bundle`; its address.
async fn serve(folder: &Path, bundle: &Path) -> String {
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
administrator = "{ADMINISTRATOR}"

[console]
folder = "{bundle}"

[storage]
kind = "local"
root = "{root}"
"#,
    store = folder.join("drive.db").display(),
    bundle = bundle.display(),
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

/// The status a refused call answered, with its code.
fn status(failure: ClientFailure) -> (u16, String) {
  let code = failure.code().unwrap_or_default().to_owned();

  match failure {
    ClientFailure::Answered { status, .. } => (status, code),
    other => panic!("the service did not answer: {other}"),
  }
}

/// The manifest as bmo-ui's `consoleManifest` reads it, run with Bun in the bundle's project, whose locked
/// packages are installed first where they are not.
fn read_by_bmo_ui(manifest: &Value) -> Result<Value, String> {
  let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("client");

  if !project.join("node_modules/bmo-ui/shared/console-manifest.ts").exists() {
    let installed = Command::new("bun")
      .args(["install", "--frozen-lockfile"])
      .current_dir(&project)
      .status()
      .expect("bun runs");
    assert!(installed.success(), "the bundle's packages are installed");
  }

  let reading = r#"
const { consoleManifest } = await import("bmo-ui/shared/console-manifest");
try {
  console.log(JSON.stringify(consoleManifest(JSON.parse(await Bun.stdin.text()))));
} catch (fault) {
  console.error(fault.message);
  process.exit(1);
}
"#;
  let mut child = Command::new("bun")
    .args(["--eval", reading])
    .current_dir(&project)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .expect("bun runs");

  std::io::Write::write_all(
    child.stdin.as_mut().expect("its input is open"),
    manifest.to_string().as_bytes(),
  )
  .expect("the manifest is handed over");
  drop(child.stdin.take());

  let done = child.wait_with_output().expect("bun ends");

  if done.status.success() {
    Ok(serde_json::from_slice(&done.stdout).expect("the reading prints JSON"))
  } else {
    Err(String::from_utf8_lossy(&done.stderr).into_owned())
  }
}

#[tokio::test]
async fn the_manifest_keeps_the_contract_as_bmo_ui_reads_it() {
  let folder = scratch("manifest");
  let address = serve(&folder, &built(&folder, "export default { routes: [] };")).await;
  let console = DriveClient::new(&address, TOKEN);

  let manifest = console.console_manifest().await.expect("the manifest answers to the token alone");
  let answered = serde_json::to_value(&manifest).expect("the manifest is JSON");
  let read =
    read_by_bmo_ui(&answered).unwrap_or_else(|fault| panic!("bmo-ui refuses the manifest: {fault}"));

  assert_eq!(read, answered, "bmo-ui reads every field as the service answers it");
  assert_eq!(manifest.role, ConsoleRole::Member);
  assert!(manifest.owner_key);
  assert_eq!(manifest.api_prefix, "/");
  assert_eq!(manifest.screen_path, "/files");
  assert_eq!(manifest.service_name, "drive");
  assert!(!manifest.events && !manifest.websocket && manifest.push_kinds.is_empty());
  assert!(
    read_by_bmo_ui(&json!({ "name": "drive" })).is_err(),
    "the reading refuses a broken manifest"
  );

  let words = console.console_words().await.expect("the words answer to the token alone");
  assert_eq!(words["en"]["drive"]["title"], "Files");
}

#[tokio::test]
async fn the_guard_refuses_without_the_credential_the_role_or_the_key() {
  let folder = scratch("guard");
  let module = "export default { routes: [] }; // the guard's";
  let address = serve(&folder, &built(&folder, module)).await;
  let manifest: ConsoleManifest = DriveClient::new(&address, TOKEN).console_manifest().await.expect("read");
  let http = reqwest::Client::new();
  let asked = |path: &str, token: Option<&str>, key: Option<&str>, role: Option<&str>| {
    let mut request = http.get(format!("{address}{path}"));

    if let Some(token) = token {
      request = request.bearer_auth(token);
    }
    if let Some(key) = key {
      request = request.header("Owner-Key", key);
    }
    if let Some(role) = role {
      request = request.header("Owner-Role", role);
    }

    request.send()
  };
  let answered = |response: reqwest::Response| response.status().as_u16();

  assert_eq!(answered(asked("/console-manifest", None, None, None).await.expect("answered")), 401);
  assert_eq!(answered(asked("/console/words", None, None, None).await.expect("answered")), 401);

  for path in [manifest.bundle.as_str(), manifest.stylesheet.as_str(), "/owner", "/places", "/nodes"] {
    assert_eq!(
      answered(asked(path, None, Some("1"), Some("member")).await.expect("answered")),
      401,
      "{path}"
    );
    assert_eq!(
      answered(asked(path, Some("wrong"), Some("1"), Some("member")).await.expect("answered")),
      401,
      "{path}"
    );
    assert_eq!(answered(asked(path, Some(TOKEN), None, None).await.expect("answered")), 403, "{path}");
    assert_eq!(
      answered(asked(path, Some(TOKEN), Some("1"), None).await.expect("answered")),
      403,
      "{path}"
    );
    assert_eq!(
      answered(asked(path, Some(TOKEN), Some("1"), Some("guest")).await.expect("answered")),
      403,
      "{path}"
    );
    assert_eq!(
      answered(asked(path, Some(TOKEN), None, Some("member")).await.expect("answered")),
      403,
      "{path}"
    );
    assert_eq!(
      answered(asked(path, Some(TOKEN), Some(""), Some("member")).await.expect("answered")),
      403,
      "{path}"
    );
  }

  let refused = asked("/owner", Some(TOKEN), Some("1"), None).await.expect("answered");
  let body: Value = refused.json().await.expect("a refusal in the error shape");
  assert_eq!(body["code"], "forbidden");

  for role in ["member", "administrator"] {
    let response = asked(&manifest.bundle, Some(TOKEN), Some("1"), Some(role)).await.expect("answered");
    assert_eq!(response.status().as_u16(), 200, "{role}");
    assert_eq!(response.headers()["content-type"], "text/javascript");
    assert_eq!(response.text().await.expect("the module"), module);
  }

  let stylesheet = asked(&manifest.stylesheet, Some(TOKEN), Some("1"), Some("member"))
    .await
    .expect("answered");
  assert_eq!(stylesheet.status().as_u16(), 200);
  assert_eq!(stylesheet.headers()["content-type"], "text/css");

  // The machine branch stays the administrator's key's alone, whatever role a member's key comes with.
  let member = DriveClient::new(&address, TOKEN).with_owner("1").with_role(ConsoleRole::Administrator);
  assert_eq!(status(member.nodes(&Default::default()).await.unwrap_err()).0, 404);
  let administrator = DriveClient::new(&address, TOKEN).with_owner(ADMINISTRATOR);
  assert_eq!(
    status(administrator.nodes(&Default::default()).await.unwrap_err()),
    (502, "upstream-failed".into())
  );
  assert_eq!(
    status(DriveClient::new(&address, TOKEN).owner().await.unwrap_err()),
    (403, "forbidden".into())
  );
}

#[tokio::test]
async fn the_module_path_follows_its_bytes() {
  let (one, other, same) = (scratch("bytes-one"), scratch("bytes-other"), scratch("bytes-same"));
  let first = serve(&one, &built(&one, "export default { routes: [] }; // one")).await;
  let second = serve(&other, &built(&other, "export default { routes: [] }; // other")).await;
  let third = serve(&same, &built(&same, "export default { routes: [] }; // one")).await;
  let manifest = |address: String| async move {
    DriveClient::new(&address, TOKEN)
      .console_manifest()
      .await
      .expect("the manifest answers")
  };
  let (first_manifest, second_manifest, third_manifest) =
    (manifest(first.clone()).await, manifest(second.clone()).await, manifest(third).await);

  assert_ne!(first_manifest.bundle, second_manifest.bundle);
  assert_eq!(first_manifest.bundle, third_manifest.bundle);
  assert_eq!(first_manifest.stylesheet, second_manifest.stylesheet);
  assert!(first_manifest.bundle.starts_with("/console/bundle-") && first_manifest.bundle.ends_with(".js"));

  let reader = DriveClient::new(&second, TOKEN).with_owner("1");
  let gone = reader.console_file(&first_manifest.bundle).await.err().expect("an old path is refused");
  assert_eq!(status(gone), (404, "not-found".into()));
  let module = reader.console_file(&second_manifest.bundle).await.expect("the module answers");
  assert_eq!(&module.bytes().await.expect("read")[..], b"export default { routes: [] }; // other");
}

#[tokio::test]
async fn an_unbuilt_bundle_is_absent() {
  let folder = scratch("unbuilt");
  let address = serve(&folder, &folder.join("nowhere")).await;
  let console = DriveClient::new(&address, TOKEN);

  assert_eq!(status(console.console_manifest().await.unwrap_err()), (404, "not-found".into()));
  assert_eq!(status(console.console_words().await.unwrap_err()), (404, "not-found".into()));
}
