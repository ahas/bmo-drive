//! What is shared with the signed-in owner on the Files screen (docs/files.md, 「The Files screen」): the
//! tree's branch below the drive, each share by its name with its sharer's name beside it, the branch's empty
//! and unread states, each share opened as a read-only place read through the member's own readings of it, a
//! shared file standing as a folder holding it alone, and a remembered share opened again only while it
//! stands. The screen's own modules run with Bun in the bundle's project, the console's fetcher and settings
//! stood in for.

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

/// What the checks below print, run with Bun against the screen's tree, roads and place memory.
fn run_checks() -> Value {
  let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("client");

  if !project.join("node_modules").exists() {
    let installed = Command::new("bun")
      .args(["install", "--frozen-lockfile"])
      .current_dir(&project)
      .status()
      .expect("bun runs");
    assert!(installed.success(), "the bundle's packages are installed");
  }

  let checks = r#"
globalThis.useAppConfig = () => ({ bmoUi: {} });
globalThis.useRuntimeConfig = () => ({ app: { baseURL: "/" } });
const asked = [];
globalThis.$fetch = async (url, options) => {
  asked.push({ url, query: options?.query ?? null });
  return { items: [{ placeName: "shared:5", path: "2026/a.txt", name: "a.txt", kind: "file", byteCount: 3, changedAt: 1 }], totalCount: 1, more: false };
};

const files = await import("./app/domain/files.ts");
const drive = await import("./app/domain/drive.ts");
const memory = await import("./app/domain/place-memory.ts");

const t = (key, params) => (params === undefined ? key : `${key} ${JSON.stringify(params)}`);
const shared = [
  { id: 5, ownerKey: "key-ann", name: "plans", kind: "folder", sharedAt: 2 },
  { id: 9, ownerKey: "key-unknown", name: "notes.txt", kind: "file", sharedAt: 1 }
];
const member = { administrator: false, viewer: "7", space: { place: "drive", usedBytes: 0, quotaBytes: null, error: null, errorCode: null }, shared, machines: [] };
const sharers = new Map([["key-ann", "Ann Tester"], ["key-unknown", "key-unknown"]]);
const row = (item) => ({
  value: item.value,
  label: item.label,
  note: item.note ?? null,
  folder: item.folder ?? null,
  readOnly: item.readOnly ?? false,
  children: item.children === undefined ? null : item.children.map((one) => one.value)
});
const tree = files.filesTree(t, member, new Map(), sharers);
const folderRoad = drive.roadTo(member, "shared:5");
const fileRoad = drive.roadTo(member, "shared:9");

const out = {
  roots: tree.map((item) => item.value),
  branch: row(tree[1]),
  shares: tree[1].children.map(row),
  unnamed: row(files.filesTree(t, member, new Map(), new Map())[1].children[0]),
  empty: row(files.filesTree(t, { ...member, shared: [] }, new Map(), sharers)[1]),
  unread: row(files.filesTree(t, { ...member, shared: null }, new Map(), sharers)[1]),
  keys: files.keysTo(member, { place: "shared:5", path: "2026/q1" }),
  label: files.placeLabel(t, member, "shared:5"),
  shown: files.shownPath(member, "shared:5", "2026/q1.txt"),
  readOnly: [folderRoad.readOnly, fileRoad.readOnly, drive.roadTo(member, "shared:404").readOnly, drive.roadTo(member, "drive").readOnly],
  folderContent: folderRoad.content({ place: "shared:5", path: "2026", name: "q1.txt" }, { download: true }),
  fileContent: fileRoad.content({ place: "shared:9", path: "", name: "notes.txt" }, { head: 16 }),
  folderListing: await folderRoad.listing({ place: "shared:5", path: "2026" }, { after: 0, limit: 200, sort: "size", fresh: true }),
  fileListing: await fileRoad.listing({ place: "shared:9", path: "" }, { after: 0, limit: 200 }),
  fileFolders: (await fileRoad.listing({ place: "shared:9", path: "" }, { folders: true, limit: 1000 })).entries,
  asked,
  remembered: memory.startingPlace(null, { place: "shared:5", path: "2026" }, member, "drive"),
  ended: memory.startingPlace(null, { place: "shared:6", path: "" }, { ...member, administrator: true }, "drive")
};

console.log(JSON.stringify(out));
"#;
  let done = Command::new("bun")
    .args(["--eval", checks])
    .current_dir(&project)
    .output()
    .expect("bun runs");

  assert!(done.status.success(), "the checks run: {}", String::from_utf8_lossy(&done.stderr));

  serde_json::from_slice(&done.stdout).expect("the checks print JSON")
}

#[test]
fn what_is_shared_stands_in_the_tree_and_opens_read_only() {
  let out = run_checks();

  // The branch stands right below the drive, every share in it.
  assert_eq!(out["roots"], json!(["drive\n", "shared"]));
  assert_eq!(out["branch"]["label"], "drive.label.shared");
  assert_eq!(out["branch"]["children"], json!(["shared:5\n", "shared:9\n"]));

  // A shared folder opens onto its own folders; a shared file stands alone. Each names its sharer, a key the
  // directory names no one for standing as the key, and each is read-only.
  assert_eq!(
    out["shares"],
    json!([
      {
        "value": "shared:5\n", "label": "plans", "note": "Ann Tester", "folder": {"place": "shared:5", "path": ""},
        "readOnly": true, "children": ["shared:5\n\n…"]
      },
      {
        "value": "shared:9\n", "label": "notes.txt", "note": "key-unknown", "folder": {"place": "shared:9", "path": ""},
        "readOnly": true, "children": null
      }
    ])
  );
  // A sharer not yet named stands with nothing beside it.
  assert_eq!(out["unnamed"]["note"], Value::Null);
  // Nothing shared says so; a branch the service could not answer says that in its place.
  assert_eq!(out["empty"]["note"], "drive.label.none");
  assert_eq!(out["empty"]["children"], Value::Null);
  assert_eq!(out["unread"]["children"], json!(["shared\n!"]));

  // The tree unfolds from the branch down to a folder of a share, and the path bar names the share.
  assert_eq!(out["keys"], json!(["shared", "shared:5\n", "shared:5\n2026"]));
  assert_eq!(out["label"], "plans");
  assert_eq!(out["shown"], "plans/2026/q1.txt");

  // A share's road is read-only, one no longer standing among those read too; the owner's own is not.
  assert_eq!(out["readOnly"], json!([true, true, true, false]));

  // A share is read through the member's own readings of it: a folder's file by its path under the folder,
  // a shared file at the empty path, and a listing with no fresh reading asked.
  assert_eq!(out["folderContent"], "/drive/shared/5/content?path=2026%2Fq1.txt&download=true");
  assert_eq!(out["fileContent"], "/drive/shared/9/content?head=16");
  assert_eq!(
    out["asked"],
    json!([{"url": "/drive/shared/5/entries", "query": {"path": "2026", "after": "0", "limit": "200", "sort": "byteCount"}}])
  );
  assert_eq!(out["folderListing"]["entries"][0]["name"], "a.txt");
  assert_eq!(
    out["fileListing"],
    json!({
      "place": "shared:9", "path": "", "total": 1, "offset": 0,
      "entries": [{"name": "notes.txt", "kind": "file", "size": null, "modified": null, "link": null}]
    })
  );
  assert_eq!(out["fileFolders"], json!([]));

  // A remembered share opens again while it stands, and gives way to the first place once it is gone.
  assert_eq!(out["remembered"], json!({"place": "shared:5", "path": "2026"}));
  assert_eq!(out["ended"], json!({"place": "drive", "path": ""}));
}
