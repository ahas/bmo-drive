//! This machine's Garage as a place of its own (`docs/files.md`, 「Garage」): where it is installed is
//! read from its own configuration, and it is reached as any S3 client reaches it, through the AWS
//! profile the settings name. The profile's keys sign requests here and go nowhere else.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::OnceLock;
use std::task::{Context, Poll};
use std::time::Duration;

use aws_runtime::env_config::file::EnvConfigFiles;
use aws_runtime::env_config::section::EnvConfigSections;
use aws_runtime::env_config::source;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::timeout::TimeoutConfig;
use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region, RequestChecksumCalculation};
use aws_sdk_s3::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_s3::primitives::ByteStream;
use aws_types::os_shim_internal::{Env, Fs};
use axum::body::{Body, Bytes};

use futures_util::{Stream, StreamExt, TryStreamExt};
use http_body_util::StreamBody;
use hyper::body::Frame;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Deserialize;

use tokio::sync::mpsc;
use tokio_util::io::ReaderStream;

use super::drives::Drive;
use super::{
  Entry, Kind, Opened, Pending, Place, Prefixed, Refused, Span, Upload, Within, into_itself, not_found,
  protected,
};
use crate::settings::Settings;
use crate::wire;

/// The place Garage is known by.
pub const PLACE: &str = "garage";

/// Why Garage cannot be opened: a code and an English sentence.
#[derive(Clone, Debug)]
struct Fault {
  code: &'static str,
  message: String,
}

impl Fault {
  fn new(code: &'static str, message: impl Into<String>) -> Self {
    Self {
      code,
      message: message.into(),
    }
  }
}

/// Where Garage stands on this machine, and how it is reached.
pub struct Garage {
  /// The drive its data directory stands on.
  disk: Option<String>,
  /// Its data and metadata folders, which only Garage itself may write.
  pub own: Vec<PathBuf>,
  reach: Result<Reach, Fault>,
  /// The words a bucket no change may touch begins with.
  protected_buckets: Vec<String>,
  // The client made at its first request, for every request this reading of the machine serves.
  client: OnceLock<Client>,
}

/// What a request to Garage is sent with. It has no `Debug`, so its keys cannot be printed by way of
/// anything that holds it.
struct Reach {
  endpoint: String,
  region: String,
  key_id: String,
  secret: String,
}

#[derive(Deserialize)]
struct Config {
  data_dir: Option<DataDir>,
  #[serde(default)]
  metadata_dir: Option<String>,
}

/// Every folder Garage's configuration names as its own: each data directory and the metadata one.
fn own_dirs(config: &str) -> Vec<PathBuf> {
  let Ok(config) = toml::from_str::<Config>(config) else {
    return Vec::new();
  };
  let data = match config.data_dir {
    Some(DataDir::One(path)) => vec![path],
    Some(DataDir::Several(paths)) => paths.into_iter().map(|one| one.path).collect(),
    None => Vec::new(),
  };

  data.into_iter().chain(config.metadata_dir).map(PathBuf::from).collect()
}

// Garage takes one data directory, or several each with its own capacity.
#[derive(Deserialize)]
#[serde(untagged)]
enum DataDir {
  One(String),
  Several(Vec<DataPath>),
}

#[derive(Deserialize)]
struct DataPath {
  path: String,
}

const CONFIG_UNREADABLE: &str = "garage-config-unreadable";
const AWS_CONFIG_UNREADABLE: &str = "aws-config-unreadable";

/// The data directory Garage's configuration names, or why it could not be read.
fn data_dir(config: &str) -> Result<Option<String>, Fault> {
  let config: Config = toml::from_str(config).map_err(|why| {
    Fault::new(CONFIG_UNREADABLE, format!("Garage's configuration could not be read: {why}"))
  })?;

  Ok(match config.data_dir {
    Some(DataDir::One(path)) => Some(path),
    Some(DataDir::Several(paths)) => paths.into_iter().next().map(|one| one.path),
    None => None,
  })
}

/// The drive a path stands on: the one holding the volume whose mount point is the longest that holds
/// it.
fn standing_on(path: &Path, drives: &[Drive]) -> Option<String> {
  drives
    .iter()
    .flat_map(|drive| drive.volumes.iter().map(move |volume| (drive, volume)))
    .filter(|(_, volume)| path.starts_with(&volume.mount))
    .max_by_key(|(_, volume)| volume.mount.len())
    .map(|(drive, _)| drive.key.clone())
}

