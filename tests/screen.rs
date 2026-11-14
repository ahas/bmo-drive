//! The Files screen (docs/files.md, 「The Files screen」). Where it opens: a reload keeps the folder the address
//! names, an address naming none opens at the folder the viewer last opened, each viewer's own, a remembered
//! place the viewer no longer reaches, a share ended among them, gives way to the first place, and storage the
//! browser refuses remembers nothing without failing. What is shared with the owner: one branch after the
//! drive and before the machines, each share a read-only place named by its sharer, read through the member's
//! own roads. The screen's own modules run with Bun in the bundle's project.

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

/// What a script of checks prints, run with Bun in the bundle's project.
fn run_checks(checks: &str) -> Value {
  let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("client");

  if !project.join("node_modules").exists() {
    let installed = Command::new("bun")
      .args(["install", "--frozen-lockfile"])
      .current_dir(&project)
      .status()
      .expect("bun runs");
    assert!(installed.success(), "the bundle's packages are installed");
  }

  let done = Command::new("bun")
    .args(["--eval", checks])
    .current_dir(&project)
    .output()
    .expect("bun runs");

  assert!(done.status.success(), "the checks run: {}", String::from_utf8_lossy(&done.stderr));

  serde_json::from_slice(&done.stdout).expect("the checks print JSON")
}

/// The checks of the screen's place memory.
const MEMORY_CHECKS: &str = r#"
const memory = await import("./app/domain/place-memory.ts");

class Store {
  kept = new Map();
  getItem(key) { return this.kept.has(key) ? this.kept.get(key) : null; }
  setItem(key, value) { this.kept.set(key, String(value)); }
  removeItem(key) { this.kept.delete(key); }
}
const refusing = {
  getItem() { throw new Error("refused"); },
  setItem() { throw new Error("refused"); },
  removeItem() { throw new Error("refused"); }
};

const shares = [{ id: 4, place: "shared:4", owner: "3", name: "plans", kind: "folder" }];
const member = { administrator: false, viewer: "7", space: { place: "drive" }, machines: [], shared: shares };
const administrator = { administrator: true, viewer: "1", space: { place: "drive" }, machines: [], shared: [] };
const inShare = { place: "shared:4", path: "2025" };
const store = new Store();
const deep = { place: "drive", path: "photos/2024" };

memory.rememberPlace(store, "7", deep);
memory.rememberPlace(store, "1", { place: "volume:/", path: "srv" });

const out = {
  reload: memory.startingPlace(deep, memory.rememberedPlace(store, "7"), member, "drive"),
  remembered: memory.startingPlace(null, memory.rememberedPlace(store, "7"), member, "drive"),
  other: memory.rememberedPlace(store, "9"),
  administrator: memory.startingPlace(null, memory.rememberedPlace(store, "1"), administrator, "volume:/"),
  unreached: memory.startingPlace(null, { place: "volume:/", path: "srv" }, member, "drive"),
  share: memory.startingPlace(null, inShare, member, "drive"),
  shareEnded: memory.startingPlace(null, inShare, { ...member, shared: [] }, "drive"),
  shareOfAnother: memory.startingPlace(null, inShare, administrator, "volume:/"),
  nothing: memory.startingPlace(null, null, member, "drive"),
  refused: (memory.rememberPlace(refusing, "7", deep), memory.forgetPlace(refusing, "7"), memory.rememberedPlace(refusing, "7")),
  garbled: (store.setItem("bmo-drive.files.place:8", "{not json"), memory.rememberedPlace(store, "8"))
};

memory.forgetPlace(store, "7");
out.forgotten = memory.rememberedPlace(store, "7");

console.log(JSON.stringify(out));
"#;

/// The checks of the shared branch: the tree drawn over places holding two shares and over places holding
/// none, and the roads each share is read by. The relay's settings and the app's base path are the host's,
/// stood in for here.
const SHARED_CHECKS: &str = r#"
globalThis.useAppConfig = () => ({ bmoUi: {} });
globalThis.useRuntimeConfig = () => ({ app: { baseURL: "/" } });

const files = await import("./app/domain/files.ts");
const drive = await import("./app/domain/drive.ts");
const people = await import("./app/domain/people.ts");

const t = (key, values) => (values === undefined ? key : `${key} ${JSON.stringify(values)}`);
const space = { place: "drive", usedBytes: null, quotaBytes: null, error: null, errorCode: null };
const shared = [
  { id: 4, place: files.sharedPlace(4), owner: "3", name: "plans", kind: "folder" },
  { id: 2, place: files.sharedPlace(2), owner: "9", name: "report.pdf", kind: "file" }
];
const places = { administrator: true, viewer: "1", space, machines: [{ key: "local", name: "host", local: true, disks: [], garage: null, error: null, errorCode: null }], shared };
const known = new Map([["shared:4\n", { state: "read", names: ["2025"], more: 0 }]]);
const row = (item) => ({ value: item.value, label: item.label, note: item.note ?? null, readOnly: item.readOnly ?? false, opens: item.folder ?? null, children: item.children?.map(row) ?? null });

const directory = { person: async (user) => (user === "3" ? { user, name: "Kim", organization: "", jobTitle: "" } : null) };
const folder = drive.roadOf(places, "shared:4");
const file = drive.roadOf(places, "shared:2");

