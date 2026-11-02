//! The node service (`docs/api.md`, 「The node service」): the node list, a node's connection and its batch
//! SSH login, as the node service's own boundary document states them. Bodies are camelCase JSON both ways;
//! the service's refusal is an answer of 200 with `answered` false and a `refusal` carrying its `code` and
//! `message`, and a request it could not answer at all is non-2xx with `code` and `error`.

use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The node service at the address the settings name, with its token.
#[derive(Clone, Debug)]
pub struct Node {
  http: reqwest::Client,
  address: String,
  token: String,
}

/// One node of the list, as the node view carries it; only what this service reads.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeView {
  pub name: String,
  pub label: String,
  /// `ssh` or `jake`.
  pub kind: String,
  /// Whether it is an ssh node whose host names this machine.
  #[serde(default)]
  pub this_machine: bool,
}

/// A node's connection, by its kind.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Target {
  #[serde(rename_all = "camelCase")]
  Ssh {
    host: String,
    port: u16,
    user: String,
    #[serde(default)]
    key_path: Option<String>,
    /// `posix` or `powershell`; none is `posix`.
    #[serde(default)]
    shell: Option<String>,
  },
  #[serde(rename_all = "camelCase")]
  Jake {
    hub_url: String,
    token_file: String,
    agent: String,
  },
}

/// Why a call to the node service gave no answer.
#[derive(Debug, thiserror::Error)]
pub enum NodeError {
  /// It did not answer, or not as its document says.
  #[error("the node service did not answer: {0}")]
  Unreachable(String),
  /// It refused, under its own code.
  #[error("the node service refused ({code}): {message}")]
  Refused { code: String, message: String },
}

impl NodeError {
  /// The node service's own code, where it refused.
  pub fn code(&self) -> Option<&str> {
    match self {
      Self::Refused { code, .. } => Some(code),
      Self::Unreachable(_) => None,
    }
  }
}

#[derive(Deserialize)]
struct Nodes {
  nodes: Vec<NodeView>,
}

#[derive(Deserialize)]
struct Login {
  ssh: Option<Vec<String>>,
}

#[derive(Serialize)]
struct LoginAsked<'a> {
  node: &'a str,
}

#[derive(Deserialize)]
struct Refusal {
  code: String,
  message: String,
}

impl Node {
  pub fn new(address: &str, token: &str) -> Self {
    Self {
      http: reqwest::Client::new(),
      address: address.trim_end_matches('/').to_owned(),
      token: token.to_owned(),
    }
  }

  /// The node list, in its order: `GET /nodes/view`.
  pub async fn nodes(&self) -> Result<Vec<NodeView>, NodeError> {
    Ok(self.call::<Nodes>(Method::GET, "/nodes/view", None).await?.nodes)
  }

  /// One node's connection: `GET /nodes/{name}/target`.
  pub async fn target(&self, name: &str) -> Result<Target, NodeError> {
    let path = format!("/nodes/{}/target", encoded(name));

    self.call(Method::GET, &path, None).await
  }

  /// The arguments that open a batch SSH login to a node, ahead of the command, or none where the node has
  /// no SSH route: `POST /nodes/login`.
  pub async fn login(&self, name: &str) -> Result<Option<Vec<String>>, NodeError> {
    let body = serde_json::to_value(LoginAsked { node: name }).unwrap_or(Value::Null);

    Ok(self.call::<Login>(Method::POST, "/nodes/login", Some(body)).await?.ssh)
  }

  async fn call<T: DeserializeOwned>(
    &self,
    method: Method,
    path: &str,
    body: Option<Value>,
  ) -> Result<T, NodeError> {
    let unreachable = |why: String| NodeError::Unreachable(why);
    let mut request = self.http.request(method, format!("{}{path}", self.address)).bearer_auth(&self.token);

    if let Some(body) = body {
      request = request.json(&body);
    }

    let answer = request.send().await.map_err(|why| unreachable(why.to_string()))?;
    let status = answer.status();
    let read: Value = answer.json().await.map_err(|why| unreachable(why.to_string()))?;

    if !status.is_success() {
      let code = read.get("code").and_then(Value::as_str).unwrap_or("failed");
      let error = read.get("error").and_then(Value::as_str).unwrap_or_default();

      return Err(unreachable(format!("{} {code}: {error}", status.as_u16())));
    }

    if read.get("answered").and_then(Value::as_bool) == Some(false) {
      let refusal: Refusal = serde_json::from_value(read.get("refusal").cloned().unwrap_or(Value::Null))
        .map_err(|why| unreachable(why.to_string()))?;

      return Err(NodeError::Refused {
        code: refusal.code,
        message: refusal.message,
      });
    }

    serde_json::from_value(read).map_err(|why| unreachable(why.to_string()))
  }
}

/// A node's name as one segment of a path.
fn encoded(name: &str) -> String {
  reqwest::Url::parse("http://node/")
    .ok()
    .and_then(|mut url| {
      url.path_segments_mut().ok()?.push(name);
      Some(url.path().trim_start_matches('/').to_owned())
    })
    .unwrap_or_else(|| name.to_owned())
}
