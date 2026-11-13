//! The service's own client (`docs/api.md`, 「The client」): one method for each route, sending and reading
//! the types of `wire`, acting for the owner key it is given. The command and the MCP server reach the
//! service through it, and so may any other Rust caller.

use axum::body::Bytes;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::{Method, RequestBuilder, Response, StatusCode, header};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::owner::{OWNER_KEY, OWNER_ROLE};
use crate::settings::Settings;
use crate::wire::{
  ArchiveRequest, ConsoleRole, ContentQuery, CopyRequest, EntriesQuery, Entry, EntryMove, EntryPage,
  EventPage, EventsQuery, FolderRequest, Health, Machine, MachineQuery, MemberShare, MemberShareChange,
  MemberSharePage, MemberShareRequest, MemberSharesQuery, NodePage, NodesQuery, Owner, PlacePage,
  PlacesQuery, Quota, QuotaRequest, Share, SharePage, ShareRequest, SharedContentQuery, SharedPage,
  SharedQuery, SharesQuery, Storage, WireError, WriteQuery,
};

// Everything but the unreserved characters is written percent-encoded in a path segment, a `/` and a `:`
// among them, so a place's name or an entry's path stands as the one segment the route names.
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

/// One value written as one path segment.
fn segment(value: &str) -> String {
  utf8_percent_encode(value, SEGMENT).to_string()
}

/// Why a call reached no answer of the route's shape.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ClientFailure {
  /// Nothing answered: the service is not where it was looked for, or its connection broke.
  #[error("the service did not answer: {0}")]
  Unreached(String),
  /// The service refused, in its error shape where it answered in one.
  #[error("the service answered {status}: {}", answer.as_ref().map_or(body.as_str(), |answer| answer.error.as_str()))]
  Answered {
    status: u16,
    answer: Option<WireError>,
    body: String,
  },
  /// The service answered, but not in the route's shape.
  #[error("the service's answer could not be read: {0}")]
  Unread(String),
}

impl ClientFailure {
  /// The code the service refused with, where it refused in its error shape.
  pub fn code(&self) -> Option<&str> {
    match self {
      Self::Answered {
        answer: Some(answer),
        ..
      } => Some(&answer.code),
      _ => None,
    }
  }
}

/// A byte range of a file to read: from `start`, at most `most` bytes where it is given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ByteRange {
  pub start: u64,
  pub most: Option<u64>,
}

impl ByteRange {
  /// The range as a `Range` header writes it.
  fn header(self) -> String {
    match self.most {
      Some(0) | None => format!("bytes={}-", self.start),
      Some(most) => format!("bytes={}-{}", self.start, self.start + most - 1),
    }
  }
}

/// A file's bytes as the service answers them, still to be read.
pub struct Content {
  /// 200 for the whole or its head, 206 for a range, 416 for a range naming no byte.
  pub status: u16,
  /// Where the bytes start in the file.
  pub start: u64,
  /// The whole file's length, where the service said it.
  pub file_byte_count: Option<u64>,
  pub content_type: Option<String>,
  response: Response,
}

impl Content {
  /// Every byte of the answer, read whole.
  pub async fn bytes(self) -> Result<Bytes, ClientFailure> {
    self.response.bytes().await.map_err(|why| ClientFailure::Unreached(why.to_string()))
  }

  /// The answer as it streams.
  pub fn into_response(self) -> Response {
    self.response
  }
}

/// The service at one address, asked with one token, acting for one owner in one role.
#[derive(Clone, Debug)]
pub struct DriveClient {
  address: String,
  token: String,
  /// The owner key sent as `Owner-Key`; none is sent where it is empty.
  owner: String,
  /// The role sent as `Owner-Role` beside the key.
  role: ConsoleRole,
  http: reqwest::Client,
}

impl DriveClient {
  /// The service at `address`, its base, asked with `token`.
  pub fn new(address: impl Into<String>, token: impl Into<String>) -> Self {
    Self {
      address: address.into().trim_end_matches('/').to_owned(),
      token: token.into(),
      owner: String::new(),
      role: ConsoleRole::Member,
      http: reqwest::Client::new(),
    }
  }

