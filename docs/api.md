# The drive service's boundary

Every route this service answers, its command, every call it makes and its settings. This document is
the contract other units match: bmo-web, whose console passes the Files screen's and the share page's
requests on to these routes; bmo-cli, which runs the command; and the switch-over, which points the
console at this service. What the file viewer does is `docs/files.md`; this document says how it is
asked. A change to anything here changes this document in the same commit and follows the version rule
(「Versions」).

## Conventions

- **Every route needs `Authorization: Bearer <token>`,** the token of the settings; a missing or wrong
  token is answered 401 `unauthorized`. The console presents it on every request it passes on, the share
  page's included: a share page asks for no sign-in in the console, and the token is the console's, not
  the holder's.
- **Bodies are JSON, both ways, with field names in camelCase,** but for an upload's body, which is the
  file's bytes, and a file's or a zip's answer, which is the bytes.
- **Times are Unix seconds,** but for an entry's `modified`, which is Unix milliseconds.
- **A refusal** is a non-2xx answer whose body carries an English sentence under `error` and a
  snake_case code under `code` (「Refusals」). The sentence is for a log; a screen shows the code in its
  own language's words. A conflict carries `names` beside them.
- **Paths and places** are as `docs/files.md` states them: a place is `volume:<mount>`, `garage` or
  `node:<name>:<mount>`; a path is names joined by `/`, the empty path being the place's top.
- **A query parameter that is a switch** is on where it is `1` and off otherwise.

## The file viewer

| Method and path | Sends | Answers |
|---|---|---|
| `GET /api/files/places` | | `machines`: this machine read with its drives and Garage, then each node of the node list with its drives unread, in the list's order |
| `GET /api/files/machine` | query `machine` | one machine read now: `local`, or `node:<name>` |
| `GET /api/files/list` | query `place`, `path`, `sort?` (`name`, `size`, `modified`), `order?` (`asc`, `desc`), `offset?`, `limit?` (1 to 1000, 200 where none), `folders?` (folders alone), `fresh?` (read again) | `place`, `path`, `total` (the entries of the whole folder), `offset`, and `entries`, the page |
| `GET /api/files/read` | query `place`, `path`, `download?`, `head?` (the first so many bytes alone); the `Range` header | the file's bytes (「A file's answer」) |
| `GET /api/files/zip` | query `place`, `path` | the folder's zip (「A zip's answer」) |
| `POST /api/files/folder` | `place`, `path`, `conflict?` | `name`: the name it was made under, null where it was skipped |
| `POST /api/files/upload` | query `place`, `path`, `conflict?`; the file's bytes as the body, `application/octet-stream`, with `Content-Length` | `name`: the name it was written under, null where it was skipped |
| `POST /api/files/rename` | `place`, `path`, `name` | `name` |
| `POST /api/files/copy` | `from` (`place`, `paths`), `to` (`place`, `path`), `move?`, `conflict?` | `names`: the names the entries arrived under, a skipped one left out |
| `POST /api/files/delete` | `place`, `paths` | an empty object |

- **`conflict`** is `refuse` (where none is given), `replace`, `rename` (kept both) or `skip`
  (`docs/files.md`, 「Changing files」). A refused one answers 409 `name_taken` with `names`, every entry
  that would collide, before anything is changed.
- **Only the parameters named above are read;** any other the request carries is passed over.

### What a machine is

| Field | What it is |
|---|---|
| `key` | `local` for this machine, `node:<name>` for a node |
| `name` | this machine's host name, or the node's label |
| `local` | whether it is this machine |
| `disks` | its drives, or null for a node not yet read or that could not be read |
| `garage` | this machine's Garage where it has one, else null |
| `error`, `errorCode` | why a node could not be read: an English sentence and a code (「Refusals」), else null |

