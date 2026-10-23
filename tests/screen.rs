//! Where the Files screen opens (docs/files.md, 「The Files screen」): a reload keeps the folder the address
//! names, an address naming none opens at the folder the viewer last opened, each viewer's own, a remembered
//! place the viewer no longer reaches gives way to the first place, and storage the browser refuses remembers
//! nothing without failing. The screen's own module runs with Bun in the bundle's project.

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

/// What the checks below print, run with Bun against the screen's place memory.
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

const member = { administrator: false, viewer: "7", space: { place: "drive" }, machines: [] };
const administrator = { administrator: true, viewer: "1", space: { place: "drive" }, machines: [] };
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
  nothing: memory.startingPlace(null, null, member, "drive"),
  refused: (memory.rememberPlace(refusing, "7", deep), memory.forgetPlace(refusing, "7"), memory.rememberedPlace(refusing, "7")),
  garbled: (store.setItem("bmo-drive.files.place:8", "{not json"), memory.rememberedPlace(store, "8"))
};

memory.forgetPlace(store, "7");
out.forgotten = memory.rememberedPlace(store, "7");

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
fn the_screen_opens_where_the_address_or_the_viewer_left_it() {
  let out = run_checks();

  // A reload keeps the address, and the address names the folder.
  assert_eq!(out["reload"], json!({"place": "drive", "path": "photos/2024"}));
  // With no folder in the address, the viewer's own last folder.
  assert_eq!(out["remembered"], json!({"place": "drive", "path": "photos/2024"}));
  assert_eq!(out["other"], Value::Null);
  assert_eq!(out["administrator"], json!({"place": "volume:/", "path": "srv"}));
  // A place the viewer no longer reaches, or nothing remembered: the first place's top.
  assert_eq!(out["unreached"], json!({"place": "drive", "path": ""}));
  assert_eq!(out["nothing"], json!({"place": "drive", "path": ""}));
  // Storage refused or garbled remembers nothing, and nothing fails.
  assert_eq!(out["refused"], Value::Null);
  assert_eq!(out["garbled"], Value::Null);
  assert_eq!(out["forgotten"], Value::Null);
}
