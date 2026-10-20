# The drive service's boundary

Every route this service answers, its command, its harness plugin, every call it makes and its settings.
What the file viewer does is `docs/files.md`; this document says how it is asked. The routes follow the
common wire format and the API design of bmo-chat's `docs/conventions.md`, which this document does not
restate. The OpenAPI document `docs/openapi.json` is the contract in machine form, generated from the
service's own types by `bmo-drive openapi`. A change to anything here changes this document and that one
in the same commit and follows the version rule (「Versions」).

## Conventions

- **Every route needs `Authorization: Bearer <token>`,** the token of the settings; a missing or wrong
  token is answered 401 `unauthorized`.
- **Bodies are JSON both ways, keys in camelCase,** but for a file's bytes written or read and a zip's
  answer.
- **Times are epoch milliseconds.**
- **A refusal** is a non-2xx answer whose body carries a kebab-case `code` and an English sentence under
  `error` (「Refusals」). A conflict carries `names` beside them.
- **A place is named by its name** (「Places」), and a path inside it is names joined by `/`, the empty path
  being the place's top (`docs/files.md`, 「Paths and the fence」). In a route's path, `{placeName}` and
  `{entryPath}` are each one segment, so a `/` or a `:` inside them is written percent-encoded (`%2F`,
  `%3A`) as any URI path parameter is.
- **A list is a page,** `{"items": [...], "more": boolean}`, asked with `after` and `limit`. Each list says
  what its cursor is.
- **A switch in a query** is on where it is `true`.
- **Only the parameters and fields named below are read;** any other a request carries is passed over.

## Health

`GET /health` answers `ready` (the store is open and migrated), `reason` (null while ready) and `version`.

## Events

`GET /events` is the service's one event stream, as server-sent events. Each event's SSE `event` is its
kind, its `id` the event number, and its `data` `{"id", "kind", "at", "data"}`.

| Kind | When | Inner `data` |
|---|---|---|
| `entry.created` | an entry is made through this service: a folder, a file written where none stood, an entry copied or moved in | the entry |
| `entry.changed` | a file written over through this service | the entry |
| `entry.deleted` | an entry deleted, or moved away, through this service | `placeName` and `path` |
| `share.created` | a share link made | the link |
| `share.deleted` | a share link taken away, or swept once its time has passed | `token` |

- **Only changes made through this service are events.** A file changed on the disk by anything else is
  not seen.
- **A reader resumes** with `Last-Event-ID` or `after` (an event number), and narrows with `kinds`, a
  comma-separated list of kinds. The service keeps the last 10,000 events; a resume from before them is
  answered 410 `gone`.
- **With `Accept: application/json`** the same events are a page, by `after` and `limit` (100 where none,
  1 to 1000).
- **A comment line every fifteen seconds** keeps the stream open.

## Places

| Method and path | Sends | Answers |
|---|---|---|
| `GET /places` | query `nodeName?`, `after?`, `limit?` | 200, a page of places |
| `GET /machine` | query `nodeName?` | 200, a machine |

- **`GET /places`** lists this machine's places, read now: its volumes, then its Garage where it has one.
  With `nodeName` it lists that node's volumes instead, from the node's reading of the last minute or, where
  none stands, read now (`docs/files.md`, 「A node's drives」). The places stand in that one order, and the cursor `after` is how many of them were
  already read; `limit` is 1 to 1000, 1000 where none is asked.
- **`GET /machine`** is this machine and its drives, read now; with `nodeName`, that node, read now, the
  reading replacing the one its places were listed from.
- **A node that cannot be read** is refused 502 `upstream-failed`, the sentence saying why; a node the node
  list does not hold is refused 404 `not-found`.

### What a place is

| Field | What it is |
|---|---|
| `name` | its key: `volume:<mount>` for a volume of this machine, `garage` for Garage, `node:<nodeName>:<mount>` for a node's volume |
| `kind` | `volume`, `store` (Garage) or `node` |
| `label` | what a person reads it as: the mount, `Garage`, or the mount on the node |
| `freeBytes` | the bytes free on a volume of this machine; null for Garage and a node's volume |
| `nodeName` | the node it stands on, null for this machine |
| `driveKey` | the drive it stands on (「What a machine is」), null for Garage where its data stands on no listed drive |
| `mount` | where it is mounted, null for Garage |
| `reason`, `reasonCode` | why it cannot be opened, an English sentence and a code, else null |

Garage's `reasonCode` is `garage-config-unreadable` (Garage's configuration file could not be read),
`aws-config-unreadable` (the AWS configuration could not be read), `garage-profile-missing` (the profile is
not in it) or `garage-profile-incomplete` (the profile lacks its endpoint, region or keys).

