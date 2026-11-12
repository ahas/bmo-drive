//! The console face (docs/api.md, 「The console face」, 「Owners」): the manifest the service serves, read through
//! bmo-ui's own reading of the contract on the release the client vendors; the face wired to the drive's
//! declaration, bundle folder and console credential; no face where no bundle is built; and the drive's own guard
//! of every other route. The face's own rules are bmo-console-face's tests'. The service runs in this process on a
//! loopback port over a bundle folder of the test's own.

use std::path::{Path, PathBuf};
use std::process::Command;

use bmo_console_face::read_manifest;
use bmo_drive::console::{ROLE, reach};
use bmo_drive::http::{Standing, router};
use bmo_drive::settings::Settings;
use bmo_drive::store::Store;
use serde_json::{Value, json};

const TOKEN: &str = "drive-token";
const CONSOLE_TOKEN: &str = "drive-console-token";
const ADMINISTRATOR: &str = "qa-admin";
const MODULE: &str = "export default { routes: [] };";

/// The console bundle's project, from the repository's root.
const CLIENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/client");

/// bmo-ui's release file the console bundle vendors, as its package manifest names it.
fn vendored_release() -> PathBuf {
  let manifest: Value = serde_json::from_slice(
    &std::fs::read(Path::new(CLIENT).join("package.json")).expect("the client's package manifest"),
  )
  .expect("the package manifest is JSON");
  let taken = manifest["devDependencies"]["bmo-ui"].as_str().expect("the client takes bmo-ui");

  Path::new(CLIENT).join(taken.strip_prefix("file:").expect("bmo-ui is taken as a file"))
}

/// The layer the vendored release names in its own layer file: the layer a bundle built on it names.
fn release_layer() -> String {
  let read = Command::new("tar")
    .arg("-xzOf")
    .arg(vendored_release())
    .arg("package/shared/layer.json")
    .output()
    .expect("tar runs");
  assert!(read.status.success(), "{}", String::from_utf8_lossy(&read.stderr));
  let file: Value = serde_json::from_slice(&read.stdout).expect("the layer file is JSON");

  file["layer"].as_str().expect("the layer file names the layer").to_owned()
}

/// A folder of its own for one test, on the disk the build stands on.
fn scratch(name: &str) -> PathBuf {
  let folder =
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("console-{}-{name}", std::process::id()));
  let _ = std::fs::remove_dir_all(&folder);
  std::fs::create_dir_all(&folder).expect("the scratch folder is made");

  folder
}

/// A bundle folder under `folder` as the bundle's build writes it, built on the vendored release's layer.
fn built(folder: &Path) -> PathBuf {
  let dist = folder.join("dist");

  std::fs::create_dir_all(&dist).expect("the bundle folder is made");
  std::fs::write(dist.join("bundle.js"), MODULE).expect("the module is written");
  std::fs::write(dist.join("bundle.css"), ".drive{display:block}").expect("the stylesheet is written");
  std::fs::write(
    dist.join("words.json"),
    json!({ "en": { "drive": { "title": "Files" } }, "ko": { "drive": { "title": "Files" } } }).to_string(),
  )
  .expect("the words are written");
  std::fs::write(dist.join("layer.json"), json!({ "layer": release_layer() }).to_string())
    .expect("the layer is written");

  dist
}

/// The settings of a service with its store and its spaces under `folder`, serving the bundle folder `bundle`.
fn settings(folder: &Path, bundle: &Path) -> Settings {
  let written = format!(
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

[console]
token = "{CONSOLE_TOKEN}"
folder = "{bundle}"

[storage]
kind = "local"
root = "{root}"
"#,
    store = folder.join("drive.db").display(),
    bundle = bundle.display(),
    root = folder.join("spaces").display(),
  );
  std::fs::write(folder.join("settings.local.toml"), written).expect("the settings are written");

  Settings::read(folder, Some(Default::default())).expect("the settings are read")
}

/// The service as `settings` stands it; its address.
async fn serve(settings: Settings) -> String {
  let store = Store::open(&settings.store.path).await.expect("the store opens");
  let router = router(Standing::new(settings, store)).expect("the routes stand");
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a port is taken");
  let address = format!("http://{}", listener.local_addr().expect("the port is known"));

  tokio::spawn(async move { axum::serve(listener, router).await });

  address
}

/// One `GET` of the service, with the token, the owner key and the role where given.
async fn get(
  address: &str,
  path: &str,
  token: Option<&str>,
  key: Option<&str>,
  role: Option<&str>,
) -> reqwest::Response {
  let mut request = reqwest::Client::new().get(format!("{address}{path}"));

  if let Some(token) = token {
    request = request.bearer_auth(token);
  }
  if let Some(key) = key {
    request = request.header("Owner-Key", key);
  }
  if let Some(role) = role {
    request = request.header("Owner-Role", role);
  }

  request.send().await.expect("the service answers")
}