/// How Garage is reached, out of the profile the AWS configuration files hold.
fn reach(sections: &EnvConfigSections, profile_name: &str) -> Result<Reach, Fault> {
  let profile = sections.get_profile(profile_name).ok_or_else(|| {
    Fault::new("garage-profile-missing", format!("the AWS profile {profile_name} is not configured"))
  })?;
  let field = |name: &str| {
    profile.get(name).map(str::to_owned).filter(|value| !value.is_empty()).ok_or_else(|| {
      Fault::new("garage-profile-incomplete", format!("the AWS profile {profile_name} has no {name}"))
    })
  };

  Ok(Reach {
    endpoint: field("endpoint_url")?,
    region: field("region")?,
    key_id: field("aws_access_key_id")?,
    secret: field("aws_secret_access_key")?,
  })
}

async fn read_reach(files: &EnvConfigFiles, env: &Env, fs: &Fs, profile: &str) -> Result<Reach, Fault> {
  let unreadable = |why: String| {
    Fault::new(AWS_CONFIG_UNREADABLE, format!("the AWS configuration could not be read: {why}"))
  };
  let loaded = source::load(env, fs, files).await.map_err(|why| unreadable(why.to_string()))?;
  let sections = EnvConfigSections::parse(loaded).map_err(|why| unreadable(why.to_string()))?;

  reach(&sections, profile)
}

/// This machine's Garage, where its configuration stands.
pub async fn this_machine(drives: &[Drive], settings: &Settings) -> Option<Garage> {
  let file = &settings.garage.config;
  let text = match tokio::fs::read_to_string(file).await {
    Err(why) if why.kind() == std::io::ErrorKind::NotFound => return None,
    Err(why) => Err(Fault::new(
      CONFIG_UNREADABLE,
      format!("Garage's configuration ({}) could not be read: {why}", file.display()),
    )),
    Ok(text) => Ok(text),
  };
  let config = text.as_deref().map_err(Clone::clone).and_then(data_dir);
  let reach =
    read_reach(&EnvConfigFiles::default(), &Env::real(), &Fs::real(), &settings.garage.profile).await;
  let mut garage = Garage::new(config, reach, drives);

  garage.own = text.as_deref().map(own_dirs).unwrap_or_default();
  garage.protected_buckets = settings.protect.garage.buckets_starting.clone();

  Some(garage)
}

impl Garage {
  fn new(config: Result<Option<String>, Fault>, reach: Result<Reach, Fault>, drives: &[Drive]) -> Self {
    let (data_dir, reach) = match config {
      Ok(data_dir) => (data_dir, reach),
      Err(why) => (None, Err(why)),
    };
    let real = data_dir
      .as_deref()
      .map(|dir| std::fs::canonicalize(dir).unwrap_or_else(|_| PathBuf::from(dir)));
    let disk = real.and_then(|dir| standing_on(&dir, drives));

    Self {
      disk,
      own: Vec::new(),
      reach,
      protected_buckets: Vec::new(),
      client: OnceLock::new(),
    }
  }

  /// The place as a caller is told of it: the drive its data stands on, and why it cannot be opened where
  /// it cannot.
  pub fn place(&self) -> wire::Place {
    let fault = self.reach.as_ref().err();

    wire::Place {
      name: PLACE.to_owned(),
      kind: wire::PlaceKind::Store,
      label: "Garage".to_owned(),
      free_bytes: None,
      node_name: None,
      drive_key: self.disk.clone(),
      mount: None,
      reason: fault.map(|fault| fault.message.clone()),
      reason_code: fault.map(|fault| fault.code.to_owned()),
    }
  }

  fn client(&self) -> Result<&Client, Refused> {
    if let Some(made) = self.client.get() {
      return Ok(made);
    }

    let reach = self.reach.as_ref().map_err(|fault| {
      Refused::rule("garage-closed", format!("Garage cannot be opened: {}", fault.message))
    })?;
    let timeouts = TimeoutConfig::builder()
      .connect_timeout(Duration::from_secs(3))
      .operation_attempt_timeout(Duration::from_secs(20))
      .build();
    let config = aws_sdk_s3::Config::builder()
      .behavior_version(BehaviorVersion::latest())
      .region(Region::new(reach.region.clone()))
      .endpoint_url(&reach.endpoint)
      .credentials_provider(Credentials::new(&reach.key_id, &reach.secret, None, None, "garage profile"))
      .force_path_style(true)
      // A streamed upload carries no checksum trailer, which not every S3 server reads.
      .request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
      .timeout_config(timeouts)
      .build();

    Ok(self.client.get_or_init(|| Client::from_conf(config)))
  }