const out = {
  tree: files.filesTree(t, places, known, new Map([["3", "Kim"]])).map((item) => item.value),
  branch: row(files.filesTree(t, places, known, new Map([["3", "Kim"]]))[1]),
  empty: row(files.filesTree(t, { ...places, shared: [] }, known)[1]),
  writable: { own: drive.roadOf(places, "drive").writable, folder: folder.writable, file: file.writable },
  folderContent: drive.contentUrl(folder, { place: "shared:4", path: "2025", name: "a b.txt" }, { download: true }),
  fileContent: drive.contentUrl(file, { place: "shared:2", path: "", name: "report.pdf" }, { head: 16 }),
  fileList: await drive.readListing(file, { place: "shared:2", path: "" }, { limit: 200 }),
  fileFolders: await drive.readListing(file, { place: "shared:2", path: "" }, { limit: 1000, folders: true }),
  label: files.placeLabel(t, places, "shared:2"),
  shown: files.shownPath(places, "shared:4", "2025/a.txt"),
  keys: files.keysTo(places, { place: "shared:4", path: "2025/q1" }),
  named: [(await people.personOf(directory, "3")).name, (await people.personOf(directory, "9")).name]
};

console.log(JSON.stringify(out));
"#;

#[test]
fn the_screen_opens_where_the_address_or_the_viewer_left_it() {
  let out = run_checks(MEMORY_CHECKS);

  // A reload keeps the address, and the address names the folder.
  assert_eq!(out["reload"], json!({"place": "drive", "path": "photos/2024"}));
  // With no folder in the address, the viewer's own last folder.
  assert_eq!(out["remembered"], json!({"place": "drive", "path": "photos/2024"}));
  assert_eq!(out["other"], Value::Null);
  assert_eq!(out["administrator"], json!({"place": "volume:/", "path": "srv"}));
  // A place the viewer no longer reaches, or nothing remembered: the first place's top.
  assert_eq!(out["unreached"], json!({"place": "drive", "path": ""}));
  assert_eq!(out["nothing"], json!({"place": "drive", "path": ""}));
  // A share is reached while it stands among those shared with the viewer, and by no one else.
  assert_eq!(out["share"], json!({"place": "shared:4", "path": "2025"}));
  assert_eq!(out["shareEnded"], json!({"place": "drive", "path": ""}));
  assert_eq!(out["shareOfAnother"], json!({"place": "volume:/", "path": ""}));
  // Storage refused or garbled remembers nothing, and nothing fails.
  assert_eq!(out["refused"], Value::Null);
  assert_eq!(out["garbled"], Value::Null);
  assert_eq!(out["forgotten"], Value::Null);
}

#[test]
fn what_is_shared_with_the_owner_stands_as_a_read_only_branch() {
  let out = run_checks(SHARED_CHECKS);

  // The branch stands after the owner's drive and before the machines.
  assert_eq!(out["tree"], json!(["drive\n", "shared", "machine:local"]));

  // Each share is a read-only place named by its sharer once the name is known, and by nothing before. A
  // shared folder opens onto its own folders, read-only all the way down; a shared file stands alone.
  assert_eq!(
    out["branch"],
    json!({
      "value": "shared", "label": "drive.label.shared", "note": null, "readOnly": false, "opens": null,
      "children": [
        {
          "value": "shared:4\n", "label": "plans", "note": "Kim", "readOnly": true, "opens": {"place": "shared:4", "path": ""},
          "children": [{"value": "shared:4\n2025", "label": "2025", "note": null, "readOnly": true, "opens": {"place": "shared:4", "path": "2025"}, "children": [{"value": "shared:4\n2025\n…", "label": "", "note": null, "readOnly": false, "opens": null, "children": null}]}]
        },
        {"value": "shared:2\n", "label": "report.pdf", "note": null, "readOnly": true, "opens": {"place": "shared:2", "path": ""}, "children": null}
      ]
    })
  );
  // A sharer is named by the directory, and a key it answers no person for stands as the key.
  assert_eq!(out["named"], json!(["Kim", "9"]));
  // Nothing shared: the branch says so in a row that opens nothing.
  assert_eq!(
    out["empty"]["children"],
    json!([{"value": "shared\n-", "label": "drive.label.no_shared", "note": null, "readOnly": false, "opens": null, "children": null}])
  );

  // A share is read through the member's own roads, and nothing is written through it.
  assert_eq!(out["writable"], json!({"own": true, "folder": false, "file": false}));
  assert_eq!(out["folderContent"], "/drive/shared/4/content?download=true&path=2025%2Fa+b.txt");
  assert_eq!(out["fileContent"], "/drive/shared/2/content?head=16");
  // A shared file is a place holding that one file, and no folder.
  assert_eq!(out["fileList"]["total"], 1);
  assert_eq!(
    out["fileList"]["entries"],
    json!([{"name": "report.pdf", "kind": "file", "size": null, "modified": null, "link": null}])
  );
  assert_eq!(out["fileFolders"]["entries"], json!([]));

  // The path bar, the copied path and the tree's unfolding run from the share.
  assert_eq!(out["label"], "report.pdf");
  assert_eq!(out["shown"], "plans/2025/a.txt");
  assert_eq!(out["keys"], json!(["shared", "shared:4\n", "shared:4\n2025"]));
}
