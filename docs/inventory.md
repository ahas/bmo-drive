# What this repository takes from the old bmo

Every file, crate, policy file, test and service unit this repository takes from the old single
repository, with where each came from, how many, and what becomes of it. Every set below was listed from
the old repository at its commit `3e4c032`, with `git ls-tree`, `git show` and `git grep` against that
commit; the command that lists a set stands beside it. Nothing here was copied with its history.

The tests are deferred: none is carried, written or run in this phase. They are written and run here
once the minimal services stand on the new repositories. The deferred tests are listed below so that
work knows what the old behaviour was held to.

## The server code: 14 files, 6,070 lines

`git ls-tree -r --name-only 3e4c032 bot/bmo-server/src/control/files`, each counted with `wc -l`.

| Old file | Lines | Here | Treatment |
|---|---|---|---|
| `mod.rs` | 923 | the places, listing and reading module | carried; its routes answer at the console's paths |
| `change.rs` | 500 | the changes module | carried |
| `drives.rs` | 387 | the drives module | carried |
| `volume.rs` | 468 | the volume place | carried |
| `guard.rs` | 103 | the protected locations | carried; its lists read from this repository's settings |
| `order.rs` | 92 | the natural order of names | carried |
| `zip.rs` | 322 | the folder zip | carried |
| `garage.rs` | 750 | the Garage place | carried; its configuration file and profile read from settings |
| `node/mod.rs` | 486 | the node machines | carried; the node list read from bmo-node |
| `node/posix.rs` | 500 | the POSIX shell's scripts | carried |
| `node/powershell.rs` | 349 | the PowerShell scripts | carried |
| `node/road.rs` | 458 | the roads onto a node | carried; the login read from bmo-node |
| `node/shell.rs` | 249 | what every shell's script shares | carried |
| `shares.rs` | 483 | the share links | carried onto this repository's own store |

### The routes: 15

`git grep -n 'const PATH' 3e4c032 -- bot/bmo-server/src/control/files`. Each answers here at the
console's own path (`docs/api.md`).

| Old route | Here |
|---|---|
| `GET /files/places` | `GET /api/files/places` |
| `GET /files/machine` | `GET /api/files/machine` |
| `GET /files/list` | `GET /api/files/list` |
| `GET /files/read` | `GET /api/files/read` |
| `GET /files/zip` | `GET /api/files/zip` |
| `POST /files/folder` | `POST /api/files/folder` |
| `POST /files/upload` | `POST /api/files/upload` |
| `POST /files/rename` | `POST /api/files/rename` |
| `POST /files/copy` | `POST /api/files/copy` |
| `POST /files/delete` | `POST /api/files/delete` |
| `GET /files/shares` | `GET /api/files/shares` |
| `POST /files/shares` | `POST /api/files/shares`, taking a span and a maker rather than seconds and `createdBy` |
| `DELETE /files/shares` (token in the query) | `DELETE /api/files/shares/{token}` |
| `GET /files/shared` (token in the query) | `GET /api/share/files/{token}` |
| `GET /files/shared/download` (token in the query) | `GET /api/share/files/{token}/download` |

### The command: one part of one file

`bot/bmo-server/src/verbs/files.rs`, read with `git show` and its items listed with
`grep -n -E '^(pub )?(async )?fn |^pub (struct|enum)|^impl'`.

| Old item | Here |
|---|---|
| the `FileShare` variant of the file verbs, its usage text and its dispatch | the `file share` command of `bmo-drive` |
| `ShareAsked` | carried |
| `Control` (`new`, `of_process`, `ask`, `lines_of`, `volume_of`) | not carried as a client: the command works through this repository's own share and place modules in its own process; `lines_of` and `volume_of` are carried as what they print and how a path finds its volume |
| `file_share` | carried |

Every other item of that file (the room file, the room upload, the mp3 conversion and what they share)
is the receiver's, which carries it in its own repository.

### What the old server lent these files: 2 sources

