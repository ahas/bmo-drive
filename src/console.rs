//! The console face the service serves (`docs/api.md`, 「The console face」), through bmo-chat's
//! `bmo-console-face`: the drive's declaration of the manifest fields it decides, over the bundle build's folder,
//! guarded by the console's credential.

use std::path::Path;

use bmo_console_face::{BundleFolder, ConsoleFace, ConsoleFaceError};
use serde_json::{Value, json};

use crate::settings::Settings;
use crate::wire::ConsoleRole;

/// The manifest fields the drive decides, as bmo-ui's console plugin contract names them.
pub const DECLARATION: &str = include_str!("console.json");

/// The role the declaration names: the least role a reader holds to open the screen, and the floor of every
/// route that takes the owner key (`docs/api.md`, 「Owners」).
pub const ROLE: ConsoleRole = ConsoleRole::Member;

// What the bundle's build writes into its folder.
const MODULE_FILE: &str = "bundle.js";
const STYLESHEET_FILE: &str = "bundle.css";
const WORDS_FILE: &str = "words.json";

/// The face over the bundle built into `folder`, guarded by `credential`, or why it cannot be served.
pub fn face(folder: &Path, credential: &str) -> Result<ConsoleFace, ConsoleFaceError> {
  let bundle = BundleFolder::new(folder, MODULE_FILE, WORDS_FILE).with_stylesheet(STYLESHEET_FILE);

  ConsoleFace::new(DECLARATION, &bundle, credential)
}

/// The face the settings stand, or why none stands: no console credential set, or a bundle the face refuses.
pub fn face_of(settings: &Settings) -> Result<ConsoleFace, String> {
  if settings.console.token.is_empty() {
    return Err("no console credential is set".to_owned());
  }

  face(&settings.console.folder, &settings.console.token).map_err(|why| why.to_string())
}

/// How a console reaches the service, as a deployment's console probe asks for it (`docs/api.md`, 「The command」):
/// the base address the settings name, the console's credential and the service token, which the face refuses.
pub fn reach(settings: &Settings) -> Value {
  json!({
    "address": format!("http://{}", settings.server.listen),
    "consoleToken": settings.console.token,
    "token": settings.server.token,
  })
}
