//! The nodes of the node list as machines the viewer walks (`docs/files.md`, 「A node's drives」): the list
//! and each node's connection read from the node service, each node read over its own road when it is
//! asked for, its drives by the rules for that kind of machine, and each of its volumes a place whose
//! every reading and change is one script on the node.

mod posix;
mod powershell;
mod road;
mod shell;

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::http::StatusCode;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::drives::Drive;
use super::{Entry, Opened, Pending, Place, Places, Refused, Span, Upload, Within, no_place, outside};
use crate::clients::node::{NodeError, Target};
use crate::http::Standing;
use posix::Posix;
use powershell::PowerShell;
use road::{Ended, JakeRoad, Road, Running, SshRoad};
use shell::{Op, Rooted, Shell, read_listing, refusal_of};

// How long one reading of a node serves the requests after it: each reading is a login over the
// network, and a node's mounts change seldom.
const NODE_FOR: Duration = Duration::from_secs(60);

// How long a reading — the drives, a folder, a file's first line — may take on a node.
const READING: Duration = Duration::from_secs(20);

/// The prefix of a node's machine key and of every place on it.
const NODE: &str = "node:";

const NODE_INCOMPLETE: &str = "node_incomplete";
const NODE_FAILED: &str = "node_failed";

/// Why a node could not be read: a code and an English sentence (`docs/api.md`, 「Refusals」).
#[derive(Clone, Debug)]
struct Fault {
  code: &'static str,
  message: String,
}

impl From<Refused> for Fault {
  fn from(refused: Refused) -> Self {
    Self {
      code: refused.code,
      message: refused.error,
    }
  }
}

fn nodes_unread(why: &NodeError) -> Refused {
  Refused::new(
    StatusCode::BAD_GATEWAY,
    "nodes_unread",
    format!("the node list could not be read: {why}"),
  )
}

/// A node as the list names it.
pub struct Listed {
  pub name: String,
  pub label: String,
}

impl Listed {
  /// The machine as the console is told of it before it is read.
  pub fn unread(&self) -> Value {
    json!({
      "key": machine_key(&self.name),
      "name": self.label,
      "local": false,
      "disks": null,
      "garage": null,
      "error": null,
      "errorCode": null,
    })
  }
}

fn machine_key(name: &str) -> String {
  format!("{NODE}{name}")
}

/// The node a place or a machine key names, where it names one.
pub fn node_of(id: &str) -> Option<&str> {
  id.strip_prefix(NODE).map(|rest| rest.split(':').next().unwrap_or(rest))
}

/// The node list, in its order, read from the node service. A node it marks as this machine is not listed
/// again: this machine stands for it.
pub async fn listed(standing: &Standing) -> Result<Vec<Listed>, Refused> {
  let nodes = standing.node.nodes().await.map_err(|why| nodes_unread(&why))?;

  Ok(
    nodes
      .into_iter()
      .filter(|node| !node.this_machine)
      .map(|node| Listed {
        label: Some(node.label).filter(|label| !label.is_empty()).unwrap_or_else(|| node.name.clone()),
        name: node.name,
      })
      .collect(),
  )
}

/// A volume of a node, walked over the node's road.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeVolume {
  place: String,
  mount: String,
  fs_type: String,
  #[serde(skip)]
  root: String,
  #[serde(skip)]
  node: Arc<str>,
  #[serde(skip)]
  road: Arc<dyn Road>,
}

/// A node read: its drives, or why they could not be read.
pub struct NodeMachine {
  key: String,
  name: String,
  drives: Result<Vec<Drive<NodeVolume>>, Fault>,
}

impl NodeMachine {
  pub fn view(&self) -> Value {
    let (disks, error, code) = match &self.drives {
      Ok(drives) => (json!(drives), Value::Null, Value::Null),
      Err(fault) => (Value::Null, json!(fault.message), json!(fault.code)),
    };

    json!({
      "key": self.key,
      "name": self.name,
      "local": false,
      "disks": disks,
      "garage": null,
      "error": error,
      "errorCode": code,
    })
  }
}

impl Places for NodeMachine {
  fn place(&self, id: &str) -> Result<&dyn Place, Refused> {
    self
      .drives
      .as_ref()
      .map_err(|fault| Refused::new(StatusCode::BAD_GATEWAY, fault.code, fault.message.clone()))?
      .iter()
      .flat_map(|drive| drive.volumes.iter())
      .find(|volume| volume.place == id)
      .map(|volume| volume as &dyn Place)
      .ok_or_else(no_place)
  }
}

/// A node's reading and when it was made.
type NodeReading = (Instant, Arc<NodeMachine>);

static READ: LazyLock<Mutex<HashMap<String, NodeReading>>> = LazyLock::new(Mutex::default);