  async fn buckets(&self) -> Result<Vec<Entry>, Refused> {
    let listed = self.client()?.list_buckets().send().await.map_err(refused)?;

    Ok(
      listed
        .buckets()
        .iter()
        .filter_map(|bucket| {
          Some(Entry {
            name: bucket.name()?.to_owned(),
            kind: Kind::Folder,
            size: None,
            modified: bucket.creation_date().and_then(|at| at.to_millis().ok()),
          })
        })
        .collect(),
    )
  }

  /// One level of a bucket: the prefixes up to the next `/` as folders, the objects at it as files.
  async fn level(&self, bucket: &str, names: &[String]) -> Result<Vec<Entry>, Refused> {
    let prefix = if names.is_empty() { String::new() } else { format!("{}/", names.join("/")) };
    let mut pages = self
      .client()?
      .list_objects_v2()
      .bucket(bucket)
      .prefix(&prefix)
      .delimiter("/")
      .into_paginator()
      .send();
    let mut entries = Vec::new();

    while let Some(page) = pages.next().await {
      let page = page.map_err(refused)?;

      for common in page.common_prefixes() {
        if let Some(name) = common.prefix().and_then(|full| full.strip_prefix(&prefix)) {
          let name = name.trim_end_matches('/');

          if !name.is_empty() {
            entries.push(Entry {
              name: name.to_owned(),
              kind: Kind::Folder,
              size: None,
              modified: None,
            });
          }
        }
      }

      for object in page.contents() {
        let Some(name) = object.key().and_then(|key| key.strip_prefix(&prefix)) else {
          continue;
        };

        if !name.is_empty() {
          entries.push(Entry {
            name: name.to_owned(),
            kind: Kind::File,
            size: object.size().and_then(|size| u64::try_from(size).ok()),
            modified: object.last_modified().and_then(|at| at.to_millis().ok()),
          });
        }
      }
    }

    Ok(entries)
  }

  async fn object(&self, within: &Within, span: Span) -> Result<Opened, Refused> {
    let bucket_not_file = || Refused::rule("bucket-not-file", "a bucket cannot be read as a file");
    let [bucket, key @ ..] = within.names.as_slice() else {
      return Err(bucket_not_file());
    };

    if key.is_empty() {
      return Err(bucket_not_file());
    }

    let client = self.client()?;
    let key = key.join("/");
    let size = client
      .head_object()
      .bucket(bucket)
      .key(&key)
      .send()
      .await
      .map_err(refused)?
      .content_length();
    let size = size.and_then(|size| u64::try_from(size).ok()).unwrap_or(0);
    let (start, length) = span.within(size);
    let mut asked = client.get_object().bucket(bucket).key(&key);

    if length < size {
      asked = asked.range(format!("bytes={start}-{}", (start + length).saturating_sub(1)));
    }

    // An empty part is asked of nothing: a range cannot name no bytes.
    let body = if length == 0 && size > 0 {
      Body::empty()
    } else {
      let got = asked.send().await.map_err(refused)?;

      Body::from_stream(ReaderStream::new(got.body.into_async_read()))
    };

    Ok(Opened {
      name: within.last().unwrap_or_default().to_owned(),
      size,
      start,
      length,
      body,
    })
  }
}

/// A refusal of Garage's said by what it was, never by what the request was signed with.
fn refused<E: ProvideErrorMetadata, R>(error: SdkError<E, R>) -> Refused {
  let (code, message) = match &error {
    SdkError::ServiceError(service) => {
      (service.err().code().map(str::to_owned), service.err().message().map(str::to_owned))
    }
    SdkError::DispatchFailure(_) | SdkError::TimeoutError(_) => {
      (None, Some("it could not be reached".to_owned()))
    }
    _ => (None, None),
  };
  match code.as_deref() {
    Some("NoSuchBucket" | "NoSuchKey" | "NotFound") => return not_found(),
    // A refusal of the key says nothing of Garage's own words, which name the key.
    Some("AccessDenied" | "Forbidden" | "InvalidAccessKeyId" | "SignatureDoesNotMatch") => {
      return Refused::upstream("Garage refused the profile's key");
    }
    Some("BucketNotEmpty") => {
      return Refused::conflict("the bucket is not empty");
    }
    _ => {}
  }

  let said = [code, message].into_iter().flatten().collect::<Vec<_>>().join(": ");

  Refused::upstream(format!(
    "Garage did not answer: {}",
    if said.is_empty() { "for no stated reason" } else { &said }
  ))
}