#[tokio::test]
async fn the_served_manifest_is_read_by_the_contract_on_the_vendored_release() {
  let folder = scratch("manifest");
  let address = serve(settings(&folder, &built(&folder))).await;

  let answered = get(&address, "/console-manifest", Some(CONSOLE_TOKEN), None, None).await;
  assert_eq!(answered.status().as_u16(), 200);
  let served: Value = answered.json().await.expect("the manifest is JSON");
  let read = read_manifest(&served, &vendored_release()).unwrap_or_else(|why| panic!("{why}"));

  assert_eq!(read["layer"], release_layer());
  assert_eq!(read["name"], "drive");
  assert_eq!(read["serviceName"], "drive");
  assert_eq!(read["screenPath"], "/files");
  assert_eq!(read["apiPrefix"], "/");
  assert_eq!(read["role"], ROLE.name(), "the declaration names the role the drive's own guard holds");
  assert_eq!(read["ownerKey"], true);
  assert_eq!(read["shareReads"], json!(["/shares/{token}", "/shares/{token}/content"]));

  // The face is guarded by the console's credential, never by the service's own token.
  let service =
    get(&address, "/console-manifest", Some(TOKEN), Some(ADMINISTRATOR), Some("administrator")).await;
  assert_eq!(service.status().as_u16(), 401);

  // The words are the bundle's, and the module is answered with no role, since the manifest names share reads.
  let words: Value =
    get(&address, read["words"].as_str().expect("a words path"), Some(CONSOLE_TOKEN), None, None)
      .await
      .json()
      .await
      .expect("the words are JSON");
  assert_eq!(words["en"]["drive"]["title"], "Files");
  let module = get(
    &address,
    read["bundle"].as_str().expect("a module path"),
    Some(CONSOLE_TOKEN),
    None,
    None,
  )
  .await;
  assert_eq!(module.status().as_u16(), 200);
  assert_eq!(module.text().await.expect("the module"), MODULE);
}

#[tokio::test]
async fn no_face_stands_where_no_bundle_is_built_and_the_routes_are_answered() {
  let folder = scratch("unbuilt");
  let address = serve(settings(&folder, &folder.join("nowhere"))).await;

  let refused = get(&address, "/console-manifest", Some(CONSOLE_TOKEN), None, None).await;
  assert_eq!(refused.status().as_u16(), 404);
  let body: Value = refused.json().await.expect("a refusal in the error shape");
  assert_eq!(body["code"], "no-route");
  assert_eq!(
    get(&address, "/owner", Some(CONSOLE_TOKEN), Some("1"), Some("member"))
      .await
      .status()
      .as_u16(),
    200
  );
}

#[tokio::test]
async fn every_other_route_takes_either_token_held_to_the_key_and_the_role() {
  let folder = scratch("guard");
  let address = serve(settings(&folder, &built(&folder))).await;
  let status = async |path: &str, token: Option<&str>, key: Option<&str>, role: Option<&str>| {
    get(&address, path, token, key, role).await.status().as_u16()
  };

  for token in [TOKEN, CONSOLE_TOKEN] {
    assert_eq!(status("/owner", Some(token), Some("1"), Some("member")).await, 200, "{token}");
    assert_eq!(status("/owner", Some(token), Some("1"), None).await, 403, "{token}");
  }

  for path in ["/owner", "/places", "/nodes"] {
    assert_eq!(status(path, None, Some("1"), Some("member")).await, 401, "{path}");
    assert_eq!(status(path, Some("wrong"), Some("1"), Some("member")).await, 401, "{path}");
    assert_eq!(status(path, Some(TOKEN), None, None).await, 403, "{path}");
    assert_eq!(status(path, Some(TOKEN), Some("1"), Some("guest")).await, 403, "{path}");
    assert_eq!(status(path, Some(TOKEN), None, Some("member")).await, 403, "{path}");
    assert_eq!(status(path, Some(TOKEN), Some(""), Some("member")).await, 403, "{path}");
  }

  let body: Value = get(&address, "/owner", Some(TOKEN), Some("1"), None)
    .await
    .json()
    .await
    .expect("a refusal in the error shape");
  assert_eq!(body["code"], "forbidden");

  // The machine branch stays the administrator's key's alone, whatever role a member's key comes with.
  assert_eq!(status("/nodes", Some(CONSOLE_TOKEN), Some("1"), Some("administrator")).await, 404);
  assert_eq!(status("/nodes", Some(CONSOLE_TOKEN), Some(ADMINISTRATOR), Some("member")).await, 502);
}

#[test]
fn the_reach_names_the_settings_address_and_both_tokens() {
  let folder = scratch("reach");
  let settings = settings(&folder, &folder);

  assert_eq!(
    reach(&settings),
    json!({ "address": "http://127.0.0.1:0", "consoleToken": CONSOLE_TOKEN, "token": TOKEN })
  );
}
