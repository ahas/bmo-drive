//! This machine's drives as a hardware monitor shows them (`docs/files.md`, 「The drives」): the block
//! devices `lsblk` lists and the file systems mounted on them. Each drive carries the volumes mounted on
//! it, and each volume is a place the viewer walks.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use super::guard::Guard;
use crate::wire;

// File systems that are the kernel's own or a view of another one, never a device's.
const NOT_STORAGE: &[&str] = &[
  "tmpfs",
  "devtmpfs",
  "overlay",
  "squashfs",
  "efivarfs",
  "rootfs",
  "proc",
  "sysfs",
  "ramfs",
  "9p",
  "drvfs",
  "nsfs",
  "autofs",
  "fuse.snapfuse",
  "fuse.portal",
  "fuse.gvfsd-fuse",
  "cgroup2",
  "devpts",
  "tracefs",
  "debugfs",
];

// The file systems that are a network share, each a drive of its own named by its source.
const NETWORK: &[&str] = &["nfs", "nfs4", "cifs", "smb3", "fuse.rclone", "fuse.sshfs"];

/// One drive, named by its model, with the volumes of whichever machine it stands in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Drive<V = Volume> {
  /// Unique within the machine: the kernel's name, or `net:<source>` for a network share.
  pub key: String,
  /// The drive's model, or the kernel's name where it has none, or the share's source.
  pub name: String,
  pub kind: Option<&'static str>,
  pub size_bytes: Option<u64>,
  pub volumes: Vec<V>,
}

impl<V> Drive<V> {
  /// The drive as a caller is told of it, each volume told as `volume` says.
  pub fn wire(&self, volume: impl Fn(&V) -> wire::DriveVolume) -> wire::Drive {
    wire::Drive {
      key: self.key.clone(),
      name: self.name.clone(),
      kind: self.kind.map(str::to_owned),
      byte_count: self.size_bytes,
      volumes: self.volumes.iter().map(volume).collect(),
    }
  }
}

/// A file system mounted on a drive.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
  /// The place the viewer knows it by.
  pub place: String,
  pub mount: String,
  pub fs_type: String,
  /// Where this machine refuses a change.
  #[serde(skip)]
  pub guard: Arc<Guard>,
}

/// The place a volume mounted at `mount` is known by.
pub fn volume_place(mount: &str) -> String {
  format!("volume:{mount}")
}

#[derive(Deserialize)]
struct Blocks {
  blockdevices: Vec<Block>,
}

#[derive(Clone, Deserialize)]
struct Block {
  name: String,
  #[serde(default)]
  pkname: Option<String>,
  #[serde(default)]
  path: Option<String>,
  #[serde(rename = "type", default)]
  kind: String,
  #[serde(default)]
  size: Option<u64>,
  #[serde(default)]
  model: Option<String>,
  #[serde(default)]
  rota: Option<bool>,
  #[serde(default)]
  tran: Option<String>,
}

#[derive(Deserialize)]
struct Mounts {
  filesystems: Vec<Mount>,
}

#[derive(Deserialize)]
struct Mount {
  #[serde(default)]
  source: Option<String>,
  target: String,
  #[serde(default)]
  fstype: Option<String>,
  #[serde(default)]
  fsroot: Option<String>,
}

/// The Windows drives WSL mounts, out of what `findmnt --json --list` said: each source, a letter's
/// root such as `C:\`, with the folder it is mounted at.
pub fn windows_mounts(mounts: &str) -> Result<Vec<(String, String)>, String> {
  let mounts: Mounts = serde_json::from_str(mounts).map_err(|why| format!("findmnt: {why}"))?;

  Ok(
    mounts
      .filesystems
      .into_iter()
      .filter(|mount| matches!(mount.fstype.as_deref(), Some("9p" | "drvfs")))
      .filter_map(|mount| Some((mount.source?, mount.target)))
      .collect(),
  )
}

fn kind_of(disk: &Block) -> &'static str {
  if disk
    .model
    .as_deref()
    .is_some_and(|model| model.to_ascii_lowercase().contains("virtual disk"))
  {
    return "virtual";
  }

  if disk.tran.as_deref() == Some("nvme") || disk.name.starts_with("nvme") {
    return "nvme";
  }

  if disk.tran.as_deref() == Some("usb") {
    return "usb";
  }

  if disk.rota == Some(true) { "hdd" } else { "ssd" }
}

