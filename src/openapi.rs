//! The service's OpenAPI document (`docs/api.md`): every route with its parameters, its body, its answers
//! and the codes it can refuse with, its body schemas generated from the types of `wire`, and the harness
//! tools from the table the MCP server lists. `bmo-drive openapi` prints it, and the repository keeps it as
//! `docs/openapi.json`.

use schemars::JsonSchema;
use schemars::generate::{SchemaGenerator, SchemaSettings};
use serde_json::{Map, Value, json};

use crate::console::{MANIFEST_PATH, WORDS_PATH};
use crate::events::KINDS;
use crate::mcp::tools;
use crate::wire::{
  ArchiveRequest, ConsoleManifest, ContentQuery, CopyRequest, EntriesQuery, Entry, EntryMove, EntryPage,
  EventPage, EventsQuery, FolderRequest, Health, Machine, MachineQuery, NodePage, NodesQuery, Owner,
  PlacePage, PlacesQuery, Quota, QuotaRequest, Share, SharePage, ShareRequest, SharesQuery, Storage,
  WireError, WriteQuery,
};

/// Where the schemas the routes refer to stand in the document.
const SCHEMAS: &str = "/components/schemas";

const DESCRIPTION: &str = "Files across places: a machine's drives, an S3-compatible store and the volumes \
  of other machines, the administrator's alone, and each owner's own drive with its quota, read, written, \
  moved and shared by link. The paths carry no prefix; whatever serves the service to a browser mounts it \
  under its own.";

/// What a route takes as its body.
enum Body {
  Json(Value),
  Bytes,
}

/// One route of the document.
struct Operation {
  method: &'static str,
  path: &'static str,
  id: &'static str,
  /// Whether the route acts for the owner `Owner-Key` names, held by a reader in the role `Owner-Role`
  /// names; such a route can also refuse with `forbidden`.
  owned: bool,
  tag: &'static str,
  summary: &'static str,
  parameters: Vec<Value>,
  body: Option<Body>,
  /// The success answers, each by its status: the content by media type, or none for no body.
  answers: Vec<(u16, Option<Value>)>,
  /// Every code the route can answer.
  codes: &'static [&'static str],
}

impl Operation {
  fn value(self) -> Value {
    let mut responses = Map::new();

    for (status, content) in self.answers {
      let mut answer = json!({ "description": self.summary });

      if let Some(content) = content {
        answer["content"] = content;
      }

      responses.insert(status.to_string(), answer);
    }

    responses.insert("default".into(), json!({ "$ref": "#/components/responses/Error" }));

    let codes: Vec<&str> = self
      .codes
      .iter()
      .flat_map(|&code| match code {
        "unauthorized" if self.owned => vec![code, "forbidden"],
        _ => vec![code],
      })
      .collect();
    let mut operation = json!({
      "operationId": self.id,
      "tags": [self.tag],
      "summary": self.summary,
      "responses": responses,
      "x-codes": codes,
    });

    let owner = if self.owned { vec![owner_key(), owner_role()] } else { Vec::new() };
    let parameters: Vec<Value> = owner.into_iter().chain(self.parameters).collect();

    if !parameters.is_empty() {
      operation["parameters"] = parameters.into();
    }

    match self.body {
      Some(Body::Json(schema)) => {
        operation["requestBody"] =
          json!({ "required": true, "content": { "application/json": { "schema": schema } } });
      }
      Some(Body::Bytes) => {
        operation["requestBody"] = json!({
          "required": true,
          "content": { "application/octet-stream": { "schema": { "type": "string", "contentEncoding": "binary" } } },
        });
      }
      None => {}
    }

    operation
  }
}

/// A JSON answer of this type.
fn json_answer<Answer: JsonSchema>(generator: &mut SchemaGenerator) -> Option<Value> {
  Some(json!({ "application/json": { "schema": generator.subschema_for::<Answer>() } }))
}

/// An answer of bytes.
fn bytes_answer(media: &str) -> Option<Value> {
  Some(json!({ media: { "schema": { "type": "string", "contentEncoding": "binary" } } }))
}

/// A JSON body of this type.
fn json_body<Sent: JsonSchema>(generator: &mut SchemaGenerator) -> Option<Body> {
  Some(Body::Json(generator.subschema_for::<Sent>().to_value()))
}

/// One path parameter: one segment, a `/` or a `:` in it written percent-encoded.
fn segment(name: &str, description: &str) -> Value {
  json!({
    "name": name,
    "in": "path",
    "required": true,
    "description": description,
    "schema": { "type": "string" },
  })
}

