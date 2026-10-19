//! The language a node's road speaks (`docs/files.md`, 「A node's drives」): every reading and every
//! change on a node is one script in it, fenced and guarded on the node itself. Each shell writes its
//! own scripts; what they print and how they end is one record shape and one set of codes for every
//! shell, read here.

use super::super::drives::Drive;
use super::super::{
  Entry, Kind, Refused, Span, cut_short, into_itself, io_kind_refusal, is_folder, not_folder, outside,
  protected, taken,
};

/// How a script ends where it refuses: each code one reason, the same in every shell's scripts.
pub mod code {
  pub const MISSING: i32 = 3;
  pub const OUTSIDE: i32 = 4;
  pub const NOT_FOLDER: i32 = 5;
  pub const IS_FOLDER: i32 = 6;
  pub const DENIED: i32 = 7;
  pub const PROTECTED: i32 = 8;
  pub const TAKEN: i32 = 9;
  pub const CUT_SHORT: i32 = 10;
  pub const INTO_ITSELF: i32 = 11;
}

/// What a code a script ended with says, where it is one of the codes above.
pub fn refusal_of(code: i32) -> Option<Refused> {
  Some(match code {
    code::MISSING => io_kind_refusal(std::io::ErrorKind::NotFound),
    code::OUTSIDE => outside(),
    code::NOT_FOLDER => not_folder(),
    code::IS_FOLDER => is_folder(),
    code::DENIED => io_kind_refusal(std::io::ErrorKind::PermissionDenied),
    code::PROTECTED => protected(),
    code::TAKEN => taken(),
    code::CUT_SHORT => cut_short(),
    code::INTO_ITSELF => into_itself(),
    _ => return None,
  })
}

/// Where a file a script writes is taken from.
pub enum Source {
  /// The script's own standard input.
  Stdin,
  /// A scratch file already on the node, which the script takes away once it is in place: absolute,
  /// or relative to the login's home.
  Scratch(String),
}

