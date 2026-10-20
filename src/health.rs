//! Whether the service is up (`docs/api.md`, 「Health」): the route, and the command that asks it.

use std::time::Duration;

use axum::http::{Method, StatusCode};
use axum::response::Response;

use crate::client::DriveClient;
use crate::http::{Call, Refused, Route, answer};
use crate::settings::Settings;
use crate::wire::Health;

/// Whether the store is open and migrated, and the version. A service that answers has its store: it opens
/// it before it answers anything.
pub struct HealthRead;

impl Route for HealthRead {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/health";

  async fn answer(&self, _call: Call) -> Result<Response, Refused> {
    Ok(answer(
      StatusCode::OK,
      Health {
        ready: true,
        reason: None,
        version: env!("CARGO_PKG_VERSION").to_owned(),
      },
    ))
  }
}

crate::route!(HealthRead);

// How long the command waits on the service.
const WAIT: Duration = Duration::from_secs(5);

/// Whether the service the settings name answers ready, asked once and waited on at most five seconds.
pub async fn ask(settings: &Settings) -> Result<(), String> {
  let health = DriveClient::of_settings(settings)
    .with_timeout(WAIT)
    .health()
    .await
    .map_err(|why| format!("the service at {} did not answer ready: {why}", settings.server.listen))?;

  match health {
    Health { ready: true, .. } => Ok(()),
    Health { reason, .. } => Err(format!(
      "the service is not ready: {}",
      reason.unwrap_or_else(|| "for no stated reason".to_owned())
    )),
  }
}