impl Place for Garage {
  fn id(&self) -> &str {
    PLACE
  }

  fn read_folder<'a>(&'a self, within: &'a Within) -> Pending<'a, Vec<Entry>> {
    Box::pin(async move {
      match within.names.as_slice() {
        [] => self.buckets().await,
        [bucket, names @ ..] => self.level(bucket, names).await,
      }
    })
  }

  fn open<'a>(&'a self, within: &'a Within, span: Span) -> Pending<'a, Opened> {
    Box::pin(self.object(within, span))
  }

  fn make_folder<'a>(&'a self, within: &'a Within) -> Pending<'a, ()> {
    Box::pin(async move {
      let client = self.client()?;

      match self.spot(within)? {
        Spot::Bucket(bucket) => {
          client.create_bucket().bucket(bucket).send().await.map_err(refused)?;
        }
        Spot::Key(bucket, key) => {
          client
            .put_object()
            .bucket(bucket)
            .key(format!("{key}/"))
            .send()
            .await
            .map_err(refused)?;
        }
      }

      Ok(())
    })
  }

  fn write<'a>(&'a self, within: &'a Within, upload: Upload) -> Pending<'a, ()> {
    Box::pin(async move {
      let Spot::Key(bucket, key) = self.spot(within)? else {
        return Err(outside_bucket());
      };
      let length = i64::try_from(upload.length).map_err(Refused::failed)?;
      let body = ByteStream::from_body_1_x(StreamBody::new(Passed::of(upload).map_ok(Frame::data)));

      self
        .client()?
        .put_object()
        .bucket(bucket)
        .key(key)
        .content_length(length)
        .body(body)
        .customize()
        .disable_payload_signing()
        .send()
        .await
        .map_err(refused)?;

      Ok(())
    })
  }

  fn rename<'a>(&'a self, within: &'a Within, name: &'a str) -> Pending<'a, ()> {
    Box::pin(async move {
      if matches!(self.spot(within)?, Spot::Bucket(_)) {
        return Err(Refused::rule("bucket-rename", "a bucket cannot be renamed"));
      }

      self.carry(within, &within.sibling(name), true).await
    })
  }

  fn remove<'a>(&'a self, within: &'a Within) -> Pending<'a, ()> {
    Box::pin(async move {
      let client = self.client()?;

      match self.spot(within)? {
        Spot::Bucket(bucket) => {
          client.delete_bucket().bucket(bucket).send().await.map_err(refused)?;
        }
        Spot::Key(bucket, key) => {
          for one in self.objects(bucket, &key).await? {
            client.delete_object().bucket(bucket).key(one).send().await.map_err(refused)?;
          }
        }
      }

      Ok(())
    })
  }

  fn copy_here<'a>(&'a self, from: &'a Within, to: &'a Within, moving: bool) -> Pending<'a, ()> {
    Box::pin(async move {
      if moving && matches!(self.spot(from)?, Spot::Bucket(_)) {
        return Err(Refused::rule("bucket-move", "a bucket cannot be moved"));
      }

      self.carry(from, to, moving).await
    })
  }

  // A prefix holds no links, so a folder of Garage is reached by its path alone.
  fn beneath<'a>(&'a self, top: &'a Within) -> Pending<'a, Box<dyn Place + 'a>> {
    Box::pin(async move {
      Ok(Box::new(Prefixed {
        place: self,
        top: top.clone(),
      }) as Box<dyn Place + 'a>)
    })
  }
}

fn outside_bucket() -> Refused {
  Refused::rule("outside-bucket", "a file cannot stand outside every bucket")
}

/// An upload's bytes handed over through a channel: the SDK takes only a body it may share between
/// threads, which a request's own body is not.
struct Passed(mpsc::Receiver<std::io::Result<Bytes>>);