/// One thing a script does under a volume's root, each path given as its names from the root.
pub enum Op<'a> {
  /// Every entry of a folder, as listing records.
  List(&'a [String]),
  /// A file's whole length on a line of its own, then the bytes the span names of it.
  Read(&'a [String], Span),
  MakeFolder(&'a [String]),
  /// A file written from `Source`, which must come to the length given; one standing there is replaced.
  Write(&'a [String], &'a Source, u64),
  Rename(&'a [String], &'a str),
  Remove(&'a [String]),
  Copy {
    from: &'a [String],
    to: &'a [String],
    moving: bool,
  },
}

/// A volume as a node's drives reading gives it: where it is mounted, and the folder its walking starts
/// from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rooted {
  pub mount: String,
  pub fs_type: String,
  pub root: String,
}

/// The language one kind of road speaks.
pub trait Shell: Send + Sync {
  /// The script that reads the machine's drives by the rules for that kind of machine.
  fn drives_script(&self) -> String;
  /// What that script printed, read as drives.
  fn read_drives(&self, printed: &[u8]) -> Result<Vec<Drive<Rooted>>, String>;
  /// Whether a name may stand in a path on this shell's machines; `Within` has already refused the
  /// names no machine allows.
  fn allows(&self, name: &str) -> bool;
  /// The script that does `op` under the volume rooted at `root`.
  fn script(&self, root: &str, op: &Op) -> String;
  /// The root of the folder `names` lead to under `root`, in this shell's own way of writing a path.
  fn beneath(&self, root: &str, names: &[String]) -> String;
  /// How an ssh login is handed a script as the command it runs.
  fn login_command(&self, script: &str) -> String;
  /// Whether a script run over an ssh login reads a whole upload from its standard input; where it
  /// cannot, the upload is put into the login's home by SFTP first.
  fn reads_uploads(&self) -> bool;
}

/// A folder's entries out of listing records: each its kind letter (`d`, `f`, anything else), its size,
/// its modification time in seconds since the epoch, and its name, a tab apart, each record ended by a
/// NUL so a name may hold any other character.
pub fn read_listing(printed: &[u8]) -> Vec<Entry> {
  printed
    .split(|byte| *byte == 0)
    .filter(|record| !record.is_empty())
    .filter_map(|record| {
      let record = String::from_utf8_lossy(record);
      let mut cells = record.splitn(4, '\t');
      let (kind, size, modified, name) = (cells.next()?, cells.next()?, cells.next()?, cells.next()?);

      if name.is_empty() || name == "." || name == ".." {
        return None;
      }

      let kind = match kind {
        "d" => Kind::Folder,
        "f" => Kind::File,
        _ => Kind::Other,
      };

      Some(Entry {
        name: name.to_owned(),
        kind,
        size: (kind == Kind::File).then(|| size.parse().ok()).flatten(),
        modified: modified.parse::<f64>().ok().map(|seconds| (seconds * 1000.0).round() as i64),
      })
    })
    .collect()
}

/// Windows' disks and lettered volumes out of the records the Windows drives reading prints: `disk`
/// with its number, model, media, bus and size, and `vol` with its disk, letter, file system and size, a
/// tab apart. Each volume walks from where `root` says its letter stands, and one with nowhere to walk
/// from is left out.
pub fn read_windows_drives(printed: &str, root: impl Fn(&str) -> Option<String>) -> Vec<Drive<Rooted>> {
  let mut drives: Vec<(String, Drive<Rooted>)> = Vec::new();
  let mut volumes: Vec<(String, Rooted)> = Vec::new();

  for line in printed.lines() {
    let cells: Vec<&str> = line.trim_end_matches('\r').split('\t').map(str::trim).collect();

    match cells.as_slice() {
      ["disk", number, model, media, bus, size, ..] => {
        let kind = match (*bus, *media) {
          ("NVMe", _) => Some("nvme"),
          ("USB", _) => Some("usb"),
          (_, "HDD") => Some("hdd"),
          (_, "SSD") => Some("ssd"),
          _ => None,
        };

        drives.push((
          (*number).to_owned(),
          Drive {
            key: format!("win{number}"),
            name: if model.is_empty() { format!("Disk {number}") } else { (*model).to_owned() },
            kind,
            size_bytes: size.parse().ok().filter(|size| *size > 0),
            volumes: Vec::new(),
          },
        ));
      }
      ["vol", disk, letter, fs_type, ..] => {
        if let Some(root) = root(letter) {
          volumes.push((
            (*disk).to_owned(),
            Rooted {
              mount: (*letter).to_owned(),
              fs_type: (*fs_type).to_owned(),
              root,
            },
          ));
        }
      }
      _ => {}
    }
  }

  for (disk, volume) in volumes {
    if let Some((_, drive)) = drives.iter_mut().find(|(number, _)| *number == disk) {
      drive.volumes.push(volume);
    }
  }

  for (_, drive) in &mut drives {
    drive.volumes.sort_by(|one, other| one.mount.cmp(&other.mount));
  }

  drives.into_iter().map(|(_, drive)| drive).collect()
}

/// The script Windows PowerShell runs to print those records.
pub const WINDOWS_DRIVES: &str = r#"$ErrorActionPreference = 'SilentlyContinue'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$t = "`t"
foreach ($d in Get-PhysicalDisk) { "disk$t$($d.DeviceId)$t$($d.FriendlyName)$t$($d.MediaType)$t$($d.BusType)$t$($d.Size)" }
foreach ($p in Get-Partition) {
  if (-not $p.DriveLetter -or $p.DriveLetter -eq [char]0) { continue }
  $v = Get-Volume -DriveLetter $p.DriveLetter
  if ($v -and $v.Size -gt 0) { "vol$t$($p.DiskNumber)$t$($p.DriveLetter):$t$($v.FileSystem)$t$($v.Size)" }
}
"#;

/// A script handed to Windows PowerShell as one encoded command: its text in UTF-16, as base64.
pub fn encoded_command(script: &str) -> String {
  use base64::Engine;

  let units: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();

  base64::engine::general_purpose::STANDARD.encode(units)
}
