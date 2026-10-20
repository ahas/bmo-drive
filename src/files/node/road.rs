//! A node's own road (`docs/files.md`, 「A node's drives」): a batch ssh login with the arguments the node
//! service hands out, or a jake hub's exec aimed at the node's agent. A script runs over it with its
//! output handed on as it comes and its ending told apart; whoever stops listening stops the script. A
//! key or a token is read for the one request that needs it and is part of no answer.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, LazyLock};

use axum::body::Bytes;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures_util::StreamExt;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde_json::{Value, json};
use sse_stream::SseStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};

use super::super::{Pending, Refused, Upload};
use super::shell::{Shell, Source};

/// How a script run over a road ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Ended {
  /// It ran and ended with this code, and the last thing it said on its standard error.
  Code(i32, String),
  /// The road itself failed, saying why.
  Failed(String),
}

/// A script under way: its output as it comes, and its ending once it has ended.
pub struct Running {
  pub output: mpsc::Receiver<Bytes>,
  pub ended: oneshot::Receiver<Ended>,
}

/// One kind of road to a node.
pub trait Road: Send + Sync {
  /// The language the road's shell speaks.
  fn shell(&self) -> &dyn Shell;
  /// Starts a script, its standard input fed from `input` where one is given.
  fn run(&self, script: String, input: Option<Upload>) -> Running;
  /// Puts an upload where a script can take it from, with what is left to feed its standard input.
  fn hand(&self, upload: Upload) -> Pending<'_, (Source, Option<Upload>)>;
}

// How many chunks of a script's output wait for whoever reads them before the script is held.
const WAITING_CHUNKS: usize = 8;

// The most a road's failure says.
const DETAIL_MOST: usize = 200;

fn last_line(text: &str) -> String {
  text
    .lines()
    .map(str::trim)
    .rfind(|line| !line.is_empty())
    .map(|line| line.chars().take(DETAIL_MOST).collect())
    .unwrap_or_default()
}

fn upload_failed(why: String) -> Refused {
  Refused::upstream(format!("the upload could not be put onto the node: {why}"))
}

/// An ssh node's road: a batch login with the arguments the node service hands out for it.
pub struct SshRoad {
  /// The login's arguments, ahead of the command it runs: its options, the key, the port where it is
  /// not 22, and `user@host` last (`docs/api.md`, 「The node service」).
  pub login: Vec<String>,
  pub shell: Arc<dyn Shell>,
}

impl Road for SshRoad {
  fn shell(&self) -> &dyn Shell {
    &*self.shell
  }

  fn run(&self, script: String, input: Option<Upload>) -> Running {
    let mut args = self.login.clone();

    args.push(self.shell.login_command(&script));

    let (output, waiting) = mpsc::channel(WAITING_CHUNKS);
    let (ending, ended) = oneshot::channel();

    tokio::spawn(async move {
      let _ = ending.send(run_ssh(args, input, output).await);
    });

    Running {
      output: waiting,
      ended,
    }
  }

  fn hand(&self, upload: Upload) -> Pending<'_, (Source, Option<Upload>)> {
    Box::pin(async move {
      if self.shell.reads_uploads() {
        return Ok((Source::Stdin, Some(upload)));
      }

      let name = self.put(upload).await.map_err(upload_failed)?;

      Ok((Source::Scratch(name), None))
    })
  }
}

impl SshRoad {
  /// The login's arguments in SFTP's own form: the options and the key as they stand, the port given as
  /// SFTP takes it, and the batch file before the address. The login's documented order puts
  /// `user@host` last and the port, where there is one, just before it.
  fn sftp_args(&self, batch: &str) -> Vec<String> {
    let mut args = self.login.clone();
    let address = args.pop().unwrap_or_default();

    if let [.., flag, port] = args.as_slice()
      && flag == "-p"
    {
      let port = port.clone();

      args.truncate(args.len() - 2);
      args.extend(["-P".to_owned(), port]);
    }

    args.extend(["-b".to_owned(), batch.to_owned(), address]);
    args
  }