impl Passed {
  fn of(mut upload: Upload) -> Self {
    let (sending, received) = mpsc::channel(4);

    tokio::spawn(async move {
      while let Some(chunk) = upload.body.next().await {
        if sending.send(chunk).await.is_err() {
          break;
        }
      }
    });

    Self(received)
  }
}

impl Stream for Passed {
  type Item = std::io::Result<Bytes>;

  fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    self.0.poll_recv(context)
  }
}

/// Where in Garage a path lands: a bucket itself, or a key inside one.
enum Spot<'a> {
  Bucket(&'a str),
  Key(&'a str, String),
}

impl<'a> Spot<'a> {
  /// Where a path lands, to be read: a protected bucket is read like any other.
  fn read(within: &'a Within) -> Result<Self, Refused> {
    let [bucket, key @ ..] = within.names.as_slice() else {
      return Err(Refused::rule("no-bucket", "no bucket was named"));
    };

    Ok(if key.is_empty() { Self::Bucket(bucket) } else { Self::Key(bucket, key.join("/")) })
  }
}

/// A copy's source as Garage reads it: its bucket and its key, each path segment encoded.
fn copy_source(bucket: &str, key: &str) -> String {
  let encoded: Vec<String> = key
    .split('/')
    .map(|segment| utf8_percent_encode(segment, NON_ALPHANUMERIC).to_string())
    .collect();

  format!("{bucket}/{}", encoded.join("/"))
}

impl Garage {
  /// Where a path lands, to be changed: never in a bucket the settings protect.
  fn spot<'a>(&self, within: &'a Within) -> Result<Spot<'a>, Refused> {
    let [bucket, key @ ..] = within.names.as_slice() else {
      return Err(protected());
    };

    if self.protected_buckets.iter().any(|start| bucket.starts_with(start.as_str())) {
      return Err(protected());
    }

    Ok(if key.is_empty() { Spot::Bucket(bucket) } else { Spot::Key(bucket, key.join("/")) })
  }

  /// The keys that make up the entry at `key`: the object itself where it is one, else every object
  /// under it as a prefix, its own folder marker included.
  async fn objects(&self, bucket: &str, key: &str) -> Result<Vec<String>, Refused> {
    let client = self.client()?;

    if !key.is_empty() && client.head_object().bucket(bucket).key(key).send().await.is_ok() {
      return Ok(vec![key.to_owned()]);
    }

    let prefix = if key.is_empty() { String::new() } else { format!("{key}/") };
    let mut pages = client.list_objects_v2().bucket(bucket).prefix(&prefix).into_paginator().send();
    let mut keys = Vec::new();

    while let Some(page) = pages.next().await {
      keys.extend(page.map_err(refused)?.contents().iter().filter_map(|one| one.key().map(str::to_owned)));
    }

    if keys.is_empty() && !key.is_empty() {
      return Err(not_found());
    }

    Ok(keys)
  }

  /// The entry at `from` copied to `to` by Garage's own copy, each object under it in turn, and taken
  /// away from where it was where it is a move.
  async fn carry(&self, from: &Within, to: &Within, moving: bool) -> Result<(), Refused> {
    // A copy only reads what it takes; a move changes it.
    let source = if moving { self.spot(from)? } else { Spot::read(from)? };
    let (from_bucket, from_key) = match source {
      Spot::Bucket(bucket) => (bucket, String::new()),
      Spot::Key(bucket, key) => (bucket, key),
    };
    let Spot::Key(to_bucket, to_key) = self.spot(to)? else {
      return Err(outside_bucket());
    };

    if from_bucket == to_bucket
      && (from_key.is_empty() || to_key == from_key || to_key.starts_with(&format!("{from_key}/")))
    {
      return Err(into_itself());
    }

    let client = self.client()?;
    let keys = self.objects(from_bucket, &from_key).await?;

    for key in &keys {
      let rest = key.strip_prefix(&from_key).unwrap_or(key);
      let target = if from_key.is_empty() { format!("{to_key}/{rest}") } else { format!("{to_key}{rest}") };

      client
        .copy_object()
        .copy_source(copy_source(from_bucket, key))
        .bucket(to_bucket)
        .key(target)
        .send()
        .await
        .map_err(refused)?;
    }

    if moving {
      for key in &keys {
        client.delete_object().bucket(from_bucket).key(key).send().await.map_err(refused)?;
      }
    }

    Ok(())
  }
}
