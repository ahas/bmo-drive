//! The drive service: the file viewer over this machine's drives, its Garage and the nodes' volumes, the
//! share links that hand one entry to someone who holds no token, the service's own client and its harness
//! plugin (`docs/files.md`, `docs/api.md`).

pub mod client;
pub mod clients;
pub mod command;
pub mod events;
pub mod files;
pub mod health;
pub mod http;
pub mod import;
pub mod mcp;
pub mod openapi;
pub mod settings;
pub mod store;
pub mod wire;

use std::io;

use tokio::net::TcpListener;

use crate::http::Standing;
use crate::settings::Settings;
use crate::store::Store;

/// Opens the store, then answers the routes on the settings' address until the process is told to stop.
pub async fn serve(settings: Settings) -> Result<(), String> {
  let store = Store::open(&settings.store.path).await.map_err(|why| why.to_string())?;
  let listen = settings.server.listen;
  let router = http::router(Standing::new(settings, store)).map_err(|faults| faults.join("; "))?;
  let listener = TcpListener::bind(listen)
    .await
    .map_err(|why| format!("{listen} could not be listened on: {why}"))?;

  axum::serve(listener, router)
    .with_graceful_shutdown(stopped())
    .await
    .map_err(|why: io::Error| why.to_string())
}

/// Resolves once the process is told to stop: an interrupt, or the service manager's terminate.
async fn stopped() {
  let interrupt = async {
    let _ = tokio::signal::ctrl_c().await;
  };
  let terminate = async {
    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
      Ok(mut signal) => {
        signal.recv().await;
      }
      Err(_) => std::future::pending().await,
    }
  };

  tokio::select! {
    () = interrupt => {}
    () = terminate => {}
  }
}