  /// An upload put into the login's home by SFTP, under a scratch name it answers with. SFTP sends
  /// only a file standing on the disk, so the upload is first written to one here, and taken away
  /// after.
  async fn put(&self, mut upload: Upload) -> Result<String, String> {
    let name = format!(".bmo-files-{:016x}", rand::random::<u64>());
    let spool = std::env::temp_dir().join(&name);
    let batch = std::env::temp_dir().join(format!("{name}.batch"));
    let sent = async {
      let mut file = tokio::fs::File::create_new(&spool).await.map_err(|why| why.to_string())?;
      let mut written: u64 = 0;

      while let Some(chunk) = upload.body.next().await {
        let chunk = chunk.map_err(|_| "the upload broke off".to_owned())?;

        written += chunk.len() as u64;
        file.write_all(&chunk).await.map_err(|why| why.to_string())?;
      }

      file.flush().await.map_err(|why| why.to_string())?;

      if written != upload.length {
        return Err("the upload broke off".to_owned());
      }

      // Both names are this service's own, a temporary folder and a hex number: nothing in them needs
      // quoting in a batch line.
      tokio::fs::write(&batch, format!("put {} {name}\n", spool.display()))
        .await
        .map_err(|why| why.to_string())?;

      let done = Command::new("sftp")
        .args(self.sftp_args(&batch.to_string_lossy()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|why| format!("sftp could not be started: {why}"))?;

      if done.status.success() {
        Ok(name.clone())
      } else {
        Err(last_line(&String::from_utf8_lossy(&done.stderr)))
      }
    }
    .await;

    let _ = tokio::fs::remove_file(&spool).await;
    let _ = tokio::fs::remove_file(&batch).await;

    sent
  }
}

async fn run_ssh(args: Vec<String>, input: Option<Upload>, output: mpsc::Sender<Bytes>) -> Ended {
  let started = Command::new("ssh")
    .args(&args)
    .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true)
    .spawn();
  let mut child = match started {
    Ok(child) => child,
    Err(why) => return Ended::Failed(format!("ssh could not be started: {why}")),
  };

  if let (Some(mut upload), Some(mut stdin)) = (input, child.stdin.take()) {
    tokio::spawn(async move {
      // A body that breaks off closes the standard input short, which the script finds by its length.
      while let Some(Ok(chunk)) = upload.body.next().await {
        if stdin.write_all(&chunk).await.is_err() {
          break;
        }
      }
    });
  }

  let said = child.stderr.take().map(|mut stderr| {
    tokio::spawn(async move {
      let mut said = String::new();
      let _ = stderr.read_to_string(&mut said).await;
      said
    })
  });

  if let Some(mut stdout) = child.stdout.take() {
    let mut buffer = vec![0; 64 * 1024];

    loop {
      match stdout.read(&mut buffer).await {
        Ok(0) | Err(_) => break,
        Ok(read) => {
          if output.send(Bytes::copy_from_slice(&buffer[..read])).await.is_err() {
            let _ = child.kill().await;
            return Ended::Failed("nobody listened".into());
          }
        }
      }
    }
  }

  let code = child.wait().await.ok().and_then(|status| status.code());
  let said = match said {
    Some(task) => task.await.unwrap_or_default(),
    None => String::new(),
  };

  match code {
    // OpenSSH's own failure: the login told by the last line it wrote.
    None => Ended::Failed("the login did not finish in time".to_owned()),
    Some(255) => Ended::Failed(
      Some(last_line(&said))
        .filter(|line| !line.is_empty())
        .unwrap_or_else(|| "ssh ended with 255".into()),
    ),
    Some(code) => Ended::Code(code, last_line(&said)),
  }
}

/// A jake node's road: its hub's exec aimed at its own agent.
pub struct JakeRoad {
  pub hub_url: String,
  /// The hub's token file, read for each request.
  pub token_file: String,
  pub agent: String,
  pub shell: Arc<dyn Shell>,
}

static HTTP: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

// The key a hub's token file holds its control token under: the hub's own file format.
const TOKEN_KEY: &str = "JAKE_CONTROL_TOKEN";

fn encoded(value: &str) -> String {
  utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

/// A path as the node service writes it, `~/` standing for this process's home.
fn expand_home(path: &str) -> PathBuf {
  let home = || std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();

  match path {
    "~" => home(),
    _ => match path.strip_prefix("~/") {
      Some(rest) => home().join(rest),
      None => PathBuf::from(path),
    },
  }
}

impl JakeRoad {
  /// The control token out of the hub's token file, or what stood in the way of reading it.
  fn token(&self) -> Result<String, String> {
    let unread = || format!("the token file {} cannot be read", self.token_file);
    let pairs = dotenvy::from_path_iter(expand_home(&self.token_file)).map_err(|_| unread())?;
    let mut token = None;

    for pair in pairs {
      let (key, value) = pair.map_err(|_| unread())?;

      if key == TOKEN_KEY {
        token = Some(value);
      }
    }

    token
      .filter(|token| !token.is_empty())
      .ok_or_else(|| format!("the token file {} holds no {TOKEN_KEY}", self.token_file))
  }

  /// What a hub's answer status says where it is not the stream asked for.
  fn refused_status(&self, status: reqwest::StatusCode) -> Option<String> {
    match status.as_u16() {
      200 => None,
      401 | 403 => Some(format!("the hub refused the token ({})", status.as_u16())),
      404 | 409 => Some(format!("the agent {} is not attached to its hub", self.agent)),
      other => Some(format!("the hub answered {other}")),
    }
  }
}

/// The JSON an event carries, where it carries any.
fn data(event: &sse_stream::Sse) -> Value {
  event
    .data
    .as_deref()
    .and_then(|data| serde_json::from_str(data).ok())
    .unwrap_or(Value::Null)
}

fn said(body: &Value, field: &str, otherwise: &str) -> String {
  body.get(field).and_then(Value::as_str).unwrap_or(otherwise).to_owned()
}

impl Road for JakeRoad {
  fn shell(&self) -> &dyn Shell {
    &*self.shell
  }

  fn run(&self, script: String, _input: Option<Upload>) -> Running {
    let (output, waiting) = mpsc::channel(WAITING_CHUNKS);
    let (ending, ended) = oneshot::channel();
    let road = Arc::new(Self {
      hub_url: self.hub_url.clone(),
      token_file: self.token_file.clone(),
      agent: self.agent.clone(),
      shell: self.shell.clone(),
    });

    tokio::spawn(async move {
      let _ = ending.send(road.exec(script, output).await);
    });

    Running {
      output: waiting,
      ended,
    }
  }

  fn hand(&self, upload: Upload) -> Pending<'_, (Source, Option<Upload>)> {
    Box::pin(async move {
      let path = format!("/tmp/bmo-files-{:016x}", rand::random::<u64>());

      self.upload(&path, upload).await.map_err(upload_failed)?;

      Ok((Source::Scratch(path), None))
    })
  }
}