  /// The service the settings name, on this machine, with the settings' token, acting for the
  /// administrator's key in the administrator's role.
  pub fn of_settings(settings: &Settings) -> Self {
    Self::new(format!("http://{}", settings.server.listen), settings.server.token.clone())
      .with_owner(settings.owners.administrator.clone())
      .with_role(ConsoleRole::Administrator)
  }

  /// The client acting for the owner `key`, sent as `Owner-Key` on every route.
  pub fn with_owner(mut self, key: impl Into<String>) -> Self {
    self.owner = key.into();
    self
  }

  /// The client acting in `role`, sent as `Owner-Role` beside the key; `member` where none is given.
  pub fn with_role(mut self, role: ConsoleRole) -> Self {
    self.role = role;
    self
  }

  fn request(&self, method: Method, path: &str) -> RequestBuilder {
    let mut request = self.http.request(method, format!("{}{path}", self.address)).bearer_auth(&self.token);

    if !self.owner.is_empty() {
      request = request.header(OWNER_KEY, &self.owner).header(OWNER_ROLE, self.role.name());
    }

    request
  }

  /// A request sent, its answer refused where its status is no success.
  async fn sent(request: RequestBuilder) -> Result<Response, ClientFailure> {
    let response = request.send().await.map_err(|why| ClientFailure::Unreached(why.to_string()))?;
    let status = response.status();

    if status.is_success() || status == StatusCode::RANGE_NOT_SATISFIABLE {
      return Ok(response);
    }

    let body = response.text().await.unwrap_or_default();

    Err(ClientFailure::Answered {
      status: status.as_u16(),
      answer: serde_json::from_str(&body).ok(),
      body,
    })
  }

  /// A request sent and its answer read as `T`.
  async fn read<T: DeserializeOwned>(request: RequestBuilder) -> Result<T, ClientFailure> {
    Self::sent(request)
      .await?
      .json()
      .await
      .map_err(|why| ClientFailure::Unread(why.to_string()))
  }

  fn with_query(request: RequestBuilder, query: &impl Serialize) -> RequestBuilder {
    request.query(query)
  }

  fn entry_path(place_name: &str, entry_path: &str) -> String {
    format!("/places/{}/entries/{}", segment(place_name), segment(entry_path))
  }

  /// `GET /health`.
  pub async fn health(&self) -> Result<Health, ClientFailure> {
    Self::read(self.request(Method::GET, "/health")).await
  }

  /// `GET /events`, as a page.
  pub async fn events(&self, query: &EventsQuery) -> Result<EventPage, ClientFailure> {
    let request = self.request(Method::GET, "/events").header(header::ACCEPT, "application/json");

    Self::read(Self::with_query(request, query)).await
  }

  /// `GET /owner`.
  pub async fn owner(&self) -> Result<Owner, ClientFailure> {
    Self::read(self.request(Method::GET, "/owner")).await
  }

  /// `GET /storage`.
  pub async fn storage(&self) -> Result<Storage, ClientFailure> {
    Self::read(self.request(Method::GET, "/storage")).await
  }

  /// `GET /quotas/{ownerKey}`.
  pub async fn quota(&self, owner_key: &str) -> Result<Quota, ClientFailure> {
    Self::read(self.request(Method::GET, &format!("/quotas/{}", segment(owner_key)))).await
  }

  /// `PUT /quotas/{ownerKey}`.
  pub async fn set_quota(&self, owner_key: &str, quota: &QuotaRequest) -> Result<Quota, ClientFailure> {
    Self::read(self.request(Method::PUT, &format!("/quotas/{}", segment(owner_key))).json(quota)).await
  }

  /// `GET /places`.
  pub async fn places(&self, query: &PlacesQuery) -> Result<PlacePage, ClientFailure> {
    Self::read(Self::with_query(self.request(Method::GET, "/places"), query)).await
  }

