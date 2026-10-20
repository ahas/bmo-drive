//! The harness tools (`docs/api.md`, 「The harness plugin」): one for each route a harness walks and changes
//! files through, each a thin call of one method of the service's own client. The MCP server lists them and
//! the OpenAPI document states them under `x-tools`, both from this one table.

use std::future::Future;
use std::pin::Pin;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use schemars::generate::SchemaGenerator;
use schemars::{JsonSchema, Schema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::client::{ByteRange, ClientFailure, DriveClient};
use crate::wire::{
  Conflict, ContentQuery, CopyRequest, EntriesQuery, EntryMove, FolderRequest, PlacesQuery, ShareRequest,
  WriteQuery,
};

/// The most bytes one reading carries in a tool answer.
pub const READ_MOST: u64 = 4 * 1024 * 1024;

/// What a tool's call came to short of the route's answer.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolFailure {
  /// The arguments are not the shape the tool takes.
  Arguments(String),
  /// The call reached no answer of the route's shape.
  Client(ClientFailure),
}

impl ToolFailure {
  /// The failure as the tool error's content: the error shape the service answered with and its status, or
  /// the sentence saying why nothing answered.
  pub fn content(&self) -> Value {
    match self {
      Self::Arguments(why) => json!({ "code": "bad-request", "error": why }),
      Self::Client(ClientFailure::Answered {
        status,
        answer: Some(answer),
        ..
      }) => json!({ "status": status, "code": answer.code, "error": answer.error, "names": answer.names }),
      Self::Client(failure @ ClientFailure::Answered { status, .. }) => {
        json!({ "status": status, "error": failure.to_string() })
      }
      Self::Client(failure) => json!({ "error": failure.to_string() }),
    }
  }
}

/// A tool's answer: the route's answer as JSON, or why there is none.
pub type Answered = Pin<Box<dyn Future<Output = Result<Value, ToolFailure>> + Send>>;

type Call = Box<dyn Fn(DriveClient, Value) -> Answered + Send + Sync>;

/// One tool: its name, what it does, the route it calls, its input and output, and the call itself.
pub struct DriveTool {
  pub name: &'static str,
  pub description: &'static str,
  /// The route the tool calls, as its method and path.
  pub route: &'static str,
  input: fn(&mut SchemaGenerator) -> Schema,
  output: fn(&mut SchemaGenerator) -> Schema,
  call: Call,
}

impl DriveTool {
  /// The tool's input, its nested types referred to where `generator` keeps its definitions.
  pub fn input(&self, generator: &mut SchemaGenerator) -> Schema {
    (self.input)(generator)
  }

  /// The tool's output, referred to where `generator` keeps its definitions.
  pub fn output(&self, generator: &mut SchemaGenerator) -> Schema {
    (self.output)(generator)
  }

  /// Calls the tool's route on the service `client` reaches, with the tool's arguments.
  pub fn call(&self, client: DriveClient, arguments: Value) -> Answered {
    (self.call)(client, arguments)
  }
}

/// A tool whose input is `I` and whose answer is `O`, calling `call` with the arguments read as `I`.
fn tool<I, O, F, Fut>(
  name: &'static str,
  description: &'static str,
  route: &'static str,
  call: F,
) -> DriveTool
where
  I: DeserializeOwned + JsonSchema,
  O: Serialize + JsonSchema,
  F: Fn(DriveClient, I) -> Fut + Send + Sync + 'static,
  Fut: Future<Output = Result<O, ToolFailure>> + Send + 'static,
{
  DriveTool {
    name,
    description,
    route,
    input: I::json_schema,
    output: SchemaGenerator::subschema_for::<O>,
    call: Box::new(move |client, arguments| {
      let input = match serde_json::from_value::<I>(arguments) {
        Ok(input) => input,
        Err(error) => {
          let why = format!("The arguments are not the shape this tool takes: {error}.");
          return Box::pin(std::future::ready(Err(ToolFailure::Arguments(why))));
        }
      };
      let answer = call(client, input);

      Box::pin(async move {
        let answer = answer.await?;

        serde_json::to_value(answer).map_err(|why| ToolFailure::Arguments(why.to_string()))
      })
    }),
  }
}

/// A client failure as a tool's.
fn failed(failure: ClientFailure) -> ToolFailure {
  ToolFailure::Client(failure)
}

/// A folder's entries, its place named beside the listing's query.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct EntriesInput {
  place_name: String,
  #[serde(flatten)]
  query: EntriesQuery,
}

/// One entry of a place.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct EntryInput {
  place_name: String,
  /// Its path from the place's top, names joined by `/`.
  entry_path: String,
}

/// An entry and where it is to stand.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct MoveInput {
  place_name: String,
  entry_path: String,
  #[serde(flatten)]
  moved: EntryMove,
}

