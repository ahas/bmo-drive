//! The service's one event stream (`docs/api.md`, 「Events」): every change made through the service written
//! to the store as it is made, the last of them kept, handed out live as server-sent events and, to a reader
//! that polls, as pages.

use std::collections::{BTreeSet, VecDeque};
use std::convert::Infallible;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::http::{Method, StatusCode, header};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures_util::{StreamExt, stream};
use serde::Serialize;
use sqlx::Row;
use tokio::sync::broadcast;

use crate::http::{Call, Refused, Route, answer};
use crate::store::Store;
use crate::wire::{Event, EventPage, EventsQuery};

/// Every kind of event the service publishes, each a resource and its change.
pub const KINDS: &[&str] =
  &["entry.created", "entry.changed", "entry.deleted", "share.created", "share.deleted"];

/// How many events the store keeps: a resume from before them is answered `gone`.
const KEPT: i64 = 10_000;

// How many events a page carries where none is asked, and the most it may.
const PAGE: u64 = 100;
const PAGE_MOST: u64 = 1000;

// How many events wait for a live reader before the slowest falls behind and reads the store instead.
const LIVE_ROOM: usize = 256;

// How often a comment line keeps the stream open.
const KEEP_OPEN: Duration = Duration::from_secs(15);

/// The current time in epoch milliseconds.
pub fn now_ms() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .ok()
    .and_then(|since| i64::try_from(since.as_millis()).ok())
    .unwrap_or_default()
}

/// The event stream: its kept events in the store and its live readers.
#[derive(Clone, Debug)]
pub struct Events {
  store: Store,
  live: broadcast::Sender<Event>,
}

impl Events {
  pub fn new(store: Store) -> Self {
    let (live, _) = broadcast::channel(LIVE_ROOM);

    Self { store, live }
  }

  /// One event of `kind` about `data`, written and handed to every live reader. A change already made is
  /// never refused for its event: an event that cannot be written is said in the log and left out.
  pub async fn publish(&self, kind: &str, data: impl Serialize) {
    if let Err(why) = self.write(kind, data).await {
      eprintln!("bmo-drive: the {kind} event could not be kept: {why}");
    }
  }

  async fn write(&self, kind: &str, data: impl Serialize) -> Result<(), String> {
    let data = serde_json::to_value(data).map_err(|why| why.to_string())?;
    let at = now_ms();
    let pool = self.store.pool();
    let id: i64 = sqlx::query("INSERT INTO events (kind, at, data) VALUES ($1, $2, $3) RETURNING event_id")
      .bind(kind)
      .bind(at)
      .bind(data.to_string())
      .fetch_one(pool)
      .await
      .and_then(|row| row.try_get(0))
      .map_err(|why| why.to_string())?;

    sqlx::query("DELETE FROM events WHERE event_id <= $1")
      .bind(id - KEPT)
      .execute(pool)
      .await
      .map_err(|why| why.to_string())?;

    // No live reader is no failure: the event is kept for whoever reads next.
    let _ = self.live.send(Event {
      id,
      kind: kind.to_owned(),
      at,
      data,
    });

    Ok(())
  }

  /// The kept events after `after` of the kinds asked, at most `limit`, and whether more stand past them.
  /// A resume from before the kept events is refused `gone`.
  async fn after(&self, after: i64, kinds: &Kinds, limit: u64) -> Result<(Vec<Event>, bool), Refused> {
    let pool = self.store.pool();
    let first: Option<i64> = sqlx::query("SELECT MIN(event_id) FROM events")
      .fetch_one(pool)
      .await
      .and_then(|row| row.try_get(0))
      .map_err(Refused::failed)?;

    if first.is_some_and(|first| after < first - 1) {
      return Err(Refused::gone(
        "the events after that one are no longer kept; read the collections again",
      ));
    }

    let rows =
      sqlx::query("SELECT event_id, kind, at, data FROM events WHERE event_id > $1 ORDER BY event_id")
        .bind(after)
        .fetch_all(pool)
        .await
        .map_err(Refused::failed)?;
    let mut events = Vec::new();
    let mut more = false;

    for row in rows {
      let kind: String = row.try_get("kind").map_err(Refused::failed)?;

      if !kinds.wants(&kind) {
        continue;
      }

      if events.len() as u64 >= limit {
        more = true;
        break;
      }

      let data: String = row.try_get("data").map_err(Refused::failed)?;

      events.push(Event {
        id: row.try_get("event_id").map_err(Refused::failed)?,
        kind,
        at: row.try_get("at").map_err(Refused::failed)?,
        data: serde_json::from_str(&data).map_err(Refused::failed)?,
      });
    }

    Ok((events, more))
  }
}

