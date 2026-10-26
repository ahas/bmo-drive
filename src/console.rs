//! The console bundle the service serves (`docs/api.md`, 「The console bundle」): its manifest by bmo-ui's
//! console plugin contract, and the module, the stylesheet and the words the manifest names, read from the
//! bundle's built folder once at start. Each file is answered under a path made from its bytes' digest, so
//! the path changes whenever the bytes do.

use std::fmt;
use std::path::Path;

use axum::body::Bytes;
use axum::http::{Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::http::{Call, Refused, Route, answer};
use crate::wire::{ConsoleManifest, ConsoleRole};

/// The least role a reader holds to open the screen: the floor of every route the screen uses.
pub const ROLE: ConsoleRole = ConsoleRole::Member;

/// The reads a link's holder makes with no sign-in (`docs/api.md`, 「Share links」; bmo-ui's
/// `docs/console-bundles.md`, 「Share pages」).
pub const SHARE_READS: [&str; 2] = ["/shares/{token}", "/shares/{token}/content"];

/// The route of the service's API the manifest is answered at, as the contract names it.
pub const MANIFEST_PATH: &str = "/console-manifest";

/// The path the words are answered at.
pub const WORDS_PATH: &str = "/console/words";

// The folder of the service's API the module and the stylesheet stand in.
const FILES_PATH: &str = "/console";

// What the bundle's build writes into its folder.
const MODULE_FILE: &str = "bundle.js";
const STYLESHEET_FILE: &str = "bundle.css";
const WORDS_FILE: &str = "words.json";

// How many hexadecimal digits of a file's digest stand in its path.
const DIGEST_DIGITS: usize = 16;

// A file whose path follows its bytes is never asked for again under that path.
const KEPT_FOR_GOOD: &str = "private, max-age=31536000, immutable";

/// One file of the bundle, as it is answered.
struct Served {
  /// Its name under the files' folder: `bundle-<digest>.js`.
  name: String,
  content_type: &'static str,
  bytes: Bytes,
}

impl Served {
  /// The built file `file` of `folder`, named from its bytes' digest with the extension `extension`.
  fn read(folder: &Path, file: &str, extension: &str, content_type: &'static str) -> Result<Self, String> {
    let path = folder.join(file);
    let bytes =
      std::fs::read(&path).map_err(|why| format!("{} could not be read: {why}", path.display()))?;
    let digest = format!("{:x}", Sha256::digest(&bytes));

    Ok(Self {
      name: format!("bundle-{}.{extension}", &digest[..DIGEST_DIGITS]),
      content_type,
      bytes: Bytes::from(bytes),
    })
  }

  /// The path of the service's API it is answered at.
  fn path(&self) -> String {
    format!("{FILES_PATH}/{}", self.name)
  }
}

/// The bundle as the service serves it: its module, its stylesheet and its words.
pub struct Bundle {
  module: Served,
  stylesheet: Served,
  words: Value,
}

impl fmt::Debug for Bundle {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("Bundle")
      .field("module", &self.module.name)
      .field("stylesheet", &self.stylesheet.name)
      .finish_non_exhaustive()
  }
}

impl Bundle {
  /// The bundle built into `folder`, or why it cannot be served: a file missing, or words that are not a
  /// JSON object.
  pub fn read(folder: &Path) -> Result<Self, String> {
    let module = Served::read(folder, MODULE_FILE, "js", "text/javascript")?;
    let stylesheet = Served::read(folder, STYLESHEET_FILE, "css", "text/css")?;
    let path = folder.join(WORDS_FILE);
    let words =
      std::fs::read(&path).map_err(|why| format!("{} could not be read: {why}", path.display()))?;
    let words: Value =
      serde_json::from_slice(&words).map_err(|why| format!("{} is not JSON: {why}", path.display()))?;

    if !words.is_object() {
      return Err(format!("{} is not one object keyed by locale", path.display()));
    }

    Ok(Self {
      module,
      stylesheet,
      words,
    })
  }

  /// The manifest naming this bundle's files.
  pub fn manifest(&self) -> ConsoleManifest {
    ConsoleManifest {
      name: "drive".into(),
      title: "drive.title".into(),
      icon: "i-material-symbols-folder-open-outline".into(),
      screen_path: "/files".into(),
      service_name: "drive".into(),
      bundle: self.module.path(),
      stylesheet: self.stylesheet.path(),
      words: WORDS_PATH.into(),
      api_prefix: "/".into(),
      role: ROLE,
      owner_key: true,
      events: false,
      websocket: false,
      push_kinds: Vec::new(),
      calls: Vec::new(),
      relays: Vec::new(),
      share_reads: SHARE_READS.iter().map(|path| (*path).to_owned()).collect(),
    }
  }

  /// The words, each locale's tree under its code.
  pub fn words(&self) -> &Value {
    &self.words
  }

  /// The file answered under `name` in the files' folder, where the bundle has one.
  fn file(&self, name: &str) -> Option<&Served> {
    [&self.module, &self.stylesheet].into_iter().find(|served| served.name == name)
  }
}

/// The manifest, to the console's own credential with no owner key and no role.
pub struct ManifestRead;

impl Route for ManifestRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = MANIFEST_PATH;

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    Ok(answer(StatusCode::OK, call.standing().console()?.manifest()))
  }
}

crate::route!(ManifestRead);

/// The words, to the console's own credential with no owner key and no role.
pub struct WordsRead;

impl Route for WordsRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = WORDS_PATH;

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    Ok(answer(StatusCode::OK, call.standing().console()?.words()))
  }
}

crate::route!(WordsRead);

/// The module or the stylesheet, to the console's own credential with no owner key and no role: a link's holder
/// loads them with no sign-in to open the share page, and neither carries an owner's data.
pub struct FileRead;

impl Route for FileRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/console/{file}";

  async fn answer(&self, mut call: Call) -> Result<Response, Refused> {
    let name = call.path_param("file").await?;
    let served = call
      .standing()
      .console()?
      .file(&name)
      .ok_or_else(|| Refused::not_found(format!("the console bundle has no file {name}")))?;

    Ok(
      (
        [(header::CONTENT_TYPE, served.content_type), (header::CACHE_CONTROL, KEPT_FOR_GOOD)],
        served.bytes.clone(),
      )
        .into_response(),
    )
  }
}

crate::route!(FileRead);