// The disk a block device stands on, following its parents; itself where it is a disk.
fn disk_of<'a>(start: &'a Block, blocks: &'a [Block]) -> Option<&'a Block> {
  let mut current = Some(start);
  let mut steps = 0;

  while let Some(block) = current {
    if block.kind == "disk" {
      return Some(block);
    }

    steps += 1;

    if steps > blocks.len() {
      return None;
    }

    current = block.pkname.as_deref().and_then(|parent| blocks.iter().find(|one| one.name == parent));
  }

  None
}

fn model_name(disk: &Block) -> String {
  disk
    .model
    .as_deref()
    .map(str::trim)
    .filter(|model| !model.is_empty())
    .unwrap_or(&disk.name)
    .to_owned()
}

/// The drives out of what `lsblk --json --list` and `findmnt --json --list` said.
pub fn read_drives(blocks: &str, mounts: &str) -> Result<Vec<Drive>, String> {
  let blocks: Blocks = serde_json::from_str(blocks).map_err(|why| format!("lsblk: {why}"))?;
  let mounts: Mounts = serde_json::from_str(mounts).map_err(|why| format!("findmnt: {why}"))?;
  let blocks = blocks.blockdevices;
  let mut drives: Vec<Drive> = blocks
    .iter()
    .filter(|block| {
      block.kind == "disk" && !["loop", "zram", "ram"].iter().any(|kind| block.name.starts_with(kind))
    })
    .map(|disk| Drive {
      key: disk.name.clone(),
      name: model_name(disk),
      kind: Some(kind_of(disk)),
      size_bytes: disk.size.filter(|size| *size > 0),
      volumes: Vec::new(),
    })
    .collect();
  let mut shares: Vec<Drive> = Vec::new();
  let mut seen: Vec<String> = Vec::new();

  for mount in mounts.filesystems {
    let (Some(source), Some(fs_type)) = (mount.source, mount.fstype) else {
      continue;
    };

    // A bind mount shows a subtree of a file system already standing, or the same source again;
    // the first place a file system stands is the one kept.
    if mount.fsroot.as_deref().is_some_and(|root| root != "/") || seen.contains(&source) {
      continue;
    }

    if NOT_STORAGE.contains(&fs_type.as_str()) || source.starts_with("/dev/loop") {
      continue;
    }

    let volume = Volume {
      place: volume_place(&mount.target),
      mount: mount.target,
      fs_type: fs_type.clone(),
      guard: Arc::default(),
    };

    if NETWORK.contains(&fs_type.as_str()) {
      seen.push(source.clone());
      shares.push(Drive {
        key: format!("net:{source}"),
        name: source,
        kind: Some("network"),
        size_bytes: None,
        volumes: vec![volume],
      });
      continue;
    }

    if !source.starts_with("/dev/") {
      continue;
    }

    seen.push(source.clone());

    let last = source.rsplit('/').next().unwrap_or_default();
    let block = blocks
      .iter()
      .find(|one| one.path.as_deref() == Some(source.as_str()))
      .or_else(|| blocks.iter().find(|one| one.name == last));

    match block.and_then(|block| disk_of(block, &blocks)) {
      Some(disk) => {
        if let Some(drive) = drives.iter_mut().find(|drive| drive.key == disk.name) {
          drive.volumes.push(volume);
        }
      }
      None => shares.push(Drive {
        key: source.clone(),
        name: source,
        kind: None,
        size_bytes: None,
        volumes: vec![volume],
      }),
    }
  }

  drives.extend(shares);

  Ok(drives)
}

async fn listing(program: &str, args: &[&str]) -> Result<String, String> {
  let run = Command::new(program).args(args).kill_on_drop(true).output();
  // Neither listing asks a mount for its sizes, so a share that stopped answering does not hold them.
  let output = run.await.map_err(|why| format!("{program}: {why}"))?;

  if !output.status.success() {
    return Err(format!("{program}: {}", String::from_utf8_lossy(&output.stderr).trim()));
  }

  String::from_utf8(output.stdout).map_err(|why| format!("{program}: {why}"))
}

/// This machine's drives, read now.
pub async fn this_machine() -> Result<Vec<Drive>, String> {
  let (blocks, mounts) = tokio::join!(
    listing(
      "lsblk",
      &["--json", "--list", "--bytes", "-o", "NAME,PKNAME,PATH,TYPE,SIZE,MODEL,ROTA,TRAN"]
    ),
    listing("findmnt", &["--json", "--list", "-o", "SOURCE,TARGET,FSTYPE,FSROOT"]),
  );

  read_drives(&blocks?, &mounts?)
}