/// A node as read a moment ago, or read now where that is older or `fresh` is asked.
pub async fn machine(standing: &Standing, name: &str, fresh: bool) -> Result<Arc<NodeMachine>, Refused> {
  if !fresh {
    let kept = READ.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    if let Some((at, machine)) = kept.get(name)
      && at.elapsed() < NODE_FOR
    {
      return Ok(machine.clone());
    }
  }

  let node = listed(standing)
    .await?
    .into_iter()
    .find(|one| one.name == name)
    .ok_or_else(|| Refused::new(StatusCode::NOT_FOUND, "no_node", format!("no node {name} is listed")))?;
  let drives = match road(standing, &node.name).await? {
    Ok(road) => read_drives(&node.name, road).await,
    Err(fault) => Err(fault),
  };
  let read = Arc::new(NodeMachine {
    key: machine_key(&node.name),
    name: node.label,
    drives,
  });

  READ
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .insert(name.to_owned(), (Instant::now(), read.clone()));

  Ok(read)
}

/// The node's own road, out of the connection and the login the node service gives for it: a fault where
/// it gives none that serves, a refusal where it did not answer.
async fn road(standing: &Standing, name: &str) -> Result<Result<Arc<dyn Road>, Fault>, Refused> {
  let incomplete = |why: String| Fault {
    code: NODE_INCOMPLETE,
    message: format!("the node service holds no usable connection for {name}: {why}"),
  };
  let protect = &standing.settings.protect;
  let posix = || -> Arc<dyn Shell> {
    Arc::new(Posix {
      policy: protect.posix.clone(),
    })
  };
  let target = match standing.node.target(name).await {
    Ok(target) => target,
    Err(why @ NodeError::Refused { .. }) => return Ok(Err(incomplete(why.to_string()))),
    Err(why) => return Err(nodes_unread(&why)),
  };

  Ok(Ok(match target {
    Target::Ssh { shell, .. } => {
      let login = match standing.node.login(name).await {
        Ok(Some(login)) => login,
        Ok(None) => return Ok(Err(incomplete("it has no SSH login".to_owned()))),
        Err(why @ NodeError::Refused { .. }) => return Ok(Err(incomplete(why.to_string()))),
        Err(why) => return Err(nodes_unread(&why)),
      };
      let shell: Arc<dyn Shell> = if shell.as_deref() == Some("powershell") {
        Arc::new(PowerShell {
          policy: protect.windows.clone(),
        })
      } else {
        posix()
      };

      Arc::new(SshRoad { login, shell })
    }
    Target::Jake {
      hub_url,
      token_file,
      agent,
    } => Arc::new(JakeRoad {
      hub_url,
      token_file,
      agent,
      shell: posix(),
    }),
  }))
}

async fn read_drives(name: &str, road: Arc<dyn Road>) -> Result<Vec<Drive<NodeVolume>>, Fault> {
  let shell = road.shell();
  let printed = whole(name, road.run(shell.drives_script(), None), Some(READING)).await?;
  let node: Arc<str> = Arc::from(name);
  let drives = shell.read_drives(&printed).map_err(|why| Fault {
    code: NODE_FAILED,
    message: format!("{name}'s drives could not be read: {why}"),
  })?;

  Ok(
    drives
      .into_iter()
      .map(|drive| Drive {
        key: drive.key,
        name: drive.name,
        kind: drive.kind,
        size_bytes: drive.size_bytes,
        volumes: drive
          .volumes
          .into_iter()
          .map(
            |Rooted {
               mount,
               fs_type,
               root,
             }| NodeVolume {
              place: format!("{NODE}{name}:{mount}"),
              mount,
              fs_type,
              root,
              node: node.clone(),
              road: road.clone(),
            },
          )
          .collect(),
      })
      .collect(),
  )
}

fn timed_out(node: &str) -> Refused {
  Refused::new(StatusCode::GATEWAY_TIMEOUT, "node_timeout", format!("{node} did not answer in time"))
}

fn no_length(node: &str) -> Refused {
  Refused::new(
    StatusCode::BAD_GATEWAY,
    "node_no_length",
    format!("{node} did not say the file's length"),
  )
}

/// What a script's ending says: nothing where it ended well, a refusal otherwise.
fn settled(node: &str, ended: Ended) -> Result<(), Refused> {
  match ended {
    Ended::Code(0, _) => Ok(()),
    Ended::Code(code, said) => Err(refusal_of(code).unwrap_or_else(|| {
      Refused::new(
        StatusCode::BAD_GATEWAY,
        NODE_FAILED,
        format!("{node} failed: {}", if said.is_empty() { format!("exit code {code}") } else { said }),
      )
    })),
    Ended::Failed(why) => Err(Refused::new(
      StatusCode::BAD_GATEWAY,
      "node_unreachable",
      format!("{node} could not be reached: {why}"),
    )),
  }
}

async fn ending(node: &str, ended: tokio::sync::oneshot::Receiver<Ended>) -> Result<(), Refused> {
  settled(node, ended.await.unwrap_or_else(|_| Ended::Failed("the run was lost".into())))
}

/// Everything a script printed once it has ended well, given up past `limit` where one is set.
async fn whole(node: &str, mut running: Running, limit: Option<Duration>) -> Result<Vec<u8>, Refused> {
  let gather = async {
    let mut printed = Vec::new();

    while let Some(chunk) = running.output.recv().await {
      printed.extend_from_slice(&chunk);
    }

    printed
  };
  let printed = match limit {
    Some(limit) => tokio::time::timeout(limit, gather).await.map_err(|_| timed_out(node))?,
    None => gather.await,
  };

  ending(node, running.ended).await?;

  Ok(printed)
}