| Old item | Here |
|---|---|
| `control/mod.rs`: `Route`, `Call`, `Refused`, `Standing`, `answer`, the `moved!` registration and `whole` | this repository's own HTTP plumbing: a route registers itself beside its answer, a refusal carries an English message and a code, and the settings stand in for `Standing` |
| `control/nodes.rs`: `ThisMachine` | not carried: bmo-node's node view says which node is this machine (`thisMachine`) |

### Crates of other units: 2

| Old crate | Items used | Here |
|---|---|---|
| `node-check` | `rows` (`NodeRow`, `Target`, `Hub`, `SshTarget`), `ssh` (`SshRun`, `login_args`, `read_run`), `hub` (`hub_token`), `Machine`, `NodeState` | not carried: the node's connection and its SSH login are read from bmo-node's routes; a hub's token is read from the token file the node's connection names; the OpenSSH failure is told by the last line it wrote |
| `bmo-policy` | `file_writes`, `ssh`, `web.public_address` | `file_writes` becomes this repository's settings (below); `ssh` is bmo-node's, read through its login route; `web.public_address` becomes the console address of this repository's settings |

### Policy files: 3

`git grep -l -E 'FILE_WRITES|file_writes|file-writes' 3e4c032` finds the files code and the policy
crate alone, so the file-write rules are this unit's own.

| Old file | Readers | Here |
|---|---|---|
| `policy/bmo-policy/policy/file-writes.toml` with `src/file_writes.rs` | this unit alone | the protected locations of this repository's settings |
| `policy/bmo-policy/policy/ssh.toml` | bmo-node, which owns it | not carried; read through bmo-node's login route |
| `policy/bmo-policy/policy/web.toml` (`public_address`) | many | not carried; the console address is a setting here |

### The share table: 2 files

| Old file | Here |
|---|---|
| `archive/bmo-archive/src/migration/m20261006_000006_file_shares.rs` | the share table of this repository's own SQLite store, with the same columns and the same one link per entry |
| `archive/archive/file-share-rows.ts` | not carried: the store's own migration makes the table |

The rows standing in the old archive move with the switch-over, not here.

## The console's files: 27

The 27 files bmo-web's own inventory lists under bmo-drive, each read with
`git show 3e4c032:client/<path>`.

### Carried into the console plugin bundle: 9

`app/components/FilesItem.vue`, `app/components/FilesList.vue`, `app/components/FilesPreview.vue`,
`app/components/FilesShareList.vue`, `app/components/FilesTree.vue`, `app/domain/files.ts`,
`app/pages/files.vue`, `app/pages/share/files/[token].vue`, `shared/file-shares.ts`.

### Realised by this repository's routes: 16

The console's server routes passed each request on to the old server and added what this repository's
routes now do themselves: the query names passed, the shown-file headers, the span turned into seconds
and the maker named.

- `server/api/files/`: `copy.post.ts`, `delete.post.ts`, `folder.post.ts`, `list.get.ts`,
  `machine.get.ts`, `places.get.ts`, `read.get.ts`, `rename.post.ts`, `shares.get.ts`, `shares.post.ts`,
  `shares/[token].delete.ts`, `upload.post.ts` (12)
- `server/api/share/files/[token]/`: `index.get.ts`, `download.get.ts` (2)
- `server/utils/files.ts`, `server/utils/file-shares.ts` (2)

### Tests deferred: 2

`app/domain/files.test.ts` (17 tests) and `server/utils/files.test.ts` (6 tests).

### Copied beside them into the bundle: 8 sources

What the carried files import that bmo-ui does not export, each carried with only what the screen uses:

| Old source | What is taken |
|---|---|
| `app/domain/urls.ts` | `appUrl` |
| `app/domain/copy.ts` | `copyText` |
| `app/domain/attachments.ts` | `fileSizeText` |
| `app/domain/share-links.ts` | the whole file |
| `app/components/ShareLinkDialog.vue` | the whole file |
| `shared/config.ts` | `SHARE_SPANS` and `ShareSpanKey` |
| `shared/types.ts` | the file viewer's types: `FileDriveKind`, `FileVolume`, `FileDrive`, `FileGarage`, `FileMachine`, `FilePlaces`, `FileConflict`, `FileConflictRefusal`, `FileEntryKind`, `FileEntry`, `FileListing`, `FileSortKey`, `FileShareKind`, `FileShareView`, `SharedFileView` |
| `shared/share-head.ts` | `SHARE_ROBOTS` |