### What a machine is

| Field | What it is |
|---|---|
| `name` | this machine's host name, or the node's label |
| `nodeName` | the node, null for this machine |
| `drives` | its drives |

A drive is `key` (the kernel's name, or `net:<source>` for a network share), `name` (its model, or its
kernel name, or the share's source), `kind` (`nvme`, `ssd`, `hdd`, `usb`, `network`, `virtual`, or null),
`byteCount` (its size, or null) and `volumes`, each `placeName`, `mount` and `fsType`.

## Entries

| Method and path | Sends | Answers |
|---|---|---|
| `GET /places/{placeName}/entries` | query `path?`, `sort?`, `order?`, `folders?`, `fresh?`, `after?`, `limit?` | 200, a page of the folder's entries |
| `GET /places/{placeName}/entries/{entryPath}` | | 200, the entry |
| `PATCH /places/{placeName}/entries/{entryPath}` | `path`, `conflict?` | 200, the entry where it now stands |
| `DELETE /places/{placeName}/entries/{entryPath}` | | 204 |
| `GET /places/{placeName}/entries/{entryPath}/content` | query `download?`, `head?`; the `Range` header | the file's bytes (「A file's answer」) |
| `PUT /places/{placeName}/entries/{entryPath}/content` | query `conflict?`; the file's bytes as the body, `application/octet-stream`, with `Content-Length` | 201 where it was made, 200 where it was written over or skipped; the entry |
| `POST /places/{placeName}/folders` | `path`, `conflict?` | 201 where it was made, 200 where it was skipped; the entry |
| `POST /places/{placeName}/copies` | `paths`, `to`, `toPlaceName?`, `conflict?` | 201, a page of the entries as they arrived, a skipped one left out |
| `POST /places/{placeName}/archives` | `paths` | 200, the zip (「A zip's answer」), kept nowhere |

- **A listing** is one order over the whole folder (`docs/files.md`, 「Listing a folder」): `sort` is
  `name` (where none is asked), `byteCount` or `changedAt`; `order` is `asc` (where none is asked) or
  `desc`; `folders` lists the folders alone, and `fresh` reads the folder again rather than from what was
  kept. The cursor `after` is how many entries of that order were already read; `limit` is 1 to 1000, 200
  where none is asked. The page carries `totalCount` beside `items` and `more`, the entries the whole
  folder holds under that asking.
- **`PATCH`** moves the entry to `path` in the same place: a new name in its own folder is a rename, a path
  in another folder a move. Moving it onto itself changes nothing.
- **`conflict`** settles a name already taken where an entry lands (`docs/files.md`, 「Changing files」):
  `refuse`, `replace`, `rename` (kept both) or `skip`. Where none is asked, writing a file's bytes
  replaces, and every other change refuses. A refused one answers 409 `conflict` with `names`, every entry
  that would collide, before anything is changed. `PATCH` takes no `skip`.
- **`copies`** copies the entries at `paths` of this place into the folder at `to` of the place
  `toPlaceName`, this same one where none is named. A move between two places is a copy followed by a
  `DELETE` of each entry copied.
- **`archives`** zips the entries at `paths`. One folder named is zipped with its entries under their paths
  from it; otherwise each named entry stands at the zip's top under its own name, a folder with everything
  under it, and two named entries sharing a name are refused 422 `name-repeated`.

### What an entry is

`placeName`, `path`, `name` (the path's last name, empty at a place's top), `kind` (`folder`, `file` or
`other`, a device, a socket or a link leading nowhere), `byteCount` (a file's alone, else null) and
`changedAt` (when it last changed, or null where the place keeps no time).

### A file's answer

- **Status:** 200 for the whole or its head; 206 for one range asked in `Range`; 416 for a range that
  names no byte of the file, with no body.
- **Headers:**
  - `content-type`, guessed from the name, text said to be UTF-8;
  - `content-length`;
  - `content-disposition`, `inline` or, with `download`, `attachment`, naming the file both plainly and
    in UTF-8;
  - `accept-ranges: bytes`;
  - `x-file-size`, the whole file's length;
  - `content-range` for 206 and 416;
  - `x-content-type-options: nosniff` and `cache-control: private, no-store`;
  - and for anything but a PDF,
    `content-security-policy: sandbox; default-src 'none'; img-src 'self' data: blob:; media-src 'self'; style-src 'unsafe-inline'`.
- **A stream that fails partway** ends the answer there as broken.

### A zip's answer

`content-type: application/zip`, `content-disposition: attachment` under the one entry's name with `.zip`
after it, or with several the name of the folder the first stands in (at a place's top, the place's own
last name), `accept-ranges: none`, and no length; and the headers every shown file carries:
`x-content-type-options: nosniff`, `cache-control: private, no-store` and, a zip being no PDF, the sandbox
`content-security-policy`. A missing path or a place that cannot be read is refused before the answer
starts; an entry that fails once the zip has started ends the answer as broken.

## Share links

| Method and path | Sends | Answers |
|---|---|---|
| `GET /shares` | query `after?`, `limit?` | 200, a page of the live links, the newest first |
| `POST /shares` | `placeName`, `path`, `expiresAt?`, `actorName?` | 201, the link made; 200, the live link that already stands for that entry |
| `GET /shares/{token}` | | 200, the link |
| `DELETE /shares/{token}` | | 204 |
| `GET /shares/{token}/content` | the `Range` header | a file as a download, answered as a file's content is with `download`; a folder as its zip |

- **A link** is `token`, `placeName`, `path`, `name` (what its entry is handed out under: its own name, at a
  place's top the place's last name), `kind` (`file` or `folder`), `url` (the address its page stands
  at), `actorName` (who made it, null where none was named), `createdAt` and `expiresAt` (null for no
  end).
- **`GET /shares`** sweeps the links whose time has passed first. Its cursor `after` is a `createdAt`: the
  page holds the links made before it. `limit` is 1 to 1000, 100 where none is asked.
- **`expiresAt`** is when the link stops opening, a time to come, or null or absent for a link with no end;
  a time already passed is refused 422 `bad-expiry`.
- **The entry is read first,** and judged a file or a folder there; anything else is refused 422
  `not-shareable`, nothing there 404 `not-found`.
- **`GET /shares/{token}` and its content** refuse a token that names no link with 404 `not-found`, one
  whose time has passed with 410 `gone`, and a link whose entry no longer stands as that kind at its path
  with 404 `not-found`. They read nothing of the request but the token and the range.

## Refusals

Beside the common codes of bmo-chat's `docs/conventions.md` (「Errors」), which this service answers as
that document states them (`bad-request`, `unauthorized`, `no-route`, `not-found`, `conflict`, `gone`,
`failed`, `upstream-failed`), these are the service's own, each answered 422:

| Code | When |
|---|---|
| `bad-name` | a new name that cannot stand in a path |
| `outside-place` | a path that would leave its place |
| `place-top` | a change asked of a place's top |
| `into-itself` | a folder copied or moved into itself or under it |
| `is-folder` | a folder asked to be read as a file |
| `not-folder` | a file asked to be listed as a folder |
| `folder-taken` | a folder of that name stands where a file is to go |
| `name-repeated` | two entries of one zip sharing a name |
| `protected` | a change landing in or taking a protected location |
| `no-permission` | the account may not read or change it |
| `failed-io` | the file system refused for another reason; the sentence says it |
| `bucket-not-file` | a bucket asked to be read as a file |
| `outside-bucket` | a file or folder asked at Garage's top, outside every bucket |
| `bucket-rename` | a bucket asked to be renamed |
| `bucket-move` | a bucket asked to be moved |
| `no-bucket` | a change at Garage that names no bucket |
| `garage-closed` | Garage was asked for while it cannot be opened; the sentence gives its place's reason |
| `bad-expiry` | an `expiresAt` already passed |
| `not-shareable` | a share link asked for what is neither a file nor a folder |

- **`bad-request`** covers a body or query not of the route's shape, a field the route needs and does not
  get, a count, sort, order or conflict not among its own, an upload with no `Content-Length` and an upload
  that ended before its length; the sentence names which.
- **`not-found`** covers nothing at a path, a place the current reading does not list, a node the node list
  does not hold, and a share link as above.
- **`conflict`** covers a name already taken (with `names`) and a bucket asked to be deleted that still
  holds objects.
- **`upstream-failed`** covers the node service not answering, a node that could not be reached, whose
  script failed or that did not answer in time, an upload that could not be put onto a node, and Garage not
  answering as asked or refusing the profile's key; the sentence says which, with the login's or the
  script's last line where there is one, and never a credential.
- **`failed`** covers this machine's drives not listed, a disk that did not answer in time and the share
  store not read or written.

## The command

`bmo-drive` is one binary, the service, its command and its harness plugin alike.

- **`bmo-drive serve`** opens and migrates the store and answers the routes.
- **`bmo-drive health`** asks the service the settings name, waiting at most five seconds, and ends 0
  where it answers ready.
- **`bmo-drive mcp [--address <url>]`** serves the harness tools over MCP on standard input and output
  (「The harness plugin」).
- **`bmo-drive openapi`** prints the OpenAPI document the code generates, the one committed as
  `docs/openapi.json`.
- **`bmo-drive nouns`** prints the nouns the command carries, one a line, each marked `home` after a tab,
  since its links are of the files of the machine the service stands on: `file share` and `file-share`.
- **`bmo-drive file share`**, also run as `bmo-drive file-share`, asks the service the settings name
  through the service's own client:
  - `<path> [--days <n> | --forever]`: a link for a file or folder on this machine, the path given to the
    volume it lies on, the deepest mount point that holds it; a week where no span is named;
  - `--place <name> <path> [--days <n> | --forever]`: the path read inside that place;
  - `--list`: every live link;
  - `--remove <token>`: one link taken away.
  - It records the settings' actor as who made a link.
  - **What it prints:** for each link, `file-share: <kind> <placeName> <path>, until <time>` (or `no
    expiry`; the path `(top)` at a place's top), `page: <url>` and `token: <token>`; `file-share: no live
    links.` where there are none; `file-share: the link is taken away.` on a removal. With `--json` it
    prints the link or the page exactly as the service answers it, and nothing on a removal.
  - **How it ends:** 0 on success; a refusal on standard error and 1, the usage among them where the words
    are not one of the four forms or `--days` is not above 0; and a removal of no such link prints
    `file-share: no such link.` and ends 1.

## The harness plugin

The plugin is `ahas-drive`: the MCP server `bmo-drive mcp` starts and the skill
`skills/ahas-drive/SKILL.md`, which tells a harness when and how to use it. The server reaches the service
at `--address`, or at the settings' `server.listen` where none is given, with the settings' token, and each
tool is one call of the service's own client. A refusal comes back as a tool error carrying the route's
status, `code` and `error`. A harness lists the places with `drive_places`, a folder with `drive_entries`,
reads and writes files with `drive_read` and `drive_write`, and hands someone a link with `drive_share`.

| Tool | Calls | Input beside the route's own | Output |
|---|---|---|---|
| `drive_places` | `GET /places` | | a page of places |
| `drive_entries` | `GET /places/{placeName}/entries` | | a page of entries |
| `drive_move` | `PATCH /places/{placeName}/entries/{entryPath}` | | the entry |
| `drive_delete` | `DELETE /places/{placeName}/entries/{entryPath}` | | an empty object |
| `drive_read` | `GET /places/{placeName}/entries/{entryPath}/content` | `start?` and `byteCount?`, sent as the `Range` header | `content`, the bytes in base64; `start`; `byteCount`, how many it carries; `fileByteCount`, the whole file's |
| `drive_write` | `PUT /places/{placeName}/entries/{entryPath}/content` | `content`, the bytes in base64 | the entry |
| `drive_folder` | `POST /places/{placeName}/folders` | | the entry |
| `drive_copy` | `POST /places/{placeName}/copies` | | a page of entries |
| `drive_share` | `POST /shares` | | the link |

`drive_read` carries at most 4 MiB in one answer: with no `byteCount` it asks for the first 4 MiB from
`start`, and a harness reads a longer file a part at a time.

## The client

`bmo_drive::client` is the service's own Rust client: one method for each route, sending and reading the
types of `bmo_drive::wire`, which the routes answer with and the OpenAPI document is generated from. The
command and the MCP server call the service through it, and the console plugin bundle reads the TypeScript
types generated from `docs/openapi.json`.

## Calls this service makes

### The node service

Each to the node service's address with its token as a bearer, as bmo-node's `docs/api.md` states it.

| Call | Route | Why |
|---|---|---|
| the node list | `GET /nodes/view` | each node's `name` and `label`, to know a node asked for and name its machine |
| a node's connection | `GET /nodes/{name}/target` | an ssh node's `host`, `port`, `user`, `keyPath?` and `shell?`, or a jake node's `hubUrl`, `tokenFile` and `agent` |
| a node's login | `POST /nodes/login` with `node` | the arguments that open a batch SSH login to the node, ahead of the command; null where the node has no SSH route |

- **An SFTP put** onto a Windows node takes the login's options as the login route hands them, up to the
  port and the address, which it gives in SFTP's own form. bmo-node serves no SFTP form of the login; the
  login route's documented order is what this rests on.
- **A node service that does not answer** leaves this machine's places as they are, and a node asked for
  is refused 502 `upstream-failed`.

### What it reaches beside the units

- **A jake hub,** an outside program a jake agent attaches to, at the `hubUrl` the node's connection
  names, with the token read from its `tokenFile` for each request: `POST /exec?agent=<agent>` to run a
  script, `POST /cancel` to stop one, and `POST /upload?path=&agent=` to put an upload's bytes onto the
  node.
- **OpenSSH and SFTP** on this machine, for an ssh node.
- **Garage** over S3, with the AWS profile of the settings.
- **`lsblk`, the mount table and each volume's free space** on this machine.

## Settings

The settings are read at start in layers, each overriding the one before: the defaults, which are the
repository's own `settings.toml` built into the binary; then `settings.toml` and `settings.local.toml`
where they stand in the settings folder, the one `BMO_DRIVE_SETTINGS` names, else the folder above the one
holding the binary (a deployment's root); then `BMO_DRIVE_<TABLE>__<KEY>` in the environment. An empty
token lets no caller in.

| Table | Key | What it is |
|---|---|---|
| `server` | `listen` | where the service answers, as `host:port` |
| `server` | `token` | the bearer token every caller presents |
| `store` | `path` | the SQLite file the share links and the kept events stand in |
| `share` | `address` | the address a link's page stands under; a link's `url` is it, a `/` and the token |
| `share` | `actor` | who the command records as a link's `actorName`; none where empty |
| `node` | `address`, `token` | the node service's base address and its token |
| `garage` | `config` | Garage's configuration file, `/etc/garage.toml` where none is set |
| `garage` | `profile` | the AWS profile Garage is reached through, `garage` where none is set |
| `protect.posix` | `system`, `top`, `home` | a POSIX machine's protected folders, folders whose own entries are protected, and folders under the login's home |
| `protect.posix` | `home_named` | each `under` (a folder under the home) and `starting` (the word every protected entry's name begins with) |
| `protect.windows` | `system`, `home` | the environment variables naming a Windows machine's protected folders, and folders under the user's profile |
| `protect.garage` | `buckets_starting` | the words a protected bucket's name begins with |
| `protect.local` | `extra` | further folders of this machine, protected with everything in them |

## Versions

The repository's version is the crate's, `0.1.0` followed by `-rev.<n>` until the first release: each
change to this document's routes, fields, codes, command, plugin or settings raises the revision alone.
The OpenAPI document and the console plugin bundle carry the same version.