  /// `GET /machine`.
  pub async fn machine(&self, query: &MachineQuery) -> Result<Machine, ClientFailure> {
    Self::read(Self::with_query(self.request(Method::GET, "/machine"), query)).await
  }

  /// `GET /nodes`.
  pub async fn nodes(&self, query: &NodesQuery) -> Result<NodePage, ClientFailure> {
    Self::read(Self::with_query(self.request(Method::GET, "/nodes"), query)).await
  }

  /// `GET /places/{placeName}/entries`.
  pub async fn entries(&self, place_name: &str, query: &EntriesQuery) -> Result<EntryPage, ClientFailure> {
    let path = format!("/places/{}/entries", segment(place_name));

    Self::read(Self::with_query(self.request(Method::GET, &path), query)).await
  }

  /// `GET /places/{placeName}/entries/{entryPath}`.
  pub async fn entry(&self, place_name: &str, entry_path: &str) -> Result<Entry, ClientFailure> {
    Self::read(self.request(Method::GET, &Self::entry_path(place_name, entry_path))).await
  }

  /// `PATCH /places/{placeName}/entries/{entryPath}`.
  pub async fn move_entry(
    &self,
    place_name: &str,
    entry_path: &str,
    moved: &EntryMove,
  ) -> Result<Entry, ClientFailure> {
    Self::read(self.request(Method::PATCH, &Self::entry_path(place_name, entry_path)).json(moved)).await
  }

  /// `DELETE /places/{placeName}/entries/{entryPath}`.
  pub async fn delete_entry(&self, place_name: &str, entry_path: &str) -> Result<(), ClientFailure> {
    Self::sent(self.request(Method::DELETE, &Self::entry_path(place_name, entry_path)))
      .await
      .map(|_| ())
  }

  /// `GET /places/{placeName}/entries/{entryPath}/content`, the bytes `range` names where it is given.
  pub async fn content(
    &self,
    place_name: &str,
    entry_path: &str,
    query: &ContentQuery,
    range: Option<ByteRange>,
  ) -> Result<Content, ClientFailure> {
    let path = format!("{}/content", Self::entry_path(place_name, entry_path));

    Self::content_of(Self::with_query(self.request(Method::GET, &path), query), range).await
  }

  /// `PUT /places/{placeName}/entries/{entryPath}/content`, `bytes` the file's whole content.
  pub async fn write_content(
    &self,
    place_name: &str,
    entry_path: &str,
    query: &WriteQuery,
    bytes: Vec<u8>,
  ) -> Result<Entry, ClientFailure> {
    let path = format!("{}/content", Self::entry_path(place_name, entry_path));
    let request = Self::with_query(self.request(Method::PUT, &path), query)
      .header(header::CONTENT_TYPE, "application/octet-stream")
      .body(bytes);

    Self::read(request).await
  }

  /// `POST /places/{placeName}/folders`.
  pub async fn create_folder(
    &self,
    place_name: &str,
    folder: &FolderRequest,
  ) -> Result<Entry, ClientFailure> {
    let path = format!("/places/{}/folders", segment(place_name));

    Self::read(self.request(Method::POST, &path).json(folder)).await
  }

  /// `POST /places/{placeName}/copies`.
  pub async fn copy(&self, place_name: &str, copy: &CopyRequest) -> Result<EntryPage, ClientFailure> {
    let path = format!("/places/{}/copies", segment(place_name));

    Self::read(self.request(Method::POST, &path).json(copy)).await
  }

  /// `POST /places/{placeName}/archives`: the zip, still to be read as it streams.
  pub async fn archive(
    &self,
    place_name: &str,
    archive: &ArchiveRequest,
  ) -> Result<Response, ClientFailure> {
    let path = format!("/places/{}/archives", segment(place_name));

    Self::sent(self.request(Method::POST, &path).json(archive)).await
  }

  /// `GET /shares`.
  pub async fn shares(&self, query: &SharesQuery) -> Result<SharePage, ClientFailure> {
    Self::read(Self::with_query(self.request(Method::GET, "/shares"), query)).await
  }