/// A part of a file to read.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ReadInput {
  place_name: String,
  entry_path: String,
  /// Where the part starts, 0 where none is given.
  #[serde(default)]
  start: Option<u64>,
  /// How many bytes to read, 4 MiB at most and where none is given.
  #[serde(default)]
  byte_count: Option<u64>,
}

/// A part of a file, read.
#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct ReadOutput {
  /// The bytes, in base64.
  content: String,
  /// Where they start in the file.
  start: u64,
  /// How many bytes it carries.
  byte_count: u64,
  /// The whole file's length.
  file_byte_count: Option<u64>,
}

/// A file to write, whole.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct WriteInput {
  place_name: String,
  entry_path: String,
  /// The file's bytes, in base64.
  content: String,
  /// How a name already taken is settled; replaced where none is asked.
  #[serde(default)]
  conflict: Option<Conflict>,
}

/// A folder to make in a place.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct FolderInput {
  place_name: String,
  #[serde(flatten)]
  folder: FolderRequest,
}

/// Entries of a place to copy.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct CopyInput {
  place_name: String,
  #[serde(flatten)]
  copy: CopyRequest,
}

/// Nothing: what a deletion answers.
#[derive(Serialize, JsonSchema)]
struct Done {}

/// The nine tools, in the order the routes they call are listed.
pub fn tools() -> Vec<DriveTool> {
  vec![
    tool(
      "drive_places",
      "The places files live in: this machine's volumes and its object store, or with nodeName a node's \
       volumes. Start here: every other tool names a place by its name.",
      "GET /places",
      |client, query: PlacesQuery| async move { client.places(&query).await.map_err(failed) },
    ),
    tool(
      "drive_entries",
      "The entries under a path of a place, a page at a time: after is how many were already read.",
      "GET /places/{placeName}/entries",
      |client, EntriesInput { place_name, query }| async move {
        client.entries(&place_name, &query).await.map_err(failed)
      },
    ),
    tool(
      "drive_move",
      "Rename or move an entry to another path of its own place.",
      "PATCH /places/{placeName}/entries/{entryPath}",
      |client,
       MoveInput {
         place_name,
         entry_path,
         moved,
       }| async move { client.move_entry(&place_name, &entry_path, &moved).await.map_err(failed) },
    ),
    tool(
      "drive_delete",
      "Delete an entry, a folder with everything in it.",
      "DELETE /places/{placeName}/entries/{entryPath}",
      |client,
       EntryInput {
         place_name,
         entry_path,
       }| async move {
        client.delete_entry(&place_name, &entry_path).await.map_err(failed)?;

        Ok(Done {})
      },
    ),
    tool(
      "drive_read",
      "A file's bytes in base64, read by range: at most 4 MiB from start in one answer, so a longer file is \
       read a part at a time.",
      "GET /places/{placeName}/entries/{entryPath}/content",
      |client,
       ReadInput {
         place_name,
         entry_path,
         start,
         byte_count,
       }| async move {
        let range = ByteRange {
          start: start.unwrap_or(0),
          most: Some(byte_count.unwrap_or(READ_MOST).clamp(1, READ_MOST)),
        };
        let content = client
          .content(&place_name, &entry_path, &ContentQuery::default(), Some(range))
          .await
          .map_err(failed)?;
        let (start, file_byte_count) = (content.start, content.file_byte_count);
        let bytes = content.bytes().await.map_err(failed)?;

        Ok(ReadOutput {
          content: STANDARD.encode(&bytes),
          start,
          byte_count: bytes.len() as u64,
          file_byte_count,
        })
      },
    ),
    tool(
      "drive_write",
      "Write a file's bytes, given in base64, replacing what stands there unless conflict says otherwise.",
      "PUT /places/{placeName}/entries/{entryPath}/content",
      |client,
       WriteInput {
         place_name,
         entry_path,
         content,
         conflict,
       }| async move {
        let bytes = STANDARD
          .decode(content.as_bytes())
          .map_err(|why| ToolFailure::Arguments(format!("The content is not base64: {why}.")))?;

        client
          .write_content(&place_name, &entry_path, &WriteQuery { conflict }, bytes)
          .await
          .map_err(failed)
      },
    ),
    tool(
      "drive_folder",
      "Make a folder.",
      "POST /places/{placeName}/folders",
      |client, FolderInput { place_name, folder }| async move {
        client.create_folder(&place_name, &folder).await.map_err(failed)
      },
    ),
    tool(
      "drive_copy",
      "Copy entries to a folder of this place, or of the place toPlaceName names.",
      "POST /places/{placeName}/copies",
      |client, CopyInput { place_name, copy }| async move {
        client.copy(&place_name, &copy).await.map_err(failed)
      },
    ),
    tool(
      "drive_share",
      "Share an entry by link, until a time or never: the link's url is what to hand someone.",
      "POST /shares",
      |client, share: ShareRequest| async move { client.create_share(&share).await.map_err(failed) },
    ),
  ]
}