fn place_name() -> Value {
  segment("placeName", "The place's name, as GET /places answers it.")
}

fn entry_path() -> Value {
  segment("entryPath", "The entry's path from the place's top, names joined by `/`.")
}

fn token() -> Value {
  segment("token", "The share link's token.")
}

/// The owner a route acts for (`docs/api.md`, 「Owners」).
fn owner_key() -> Value {
  json!({
    "name": "Owner-Key",
    "in": "header",
    "required": true,
    "description": "The opaque key of the owner the caller acts for, 1 to 200 characters.",
    "schema": { "type": "string", "minLength": 1, "maxLength": 200 },
  })
}

/// The role the reader holds, the floor of every route that takes the owner key (`docs/api.md`, 「Owners」).
fn owner_role() -> Value {
  json!({
    "name": "Owner-Role",
    "in": "header",
    "required": true,
    "description": "The role the reader holds; either allows the console bundle's role, member.",
    "schema": { "type": "string", "enum": ["administrator", "member"] },
  })
}

fn owner_key_segment() -> Value {
  segment("ownerKey", "The owner key whose quota is read or set.")
}

/// A header parameter.
fn header(name: &str, description: &str) -> Value {
  json!({ "name": name, "in": "header", "required": false, "description": description, "schema": { "type": "string" } })
}

/// The query parameters of a query type, each property one parameter.
fn query<Query: JsonSchema>(generator: &mut SchemaGenerator) -> Vec<Value> {
  let schema = Query::json_schema(generator).to_value();
  let Some(properties) = schema["properties"].as_object() else {
    return Vec::new();
  };

  properties
    .iter()
    .map(|(name, property)| {
      let mut parameter = json!({ "name": name, "in": "query", "required": false, "schema": property });

      if let Some(description) = property.get("description") {
        parameter["description"] = description.clone();
      }

      parameter
    })
    .collect()
}