impl JakeRoad {
  async fn exec(&self, script: String, output: mpsc::Sender<Bytes>) -> Ended {
    let token = match self.token() {
      Ok(token) => token,
      Err(why) => return Ended::Failed(why),
    };
    let asked = HTTP
      .post(format!("{}/exec?agent={}", self.hub_url, encoded(&self.agent)))
      .bearer_auth(&token)
      .json(&json!({ "shell": "linux", "command": script, "env": {} }))
      .send()
      .await;
    let answer = match asked {
      Ok(answer) => answer,
      Err(why) => return Ended::Failed(format!("the hub did not answer: {}", last_line(&why.to_string()))),
    };

    if let Some(why) = self.refused_status(answer.status()) {
      return Ended::Failed(why);
    }

    let mut events = SseStream::from_bytes_stream(answer.bytes_stream());
    let mut id: Option<String> = None;
    let mut stderr = String::new();

    while let Some(event) = events.next().await {
      let Ok(event) = event else {
        return Ended::Failed("the hub's stream broke off".into());
      };
      let body = data(&event);

      match event.event.as_deref() {
        Some("started") => id = body.get("id").and_then(Value::as_str).map(str::to_owned),
        Some("stdout" | "stderr") => {
          let Some(bytes) = body
            .pointer("/chunk/base64")
            .and_then(Value::as_str)
            .and_then(|chunk| STANDARD.decode(chunk).ok())
          else {
            continue;
          };

          if event.event.as_deref() == Some("stderr") {
            stderr.push_str(&String::from_utf8_lossy(&bytes));
          } else if output.send(Bytes::from(bytes)).await.is_err() {
            if let Some(id) = &id {
              self.cancel(&token, id).await;
            }

            return Ended::Failed("nobody listened".into());
          }
        }
        Some("exit") => {
          let code = body.get("code").and_then(Value::as_i64).and_then(|code| i32::try_from(code).ok());

          return Ended::Code(code.unwrap_or(-1), last_line(&stderr));
        }
        Some("error") => return Ended::Failed(said(&body, "message", "the hub reported an error")),
        Some("ended") => {
          return Ended::Failed(format!("the run ended: {}", said(&body, "reason", "no reason given")));
        }
        _ => {}
      }
    }

    Ended::Failed("the hub's stream closed without the command's end".into())
  }

  async fn cancel(&self, token: &str, id: &str) {
    let _ = HTTP
      .post(format!("{}/cancel", self.hub_url))
      .bearer_auth(token)
      .json(&json!({ "id": id }))
      .send()
      .await;
  }

  /// A whole file written on the node at `path` through the hub, which keeps it only once all of it
  /// has arrived.
  async fn upload(&self, path: &str, upload: Upload) -> Result<(), String> {
    let token = self.token()?;
    let answer = HTTP
      .post(format!("{}/upload?path={}&agent={}", self.hub_url, encoded(path), encoded(&self.agent)))
      .bearer_auth(&token)
      .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
      .header(reqwest::header::CONTENT_LENGTH, upload.length)
      .body(reqwest::Body::wrap_stream(upload.body))
      .send()
      .await
      .map_err(|why| format!("the hub did not answer: {}", last_line(&why.to_string())))?;

    if let Some(why) = self.refused_status(answer.status()) {
      return Err(why);
    }

    let mut events = SseStream::from_bytes_stream(answer.bytes_stream());

    while let Some(event) = events.next().await {
      let event = event.map_err(|_| "the hub's stream broke off".to_owned())?;
      let body = data(&event);

      match event.event.as_deref() {
        Some("completed") => return Ok(()),
        Some("error") => return Err(said(&body, "message", "the hub reported an error")),
        Some("ended") => {
          return Err(format!("the transfer ended: {}", said(&body, "reason", "no reason given")));
        }
        _ => {}
      }
    }

    Err("the hub's stream closed without the transfer's end".into())
  }
}