  /// `POST /shares`: the link made, or the live one already standing for the entry.
  pub async fn create_share(&self, share: &ShareRequest) -> Result<Share, ClientFailure> {
    Self::read(self.request(Method::POST, "/shares").json(share)).await
  }

  /// `GET /shares/{token}`.
  pub async fn share(&self, token: &str) -> Result<Share, ClientFailure> {
    Self::read(self.request(Method::GET, &format!("/shares/{}", segment(token)))).await
  }

  /// `DELETE /shares/{token}`.
  pub async fn delete_share(&self, token: &str) -> Result<(), ClientFailure> {
    Self::sent(self.request(Method::DELETE, &format!("/shares/{}", segment(token))))
      .await
      .map(|_| ())
  }

  /// `GET /shares/{token}/content`, the bytes `range` names where it is given.
  pub async fn share_content(
    &self,
    token: &str,
    range: Option<ByteRange>,
  ) -> Result<Content, ClientFailure> {
    let request = self.request(Method::GET, &format!("/shares/{}/content", segment(token)));

    Self::content_of(request, range).await
  }

  /// `GET /member-shares`.
  pub async fn member_shares(&self, query: &MemberSharesQuery) -> Result<MemberSharePage, ClientFailure> {
    Self::read(Self::with_query(self.request(Method::GET, "/member-shares"), query)).await
  }

  /// `POST /member-shares`: the member shares of the users asked, made now or already standing.
  pub async fn create_member_shares(
    &self,
    shares: &MemberShareRequest,
  ) -> Result<MemberSharePage, ClientFailure> {
    Self::read(self.request(Method::POST, "/member-shares").json(shares)).await
  }

  /// `PATCH /member-shares/{id}`.
  pub async fn update_member_share(
    &self,
    id: i64,
    change: &MemberShareChange,
  ) -> Result<MemberShare, ClientFailure> {
    Self::read(self.request(Method::PATCH, &format!("/member-shares/{id}")).json(change)).await
  }

  /// `DELETE /member-shares/{id}`.
  pub async fn delete_member_share(&self, id: i64) -> Result<(), ClientFailure> {
    Self::sent(self.request(Method::DELETE, &format!("/member-shares/{id}")))
      .await
      .map(|_| ())
  }

  /// `GET /shared`.
  pub async fn shared(&self, query: &SharedQuery) -> Result<SharedPage, ClientFailure> {
    Self::read(Self::with_query(self.request(Method::GET, "/shared"), query)).await
  }

  /// `GET /shared/{id}/entries`.
  pub async fn shared_entries(&self, id: i64, query: &EntriesQuery) -> Result<EntryPage, ClientFailure> {
    Self::read(Self::with_query(self.request(Method::GET, &format!("/shared/{id}/entries")), query)).await
  }

  /// `GET /shared/{id}/content`, the bytes `range` names where it is given.
  pub async fn shared_content(
    &self,
    id: i64,
    query: &SharedContentQuery,
    range: Option<ByteRange>,
  ) -> Result<Content, ClientFailure> {
    let request = Self::with_query(self.request(Method::GET, &format!("/shared/{id}/content")), query);

    Self::content_of(request, range).await
  }

  async fn content_of(request: RequestBuilder, range: Option<ByteRange>) -> Result<Content, ClientFailure> {
    let request = match range {
      Some(range) => request.header(header::RANGE, range.header()),
      None => request,
    };
    let response = Self::sent(request).await?;
    let status = response.status();
    let headers = response.headers();
    let text = |name: &str| headers.get(name).and_then(|value| value.to_str().ok()).map(str::to_owned);
    // A partial answer starts where the range asked, which is the one form of range this client asks.
    let start = match (status, range) {
      (StatusCode::PARTIAL_CONTENT, Some(range)) => range.start,
      _ => 0,
    };

    Ok(Content {
      status: status.as_u16(),
      start,
      file_byte_count: text("x-file-size").and_then(|size| size.parse().ok()),
      content_type: text(header::CONTENT_TYPE.as_str()),
      response,
    })
  }
}