/// Every route the service answers, in the order `docs/api.md` lists them.
fn operations(generator: &mut SchemaGenerator) -> Vec<Operation> {
  let range = header("Range", "One byte range of the file.");

  vec![
    Operation {
      method: "get",
      path: "/health",
      id: "readHealth",
      owned: false,
      tag: "health",
      summary: "Readiness and version.",
      parameters: Vec::new(),
      body: None,
      answers: vec![(200, json_answer::<Health>(generator))],
      codes: &["unauthorized"],
    },
    Operation {
      method: "get",
      path: "/owner",
      id: "readOwner",
      owned: true,
      tag: "owners",
      summary: "What the asking owner may reach: whether the machines are its, and its drive's usage and quota.",
      parameters: Vec::new(),
      body: None,
      answers: vec![(200, json_answer::<Owner>(generator))],
      codes: &["unauthorized", "bad-request", "failed"],
    },
    Operation {
      method: "get",
      path: "/storage",
      id: "readStorage",
      owned: true,
      tag: "owners",
      summary: "Where the drive stands and how it is backed up, as the settings state it.",
      parameters: Vec::new(),
      body: None,
      answers: vec![(200, json_answer::<Storage>(generator))],
      codes: &["unauthorized", "bad-request", "not-found"],
    },
    Operation {
      method: "get",
      path: "/quotas/{ownerKey}",
      id: "readQuota",
      owned: true,
      tag: "owners",
      summary: "One owner's quota and usage.",
      parameters: vec![owner_key_segment()],
      body: None,
      answers: vec![(200, json_answer::<Quota>(generator))],
      codes: &["unauthorized", "bad-request", "not-found", "failed"],
    },
    Operation {
      method: "put",
      path: "/quotas/{ownerKey}",
      id: "setQuota",
      owned: true,
      tag: "owners",
      summary: "Set one owner's quota, or return it to the default.",
      parameters: vec![owner_key_segment()],
      body: json_body::<QuotaRequest>(generator),
      answers: vec![(200, json_answer::<Quota>(generator))],
      codes: &["unauthorized", "bad-request", "not-found", "no-quota", "failed"],
    },
    Operation {
      method: "get",
      path: "/events",
      id: "listEvents",
      owned: true,
      tag: "events",
      summary: "The service's one event stream (SSE), or a page of it with Accept: application/json.",
      parameters: [
        query::<EventsQuery>(generator),
        vec![header("Last-Event-ID", "The event to resume after.")],
      ]
      .concat(),
      body: None,
      answers: vec![(
        200,
        Some(json!({
          "text/event-stream": { "schema": { "type": "string" } },
          "application/json": { "schema": generator.subschema_for::<EventPage>() },
        })),
      )],
      codes: &["unauthorized", "bad-request", "gone", "failed"],
    },
    Operation {
      method: "get",
      path: "/places",
      id: "listPlaces",
      owned: true,
      tag: "places",
      summary: "The places files live in: the owner's drive and this machine's, or a node's volumes.",
      parameters: query::<PlacesQuery>(generator),
      body: None,
      answers: vec![(200, json_answer::<PlacePage>(generator))],
      codes: &["unauthorized", "bad-request", "not-found", "upstream-failed", "failed"],
    },
    Operation {
      method: "get",
      path: "/machine",
      id: "readMachine",
      owned: true,
      tag: "machine",
      summary: "This machine and its drives, or a node and its drives.",
      parameters: query::<MachineQuery>(generator),
      body: None,
      answers: vec![(200, json_answer::<Machine>(generator))],
      codes: &["unauthorized", "bad-request", "not-found", "upstream-failed", "failed"],
    },
    Operation {
      method: "get",
      path: "/nodes",
      id: "listNodes",
      owned: true,
      tag: "machine",
      summary: "The node list as the node service answers it now, this machine left out.",
      parameters: query::<NodesQuery>(generator),
      body: None,
      answers: vec![(200, json_answer::<NodePage>(generator))],
      codes: &["unauthorized", "bad-request", "not-found", "upstream-failed"],
    },
    Operation {
      method: "get",
      path: "/places/{placeName}/entries",
      id: "listEntries",
      owned: true,
      tag: "entries",
      summary: "The entries under a path of a place.",
      parameters: [vec![place_name()], query::<EntriesQuery>(generator)].concat(),
      body: None,
      answers: vec![(200, json_answer::<EntryPage>(generator))],
      codes: &[
        "unauthorized",
        "bad-request",
        "not-found",
        "outside-place",
        "not-folder",
        "no-permission",
        "failed-io",
        "garage-closed",
        "storage-unready",
        "upstream-failed",
        "failed",
      ],
    },
    Operation {
      method: "get",
      path: "/places/{placeName}/entries/{entryPath}",
      id: "readEntry",
      owned: true,
      tag: "entries",
      summary: "One entry.",
      parameters: vec![place_name(), entry_path()],
      body: None,
      answers: vec![(200, json_answer::<Entry>(generator))],
      codes: &[
        "unauthorized",
        "bad-request",
        "not-found",
        "outside-place",
        "no-permission",
        "garage-closed",
        "storage-unready",
        "upstream-failed",
        "failed",
      ],
    },
    Operation {
      method: "patch",
      path: "/places/{placeName}/entries/{entryPath}",
      id: "moveEntry",
      owned: true,
      tag: "entries",
      summary: "Rename or move an entry.",
      parameters: vec![place_name(), entry_path()],
      body: json_body::<EntryMove>(generator),
      answers: vec![(200, json_answer::<Entry>(generator))],
      codes: &[
        "unauthorized",
        "bad-request",
        "not-found",
        "conflict",
        "outside-place",
        "place-top",
        "into-itself",
        "bad-name",
        "protected",
        "no-permission",
        "failed-io",
        "bucket-rename",
        "bucket-move",
        "no-bucket",
        "garage-closed",
        "storage-unready",
        "upstream-failed",
        "failed",
      ],
    },
    Operation {
      method: "delete",
      path: "/places/{placeName}/entries/{entryPath}",
      id: "deleteEntry",
      owned: true,
      tag: "entries",
      summary: "Delete an entry.",
      parameters: vec![place_name(), entry_path()],
      body: None,
      answers: vec![(204, None)],
      codes: &[
        "unauthorized",
        "bad-request",
        "not-found",
        "conflict",
        "outside-place",
        "place-top",
        "protected",
        "no-permission",
        "failed-io",
        "no-bucket",
        "garage-closed",
        "storage-unready",
        "upstream-failed",
        "failed",
      ],
    },
    Operation {
      method: "get",
      path: "/places/{placeName}/entries/{entryPath}/content",
      id: "readContent",
      owned: true,
      tag: "entries",
      summary: "A file's bytes, with Range.",
      parameters: [vec![place_name(), entry_path()], query::<ContentQuery>(generator), vec![range.clone()]]
        .concat(),
      body: None,
      answers: vec![
        (200, bytes_answer("application/octet-stream")),
        (206, bytes_answer("application/octet-stream")),
      ],
      codes: &[
        "unauthorized",
        "bad-request",
        "not-found",
        "outside-place",
        "is-folder",
        "no-permission",
        "bucket-not-file",
        "garage-closed",
        "storage-unready",
        "upstream-failed",
        "failed",
      ],
    },
    Operation {
      method: "put",
      path: "/places/{placeName}/entries/{entryPath}/content",
      id: "writeContent",
      owned: true,
      tag: "entries",
      summary: "Write a file's bytes.",
      parameters: [vec![place_name(), entry_path()], query::<WriteQuery>(generator)].concat(),
      body: Some(Body::Bytes),
      answers: vec![(201, json_answer::<Entry>(generator)), (200, json_answer::<Entry>(generator))],
      codes: &[
        "unauthorized",
        "bad-request",
        "not-found",
        "conflict",
        "outside-place",
        "place-top",
        "quota-exceeded",
        "folder-taken",
        "protected",
        "no-permission",
        "failed-io",
        "outside-bucket",
        "no-bucket",
        "garage-closed",
        "storage-unready",
        "upstream-failed",
        "failed",
      ],
    },
    Operation {
      method: "post",
      path: "/places/{placeName}/folders",
      id: "createFolder",
      owned: true,
      tag: "entries",
      summary: "Make a folder.",
      parameters: vec![place_name()],
      body: json_body::<FolderRequest>(generator),
      answers: vec![(201, json_answer::<Entry>(generator)), (200, json_answer::<Entry>(generator))],
      codes: &[
        "unauthorized",
        "bad-request",
        "not-found",
        "conflict",
        "outside-place",
        "place-top",
        "protected",
        "no-permission",
        "failed-io",
        "no-bucket",
        "garage-closed",
        "storage-unready",
        "upstream-failed",
        "failed",
      ],
    },
    Operation {
      method: "post",
      path: "/places/{placeName}/copies",
      id: "createCopies",
      owned: true,
      tag: "entries",
      summary: "Copy entries to a folder of this place or another.",
      parameters: vec![place_name()],
      body: json_body::<CopyRequest>(generator),
      answers: vec![(201, json_answer::<EntryPage>(generator))],
      codes: &[
        "unauthorized",
        "bad-request",
        "not-found",
        "conflict",
        "outside-place",
        "place-top",
        "into-itself",
        "quota-exceeded",
        "protected",
        "no-permission",
        "failed-io",
        "outside-bucket",
        "no-bucket",
        "garage-closed",
        "storage-unready",
        "upstream-failed",
        "failed",
      ],
    },
    Operation {
      method: "post",
      path: "/places/{placeName}/archives",
      id: "createArchive",
      owned: true,
      tag: "entries",
      summary: "The named entries as one zip, answered as bytes and kept nowhere.",
      parameters: vec![place_name()],
      body: json_body::<ArchiveRequest>(generator),
      answers: vec![(200, bytes_answer("application/zip"))],
      codes: &[
        "unauthorized",
        "bad-request",
        "not-found",
        "outside-place",
        "name-repeated",
        "no-permission",
        "garage-closed",
        "storage-unready",
        "upstream-failed",
        "failed",
      ],
    },
    Operation {
      method: "get",
      path: "/shares",
      id: "listShares",
      owned: true,
      tag: "shares",
      summary: "The live share links, the newest first.",
      parameters: query::<SharesQuery>(generator),
      body: None,
      answers: vec![(200, json_answer::<SharePage>(generator))],
      codes: &["unauthorized", "bad-request", "failed"],
    },
    Operation {
      method: "post",
      path: "/shares",
      id: "createShare",
      owned: true,
      tag: "shares",
      summary: "Share an entry by link, until a time or never.",
      parameters: Vec::new(),
      body: json_body::<ShareRequest>(generator),
      answers: vec![(201, json_answer::<Share>(generator)), (200, json_answer::<Share>(generator))],
      codes: &[
        "unauthorized",
        "bad-request",
        "not-found",
        "outside-place",
        "not-shareable",
        "bad-expiry",
        "no-permission",
        "garage-closed",
        "storage-unready",
        "upstream-failed",
        "failed",
      ],
    },
    Operation {
      method: "get",
      path: "/shares/{token}",
      id: "readShare",
      owned: false,
      tag: "shares",
      summary: "A share link and what it opens.",
      parameters: vec![token()],
      body: None,
      answers: vec![(200, json_answer::<Share>(generator))],
      codes: &["unauthorized", "not-found", "gone", "upstream-failed", "failed"],
    },
    Operation {
      method: "delete",
      path: "/shares/{token}",
      id: "deleteShare",
      owned: true,
      tag: "shares",
      summary: "End a share link.",
      parameters: vec![token()],
      body: None,
      answers: vec![(204, None)],
      codes: &["unauthorized", "bad-request", "not-found", "failed"],
    },
    Operation {
      method: "get",
      path: "/shares/{token}/content",
      id: "readShareContent",
      owned: false,
      tag: "shares",
      summary: "The shared bytes: a file as a download, a folder as its zip.",
      parameters: vec![token(), range],
      body: None,
      answers: vec![
        (200, bytes_answer("application/octet-stream")),
        (206, bytes_answer("application/octet-stream")),
      ],
      codes: &["unauthorized", "not-found", "gone", "upstream-failed", "failed"],
    },
    Operation {
      method: "get",
      path: MANIFEST_PATH,
      id: "readConsoleManifest",
      owned: false,
      tag: "console",
      summary: "The console bundle's manifest, by bmo-ui's console plugin contract.",
      parameters: Vec::new(),
      body: None,
      answers: vec![(200, json_answer::<ConsoleManifest>(generator))],
      codes: &["unauthorized", "not-found"],
    },
    Operation {
      method: "get",
      path: WORDS_PATH,
      id: "readConsoleWords",
      owned: false,
      tag: "console",
      summary: "The console bundle's words: each locale's tree under the locale's code.",
      parameters: Vec::new(),
      body: None,
      answers: vec![(200, Some(json!({ "application/json": { "schema": { "type": "object" } } })))],
      codes: &["unauthorized", "not-found"],
    },
    Operation {
      method: "get",
      path: "/console/{file}",
      id: "readConsoleFile",
      owned: true,
      tag: "console",
      summary: "The console bundle's module or stylesheet, at the path the manifest names.",
      parameters: vec![segment("file", "The file's name, as the manifest's path ends.")],
      body: None,
      answers: vec![(
        200,
        Some(json!({
          "text/javascript": { "schema": { "type": "string" } },
          "text/css": { "schema": { "type": "string" } },
        })),
      )],
      codes: &["unauthorized", "bad-request", "not-found"],
    },
  ]
}

