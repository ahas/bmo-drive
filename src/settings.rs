//! The settings (`docs/api.md`, 「Settings」): the defaults built in from `settings.toml`, then the
//! settings folder's `settings.toml` and `settings.local.toml` where they stand, then the environment.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use config::{Config, Environment, File, FileFormat};
use serde::Deserialize;

/// The defaults, as the repository's own settings file states them.
const DEFAULTS: &str = include_str!("../settings.toml");
/// The settings file a deployment may give.
pub const SETTINGS_FILE: &str = "settings.toml";
/// The file a deployment overrides it with.
pub const LOCAL_SETTINGS_FILE: &str = "settings.local.toml";
/// The environment variable naming the settings folder.
pub const SETTINGS_FOLDER: &str = "BMO_DRIVE_SETTINGS";
/// What every environment variable that sets a key begins with.
pub const ENVIRONMENT_PREFIX: &str = "BMO_DRIVE";

/// Everything the service and its command are started with.
#[derive(Clone, Debug, Deserialize)]
pub struct Settings {
  pub server: Server,
  pub store: Store,
  pub node: Node,
  pub garage: Garage,
  pub share: Share,
  pub owners: Owners,
  pub storage: Storage,
  pub quota: Quota,
  pub protect: Protect,
}

/// The HTTP server.
#[derive(Clone, Debug, Deserialize)]
pub struct Server {
  /// Where the service answers.
  pub listen: SocketAddr,
  /// The bearer token every caller presents; an empty one lets no caller in.
  pub token: String,
}

/// The store the share links and the kept events stand in.
#[derive(Clone, Debug, Deserialize)]
pub struct Store {
  /// The SQLite file; a relative one is read from where the process starts.
  pub path: PathBuf,
}

/// The node service.
#[derive(Clone, Debug, Deserialize)]
pub struct Node {
  pub address: String,
  pub token: String,
}

/// This machine's Garage.
#[derive(Clone, Debug, Deserialize)]
pub struct Garage {
  /// Garage's own configuration file.
  pub config: PathBuf,
  /// The AWS profile Garage is reached through.
  pub profile: String,
}

/// The share links.
#[derive(Clone, Debug, Deserialize)]
pub struct Share {
  /// The address a link's page stands under: a link's `url` is it, a `/` and the token.
  pub address: String,
  /// Who the command records as a link's maker; none where empty.
  pub actor: String,
}

/// The owners requests act for (`docs/api.md`, 「Owners」).
#[derive(Clone, Debug, Deserialize)]
pub struct Owners {
  /// The administrator's owner key, whose alone the machine branch is; none where empty.
  pub administrator: String,
}

/// What kind of location the BMO drive stands in (`docs/files.md`, 「The BMO drive」).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageKind {
  /// A root folder on this machine.
  #[default]
  Local,
  /// A root folder on a node of the node list.
  Node,
  /// A key prefix in a bucket of an S3 store.
  S3,
}

/// Where the BMO drive stands and how it is kept.
#[derive(Clone, Debug, Deserialize)]
pub struct Storage {
  pub kind: StorageKind,
  /// The root folder of the spaces, or the key prefix in the bucket.
  pub root: String,
  /// The node a `node` location stands on.
  pub node: String,
  /// The bucket of an `s3` location.
  pub bucket: String,
  /// The AWS profile an `s3` location is reached through.
  pub profile: String,
  /// How the storage is backed up, as the owner decided it; none where empty.
  pub backup: String,
}

/// The spaces' quotas.
#[derive(Clone, Debug, Deserialize)]
pub struct Quota {
  /// The quota of a space no quota was set for.
  pub default_bytes: u64,
}

/// Where the file viewer never changes anything (`docs/files.md`, 「Protected locations」).
#[derive(Clone, Debug, Deserialize)]
pub struct Protect {
  pub posix: PosixProtect,
  pub windows: WindowsProtect,
  pub garage: GarageProtect,
  pub local: LocalProtect,
}

/// A machine read by a POSIX shell.
#[derive(Clone, Debug, Deserialize)]
pub struct PosixProtect {
  /// The operating system's own folders.
  pub system: Vec<String>,
  /// Folders whose own entries are never changed.
  pub top: Vec<String>,
  /// Folders under the login's home.
  pub home: Vec<String>,
  /// Every entry of a folder under the home whose name begins with a word.
  pub home_named: Vec<HomeNamed>,
}

/// The entries of one folder under the home whose name begins with a word.
#[derive(Clone, Debug, Deserialize)]
pub struct HomeNamed {
  pub under: String,
  pub starting: String,
}

/// A Windows machine.
#[derive(Clone, Debug, Deserialize)]
pub struct WindowsProtect {
  /// The environment variables naming its protected folders.
  pub system: Vec<String>,
  /// Folders under the user's profile.
  pub home: Vec<String>,
}

/// This machine's Garage.
#[derive(Clone, Debug, Deserialize)]
pub struct GarageProtect {
  /// The words a protected bucket's name begins with.
  pub buckets_starting: Vec<String>,
}

/// This machine besides.
#[derive(Clone, Debug, Deserialize)]
pub struct LocalProtect {
  /// Further folders protected with everything in them.
  pub extra: Vec<PathBuf>,
}

impl Settings {
  /// The settings of this process: its settings folder, then its environment.
  pub fn load() -> Result<Self, SettingsError> {
    Self::read(&folder(), None)
  }

  /// The settings read from `dir`, then from `environment` where it is given and from the process's own
  /// environment where it is not.
  pub fn read(dir: &Path, environment: Option<HashMap<String, String>>) -> Result<Self, SettingsError> {
    let environment = Environment::with_prefix(ENVIRONMENT_PREFIX)
      .prefix_separator("_")
      .separator("__")
      .try_parsing(true)
      .source(environment);

    Ok(
      Config::builder()
        .add_source(File::from_str(DEFAULTS, FileFormat::Toml))
        .add_source(File::from(dir.join(SETTINGS_FILE)).format(FileFormat::Toml).required(false))
        .add_source(File::from(dir.join(LOCAL_SETTINGS_FILE)).format(FileFormat::Toml).required(false))
        .add_source(environment)
        .build()?
        .try_deserialize()?,
    )
  }
}

/// The settings folder: the one the environment names, else the folder above the one holding the binary,
/// a deployment's root.
fn folder() -> PathBuf {
  std::env::var_os(SETTINGS_FOLDER)
    .filter(|named| !named.is_empty())
    .map(PathBuf::from)
    .or_else(|| {
      let binary = std::env::current_exe().ok()?;

      Some(binary.parent()?.parent()?.to_path_buf())
    })
    .unwrap_or_else(|| PathBuf::from("."))
}

/// Why the settings could not be read.
#[derive(Debug, thiserror::Error)]
#[error("the settings could not be read: {0}")]
pub struct SettingsError(#[from] config::ConfigError);