/// The kinds a reader narrows to; every kind where it names none.
#[derive(Clone, Debug, Default)]
struct Kinds(Option<BTreeSet<String>>);

impl Kinds {
  fn read(kinds: Option<&str>) -> Self {
    Self(kinds.filter(|kinds| !kinds.trim().is_empty()).map(|kinds| {
      kinds
        .split(',')
        .map(str::trim)
        .filter(|kind| !kind.is_empty())
        .map(str::to_owned)
        .collect()
    }))
  }

  fn wants(&self, kind: &str) -> bool {
    self.0.as_ref().is_none_or(|kinds| kinds.contains(kind))
  }
}

/// What a live reader has still to be handed and where it stands.
struct Reading {
  events: Events,
  kinds: Kinds,
  waiting: VecDeque<Event>,
  last: i64,
  live: broadcast::Receiver<Event>,
}

impl Reading {
  /// The next event to hand out, read from the store again where the reader fell behind the live ones.
  async fn next(mut self) -> Option<(Event, Self)> {
    loop {
      if let Some(event) = self.waiting.pop_front() {
        self.last = event.id;
        return Some((event, self));
      }

      match self.live.recv().await {
        Ok(event) if event.id > self.last && self.kinds.wants(&event.kind) => {
          self.last = event.id;
          return Some((event, self));
        }
        Ok(_) => {}
        Err(broadcast::error::RecvError::Lagged(_)) => {
          let (behind, _) = self.events.after(self.last, &self.kinds, u64::MAX).await.ok()?;

          self.waiting.extend(behind);
        }
        Err(broadcast::error::RecvError::Closed) => return None,
      }
    }
  }
}

/// The event as the stream writes it: its kind as the SSE event, its number as the SSE id.
fn sse_event(event: &Event) -> SseEvent {
  SseEvent::default()
    .event(&event.kind)
    .id(event.id.to_string())
    .json_data(event)
    .unwrap_or_else(|_| SseEvent::default().comment("an event that could not be written"))
}

/// The service's one event stream, or a page of it to a reader asking for JSON.
pub struct EventStream;

impl Route for EventStream {
  const METHOD: Method = Method::GET;
  const PATH: &'static str = "/events";

  async fn answer(&self, call: Call) -> Result<Response, Refused> {
    let query: EventsQuery = call.query()?;
    let resumed = call
      .header("last-event-id")
      .map(|id| {
        id.trim()
          .parse::<i64>()
          .map_err(|_| Refused::bad_request("Last-Event-ID is not an event number"))
      })
      .transpose()?;
    let after = resumed.or(query.after).unwrap_or(0);
    let kinds = Kinds::read(query.kinds.as_deref());
    let events = call.standing().events.clone();

    if call.accepts_json() {
      let limit = query.limit.unwrap_or(PAGE).clamp(1, PAGE_MOST);
      let (items, more) = events.after(after, &kinds, limit).await?;

      return Ok(answer(StatusCode::OK, EventPage { items, more }));
    }

    // The live readers are joined before the kept events are read, so nothing written between the two
    // is missed; an event read both ways is handed out once, by its number.
    let live = events.live.subscribe();
    let (waiting, _) = events.after(after, &kinds, u64::MAX).await?;
    let reading = Reading {
      events,
      kinds,
      waiting: waiting.into(),
      last: after,
      live,
    };
    let stream = stream::unfold(reading, Reading::next).map(|event| Ok::<_, Infallible>(sse_event(&event)));
    let mut response = Sse::new(stream).keep_alive(KeepAlive::new().interval(KEEP_OPEN)).into_response();

    response
      .headers_mut()
      .insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-store"));

    Ok(response)
  }
}

crate::route!(EventStream);
