//! The drive service: the file viewer over this machine's drives, its Garage and the nodes' volumes, the
//! share links that hand one entry to someone who holds no token, the service's own client and its harness
//! plugin (`docs/files.md`, `docs/api.md`).

pub mod client;
pub mod clients;
pub mod command;
pub mod console;
pub mod events;
pub mod files;
pub mod health;
pub mod http;
pub mod import;
pub mod mcp;
pub mod openapi;
pub mod owner;
pub mod settings;
pub mod store;
pub mod wire;


use tokio::net::TcpListener;

use crate::http::Standing;
use crate::settings::Settings;
use crate::store::Store;

/// The version the drive's health, its MCP server and its command name: the set's version the build was
/// made at, which the testbed hands the build as `BMO_VERSION`, else the manifest's placeholder (bmo-chat's
/// `docs/conventions.md`, 「Versions」).
pub const VERSION: &str = match option_env!("BMO_VERSION") {
  Some(version) => version,
  None => env!("CARGO_PKG_VERSION"),
};

/// Opens the store, then answers the routes on the settings' address until the process is told to stop, and
/// then until every request still in hand has ended, the live event streams ended at once.
pub async fn serve(settings: Settings) -> Result<(), String> {
  let store = Store::open(&settings.store.path).await.map_err(|why| why.to_string())?;

  if !settings.owners.administrator.is_empty() {
    store.adopt(&settings.owners.administrator).await.map_err(|why| why.to_string())?;
  }

  let listen = settings.server.listen;
  let standing = Standing::new(settings, store);

  if let Err(why) = &standing.face {
    eprintln!("bmo-drive: the console face is not served: {why}");
  }

  let events = standing.events.clone();
  let router = http::router(standing).map_err(|faults| faults.join("; "))?;
  let listener = TcpListener::bind(listen)
    .await
    .map_err(|why| format!("{listen} could not be listened on: {why}"))?;

  bmo_app_service::serve_until(listener, router, async move {
    bmo_app_service::shutdown_signal().await;
    events.stop();
  })
  .await
  .map_err(|why| why.to_string())
}