/// The service's OpenAPI document.
pub fn openapi() -> Value {
  let mut generator = SchemaSettings::draft2020_12()
    .for_serialize()
    .with(|settings| {
      settings.definitions_path = SCHEMAS.into();
      settings.meta_schema = None;
    })
    .into_generator();
  let error = generator.subschema_for::<WireError>();
  let mut tags: Vec<&str> = Vec::new();
  let mut paths = Map::new();

  for operation in operations(&mut generator) {
    if !tags.contains(&operation.tag) {
      tags.push(operation.tag);
    }

    let (path, method) = (operation.path, operation.method);

    paths.entry(path).or_insert_with(|| json!({}))[method] = operation.value();
  }

  let x_tools: Vec<Value> = tools()
    .iter()
    .map(|tool| {
      json!({
        "name": tool.name,
        "description": tool.description,
        "input": tool.input(&mut generator),
        "output": tool.output(&mut generator),
        "route": tool.route,
      })
    })
    .collect();

  json!({
    "openapi": "3.1.0",
    "info": { "title": "bmo-drive", "version": env!("CARGO_PKG_VERSION"), "description": DESCRIPTION },
    "servers": [{ "url": "/", "description": "the drive service" }],
    "security": [{ "bearer": [] }],
    "x-layer": "upstream",
    "x-stage": 2,
    "x-calls": ["node"],
    "x-harness": {
      "plugin": "bmo-drive",
      "mcp": "bmo-drive mcp",
      "entry": "A harness lists the places with drive_places, a folder with drive_entries, reads and writes \
        files with drive_read and drive_write, and hands someone a link with drive_share.",
    },
    "tags": tags.iter().map(|tag| json!({ "name": tag })).collect::<Vec<_>>(),
    "paths": paths,
    "components": {
      "schemas": generator.take_definitions(true),
      "responses": {
        "Error": {
          "description": "A refusal, in the common error shape.",
          "content": { "application/json": { "schema": error } },
        },
      },
      "securitySchemes": {
        "bearer": {
          "type": "http",
          "scheme": "bearer",
          "description": "The token of the service's settings.",
        },
      },
    },
    "x-events": KINDS,
    "x-tools": x_tools,
  })
}

/// The document as the repository keeps it: two-space indented JSON with a closing newline.
pub fn openapi_json() -> String {
  let mut text = serde_json::to_string_pretty(&openapi()).unwrap_or_default();

  text.push('\n');
  text
}