// The most a file's first line — its length — may take.
const LENGTH_LINE_MOST: usize = 32;

impl NodeVolume {
  /// The names of a path, refused where this node's shell takes one of them in no path.
  fn names<'a>(&self, within: &'a Within) -> Result<&'a [String], Refused> {
    if within.names.iter().all(|name| self.road.shell().allows(name)) {
      Ok(&within.names)
    } else {
      Err(outside())
    }
  }

  fn run(&self, op: &Op, input: Option<Upload>) -> Running {
    self.road.run(self.road.shell().script(&self.root, op), input)
  }

  async fn change(&self, op: Op<'_>) -> Result<(), Refused> {
    whole(&self.node, self.run(&op, None), None).await.map(|_| ())
  }

  async fn opened(&self, within: &Within, span: Span) -> Result<Opened, Refused> {
    let names = self.names(within)?;
    let mut running = self.run(&Op::Read(names, span), None);
    let mut first: Vec<u8> = Vec::new();
    let line = tokio::time::timeout(READING, async {
      while let Some(chunk) = running.output.recv().await {
        first.extend_from_slice(&chunk);

        if let Some(cut) = first.iter().position(|byte| *byte == b'\n') {
          return Some(cut);
        }

        if first.len() > LENGTH_LINE_MOST {
          return None;
        }
      }

      None
    })
    .await
    .map_err(|_| timed_out(&self.node))?;
    let Some(cut) = line else {
      ending(&self.node, running.ended).await?;

      return Err(no_length(&self.node));
    };
    let size: u64 = String::from_utf8_lossy(&first[..cut])
      .trim()
      .parse()
      .map_err(|_| no_length(&self.node))?;
    let (start, length) = span.within(size);
    let rest = Bytes::copy_from_slice(&first[cut + 1..]);
    let mut output = running.output;
    let (passed, body) = mpsc::channel::<Result<Bytes, std::io::Error>>(1);

    tokio::spawn(async move {
      if !rest.is_empty() && passed.send(Ok(rest)).await.is_err() {
        return;
      }

      while let Some(chunk) = output.recv().await {
        if passed.send(Ok(chunk)).await.is_err() {
          return;
        }
      }
    });

    Ok(Opened {
      name: within.last().unwrap_or_default().to_owned(),
      size,
      start,
      length,
      body: Body::from_stream(futures_util::stream::unfold(body, |mut body| async move {
        body.recv().await.map(|chunk| (chunk, body))
      })),
    })
  }
}

impl Place for NodeVolume {
  fn id(&self) -> &str {
    &self.place
  }

  fn read_folder<'a>(&'a self, within: &'a Within) -> Pending<'a, Vec<Entry>> {
    Box::pin(async move {
      let names = self.names(within)?;

      Ok(read_listing(&whole(&self.node, self.run(&Op::List(names), None), Some(READING)).await?))
    })
  }

  fn open<'a>(&'a self, within: &'a Within, span: Span) -> Pending<'a, Opened> {
    Box::pin(self.opened(within, span))
  }

  fn make_folder<'a>(&'a self, within: &'a Within) -> Pending<'a, ()> {
    Box::pin(async move { self.change(Op::MakeFolder(self.names(within)?)).await })
  }

  fn write<'a>(&'a self, within: &'a Within, upload: Upload) -> Pending<'a, ()> {
    Box::pin(async move {
      let names = self.names(within)?;
      let length = upload.length;
      let (source, input) = self.road.hand(upload).await?;

      whole(&self.node, self.run(&Op::Write(names, &source, length), input), None)
        .await
        .map(|_| ())
    })
  }

  fn rename<'a>(&'a self, within: &'a Within, name: &'a str) -> Pending<'a, ()> {
    Box::pin(async move {
      if !self.road.shell().allows(name) {
        return Err(outside());
      }

      self.change(Op::Rename(self.names(within)?, name)).await
    })
  }

  fn remove<'a>(&'a self, within: &'a Within) -> Pending<'a, ()> {
    Box::pin(async move { self.change(Op::Remove(self.names(within)?)).await })
  }

  fn copy_here<'a>(&'a self, from: &'a Within, to: &'a Within, moving: bool) -> Pending<'a, ()> {
    Box::pin(async move {
      self
        .change(Op::Copy {
          from: self.names(from)?,
          to: self.names(to)?,
          moving,
        })
        .await
    })
  }

  // The script fences every path to the root it is handed, so the folder handed as the root is the
  // fence.
  fn beneath<'a>(&'a self, top: &'a Within) -> Pending<'a, Box<dyn Place + 'a>> {
    Box::pin(async move {
      let names = self.names(top)?;

      Ok(Box::new(Self {
        root: self.road.shell().beneath(&self.root, names),
        ..self.clone()
      }) as Box<dyn Place + 'a>)
    })
  }
}