A drive is `key` (the kernel's name, or `net:<source>` for a network share), `name` (its model, or its
kernel name, or the share's source), `kind` (`nvme`, `ssd`, `hdd`, `usb`, `network`, `virtual`, or null),
`sizeBytes` (or null) and `volumes`, each `place`, `mount` and `fsType`.

Garage is `place` (`garage`), `disk` and `mount` (the drive key and the mount its data stands on, or
null), `dataDir` (or null), and `error` and `errorCode` where it cannot be opened, else null.

### What an entry is

`name`, `kind` (`folder`, `file` or `other`), `size` (bytes, a file's alone, else null) and `modified`
(Unix milliseconds, or null).

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

`content-type: application/zip`, `content-disposition: attachment` under the folder's name with `.zip`
after it (at a place's top, the place's own last name), `accept-ranges: none`, and no length; and the
headers every shown file carries: `x-content-type-options: nosniff`, `cache-control: private, no-store`
and, a zip being no PDF, the sandbox `content-security-policy` a file read answers with. A file, a
missing path or a place that cannot be read is refused before the answer starts; an entry that fails
once the zip has started ends the answer as broken.

## Share links

| Method and path | Sends | Answers |
|---|---|---|
| `GET /api/files/shares` | | `shares`: every live link, the newest first, each a link (below); links whose time has passed are swept first |
| `POST /api/files/shares` | `place`, `path`, `span`, `by` | `share`: the link standing for that entry as it is, or one made now |
| `DELETE /api/files/shares/{token}` | | `removed`: whether there was such a link |
| `GET /api/share/files/{token}` | | what a holder is shown: `name`, `kind`, `size` (a file's, else null) and `expiresAt` (null for no end); with `cache-control: private, no-store` |
| `GET /api/share/files/{token}/download` | the `Range` header | a file as a download, answered as `GET /api/files/read` answers it with `download`; a folder as its zip; with `cache-control: private, no-store` |

- **A link** is `token`, `place`, `path`, `name` (what its entry is handed out under), `kind` (`file` or
  `folder`), `createdBy`, `createdAt` and `expiresAt` (null for no end). It carries its token, so only a
  signed-in screen is handed one.
- **`span`** is one of `1h`, `6h`, `24h`, `7d`, `30d` and `forever`; the link lives that long from now,
  and `forever` makes one with no end. Any other is refused 400 `bad_expiry` before anything is read.
- **`by`** is who makes the link, which the console names as the signed-in user; none, or only spaces, is
  refused 400 `bad_maker`.
- **The entry is read first,** and judged a file or a folder there; anything else is refused 400
  `not_shareable`, nothing there 404 `not_found`.
- **The two a holder reaches** refuse a token that names no link with 404 `share_not_found`, one whose
  time has passed with 410 `share_expired`, and a link whose entry no longer stands as that kind at its
  path with 404 `shared_entry_gone`. They read nothing of the request but the token and the range.

## Health

`GET /health` answers `pid`, `version` and `ready`: the store is open and migrated.

## Refusals

| Status | Code | When |
|---|---|---|
| 400 | `bad_request` | the body is not JSON of the route's shape |
| 400 | `missing_field` | a field the route needs is not sent; the message names it |
| 400 | `missing_place` | no `place` |
| 400 | `missing_machine` | no `machine` |
| 400 | `bad_count` | `offset`, `limit` or `head` is not a whole number from 0 |
| 400 | `bad_sort` | `sort` is not a column |
| 400 | `bad_order` | `order` is not `asc` or `desc` |
| 400 | `bad_conflict` | `conflict` is not one of its four |
| 400 | `bad_name` | a new name that cannot stand in a path |
| 400 | `outside_place` | a path that would leave its place |
| 400 | `place_top` | a change asked of a place's top |
| 400 | `into_itself` | a folder copied or moved into itself or under it |
| 400 | `is_folder` | a folder asked to be read as a file |
| 400 | `not_folder` | a file asked to be listed as a folder |
| 400 | `zip_of_file` | a file asked for as a zip |
| 400 | `cut_short` | an upload that ended before its length |
| 400 | `folder_taken` | a folder of that name stands where a file is to go |
| 400 | `failed_io` | the file system refused for another reason; the message says it |
| 400 | `bucket_not_file` | a bucket asked to be read as a file |
| 400 | `outside_bucket` | a file or folder asked at Garage's top, outside every bucket |
| 400 | `bucket_rename` | a bucket asked to be renamed |
| 400 | `bucket_move` | a bucket asked to be moved |
| 400 | `no_bucket` | a change at Garage that names no bucket |
| 400 | `bad_expiry` | `span` is not one of its six |
| 400 | `bad_maker` | no `by` |
| 400 | `not_shareable` | a share link asked for what is neither a file nor a folder |
| 401 | `unauthorized` | the token is missing or wrong |
| 403 | `protected` | a change landing in or taking a protected location |
| 403 | `no_permission` | the account may not read or change it |
| 403 | `garage_forbidden` | Garage refused the profile's key |
| 404 | `not_found` | nothing at that path |
| 404 | `no_place` | a place the current reading does not list |
| 404 | `no_machine` | a machine key that is neither `local` nor a listed node |
| 404 | `no_node` | a node the node list does not hold |
| 404 | `share_not_found` | a token that names no link |
| 404 | `shared_entry_gone` | a link whose entry no longer stands |
| 404 | `no_route` | no such route |
| 409 | `name_taken` | a name already taken in the folder; with `names` |
| 409 | `bucket_not_empty` | a bucket asked to be deleted that still holds objects |
| 410 | `share_expired` | a link whose time has passed |
| 411 | `length_required` | an upload with no `Content-Length` |
| 502 | `drives_unread` | this machine's drives could not be listed |
| 502 | `nodes_unread` | the node service did not answer for the node list |
| 502 | `node_incomplete` | the node service holds no usable connection for the node |
| 502 | `node_unreachable` | the node could not be reached; the message carries the login's last line |
| 502 | `node_failed` | the node's script failed; the message carries its last line or its exit code |
| 502 | `node_no_length` | the node did not say a file's length |
| 502 | `node_upload_failed` | the upload could not be put onto the node |
| 502 | `garage_failed` | Garage did not answer as asked; the message carries what it said |
| 503 | `store_failed` | the share store could not be read or written |
| 503 | `garage_config_unreadable`, `aws_config_unreadable`, `garage_profile_missing`, `garage_profile_incomplete` | Garage was asked for while it cannot be opened, for the reason its place states (below) |
| 504 | `node_timeout` | the node did not answer in time |
| 504 | `disk_timeout` | the disk did not answer in time |
| 500 | `failed` | anything else; the message says it |

A machine's `errorCode` is one of `node_incomplete`, `node_unreachable`, `node_failed` and
`node_timeout`, or the refusal code the node's drives script ended with: `not_found`, `outside_place`,
`not_folder`, `is_folder`, `no_permission`, `protected`, `name_taken`, `cut_short` or `into_itself`. A
node whose block devices or mounts cannot be listed shows `no_permission`. Garage's is one of `garage_config_unreadable` (Garage's configuration file could not be
read), `aws_config_unreadable` (the AWS configuration could not be read), `garage_profile_missing` (the
profile is not in it) and `garage_profile_incomplete` (the profile lacks its endpoint, region or keys).
A Garage that could not be reached while answering is refused 502 `garage_failed`.

## The command

`bmo-drive` is one binary, the service and its command alike, read by bmo-cli as a unit command
(bmo-cli's `docs/boundary.md`, 「Unit commands」).

- **`bmo-drive serve`** opens and migrates the store and answers the routes.
- **`bmo-drive health`** asks the service the settings name, waiting at most five seconds, and ends 0
  where it answers ready.
- **`bmo-drive nouns`** prints the nouns it carries, one a line, each marked `home` after a tab, since
  its links are of the files of the machine the service stands on:
  `file share` and `file-share`.
- **`bmo-drive file share`**, also run as `bmo-drive file-share`, and so `bmo file share`:
  - `<path> [--days <n> | --forever]`: a link for a file or folder on this machine, the path given to
    the volume it lies on, the deepest mount point that holds it; a week where no span is named;
  - `--place <id> <path> [--days <n> | --forever]`: the path read inside that place;
  - `--list`: every live link;
  - `--remove <token>`: one link taken away.
  - It makes, lists and removes links through the same share module the routes use, in its own
    process, recording the settings' maker as who made a link.
  - **What it prints:** for each link, `file-share: <kind> <place> <path>, until <time>` (or
    `no expiry`; the path `(top)` at a place's top), `page: <console>/share/files/<token>`,
    `download: <console>/api/share/files/<token>/download` and `token: <token>`; `file-share: no live
    links.` where there are none; `file-share: the link is taken away.` on a removal.
  - **How it ends:** 0 on success; a refusal on standard error and 1, the usage among them where the
    words are not one of the four forms or `--days` is not above 0; and a removal of no such link
    prints `file-share: no such link.` and ends 1.

## Calls this service makes

### The node service

Each to the node service's address with its token as a bearer, as bmo-node's `docs/api.md` states it:
camelCase bodies, and a refusal as 200 with `answered` false and a `refusal` carrying its `code`.

| Call | Route | Why |
|---|---|---|
| the node list | `GET /nodes/view` | each node's `name`, `label`, `kind` and `thisMachine`: the machines the viewer lists after this one, a node that is this machine left out |
| a node's connection | `GET /nodes/{name}/target` | an ssh node's `host`, `port`, `user`, `keyPath?` and `shell?`, or a jake node's `hubUrl`, `tokenFile` and `agent`; refused `no-node` or `incomplete` |
| a node's login | `POST /nodes/login` with `node` | the arguments that open a batch SSH login to the node, ahead of the command; null where the node has no SSH route |

- **An SFTP put** onto a Windows node takes the login's options as the login route hands them, up to the
  port and the address, which it gives in SFTP's own form. bmo-node serves no SFTP form of the login; the
  login route's documented order is what this rests on.
- **A node service that does not answer** leaves the places with this machine alone, and a node asked
  for answers 502 `nodes_unread`.

### What it reaches beside the units

- **A jake hub,** an outside program a jake agent attaches to, at the `hubUrl` the node's connection
  names, with the token read from its `tokenFile` for each request: `POST /exec?agent=<agent>` to run a
  script, `POST /cancel` to stop one, and `POST /upload?path=&agent=` to put an upload's bytes onto the
  node.
- **OpenSSH and SFTP** on this machine, for an ssh node.
- **Garage** over S3, with the AWS profile of the settings.
- **`lsblk` and the mount table** of this machine.

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
| `store` | `path` | the SQLite file the share links stand in |
| `console` | `address` | the console's public address, the base of a link's page and download |
| `node` | `address`, `token` | the node service's base address and its token |
| `garage` | `config` | Garage's configuration file, `/etc/garage.toml` where none is set |
| `garage` | `profile` | the AWS profile Garage is reached through, `garage` where none is set |
| `share` | `maker` | who the command records as a link's maker |
| `protect.posix` | `system`, `top`, `home` | a POSIX machine's protected folders, folders whose own entries are protected, and folders under the login's home |
| `protect.posix` | `home_named` | each `under` (a folder under the home) and `starting` (the word every protected entry's name begins with) |
| `protect.windows` | `system`, `home` | the environment variables naming a Windows machine's protected folders, and folders under the user's profile |
| `protect.garage` | `buckets_starting` | the words a protected bucket's name begins with |
| `protect.local` | `extra` | further folders of this machine, protected with everything in them |

## What the console does

These are bmo-web's to carry; this repository holds none of them.

- **Pass on** every request under `/api/files/` and `/api/share/files/`, whole — the method, the query,
  the body as it streams and the headers that say what is asked, `Range` and `Content-Length` among
  them — to the address `DRIVE_ADDRESS` names with the token `DRIVE_TOKEN` names, and hand the answer back
  as it came, a file's bytes as they stream. The routes under `/api/files/` are behind the console's
  sign-in, the administrator's alone; those under `/api/share/files/` ask for no sign-in.
- **Name the maker:** `POST /api/files/shares` carries the signed-in user as `by`, whatever the browser
  sent.
- **Extend the console plugin bundle** under `client/`, and serve its Files page at `/files` and its
  share page at `/share/files/{token}`.
- **Hold the words:** the language pack's `files` keys the screen reads, and one key for each code
  above as `files.error.<code>`.

## Versions

The repository's version is the crate's, in semver. The major moves on a change to this document's
routes, fields, codes, command or settings that a caller must follow; the minor on an addition to any of
them; the patch on a change that leaves every statement here true.