The share dialog and its road are also carried by the bmo-artifact bundle; a base home for them is
bmo-ui's to give.

### Already in bmo-ui: 4 components

`FilesNameField`, `FilesConflictDialog`, `FilesProgress` and `FilesText` stand in bmo-ui, which the
bundle takes as a peer. So do the modules the screen reads time, numbers, drag selection, the hold
menu, the bars, failures, file icons, the island, side parts and the screen width from.

### Owed by the host: 3 components

`DeployMark`, `TodoOpenButton` and `UsageMeters`, the console's own title-bar pieces.

### The screen's words

The screen reads its words from bmo-web's language pack under `files`, beside the `common` and
`screens` keys the console already holds. The old pack's whole `files` part, and a key for each refusal
code (`files.error.<code>`, `docs/api.md`), are bmo-web's to hold.

## Tests: deferred

None is carried. What the old behaviour was held to:

| Old place | Tests |
|---|---|
| `#[test]` functions inside the 14 server files | 35: `mod.rs` 7, `garage.rs` 4, `node/posix.rs` 4, `node/powershell.rs` 3, `node/shell.rs` 3, `volume.rs` 3, `change.rs` 2, `drives.rs` 2, `order.rs` 2, `shares.rs` 2, `zip.rs` 2, `guard.rs` 1, `node/mod.rs` 0, `node/road.rs` 0 |
| `#[test]` functions inside `policy/bmo-policy/src/file_writes.rs` | 1 |
| `bot/bmo-server/tests/integration/verbs_files.rs` | 1 of its 23: the share test; the rest are the receiver's |
| `bot/bmo-server-tests/control-files.test.ts` | 14 |
| `bot/bmo-server-tests/control-file-shares.test.ts` | 7 |
| `bot/bmo-server-tests/control-file-shares-postgres.test.ts` | runs the share tests above against PostgreSQL |
| `client/app/domain/files.test.ts`, `client/server/utils/files.test.ts` | 17 and 6 |

Counted with `grep -c -E '^\s*#\[(tokio::)?test'` on each Rust file and `grep -c -E '^\s*(test|it)\('`
on each TypeScript file.

## Hangul in the carried code: 93 lines

`git show 3e4c032:<file> | grep -n -P '[\x{AC00}-\x{D7A3}]'` (every Hangul syllable) on each of the 14 server files, split at the file's first
`#[cfg(test)]`. No escaped Hangul (unicode or byte escapes, char codes) stands in any of them.

- **78 outside the test modules,** in 9 files: `garage.rs` 19, `mod.rs` 18, `node/mod.rs` 11,
  `change.rs` 10, `shares.rs` 10, `volume.rs` 5, `node/road.rs` 2, `node/shell.rs` 2, `zip.rs` 1. Every
  one is a refusal or failure the console showed as it stood. Each becomes an English message with a
  code (`docs/api.md`, 「Refusals」), which the screen shows in the language pack's words.
- **15 inside the test modules,** not carried: `change.rs` 3, `node/powershell.rs` 4, `zip.rs` 4,
  `garage.rs` 2, `mod.rs` 1, `shares.rs` 1.

No code here handles Korean text in itself, so the Korean ledger holds no line.

## Documents: 2 sources

| Old document | Here |
|---|---|
| `docs/files.md` (348 lines) | `docs/files.md`, restated for this service |
| `docs/shared-policy.md`, its bullet on the file viewer's protected locations | folded into `docs/files.md`, 「Protected locations」 |

`docs/upload.md` and `docs/room-files.md` describe the room's files, which are the receiver's.

## Service units

None: the old file viewer ran inside the chat server's process. `deploy/bmo-drive.service` is new.

## Tables

`web_file_shares` of the old archive (one row when it was listed) becomes the share table of this
repository's own store. No other table of the old archive is read: the node list comes from bmo-node.
