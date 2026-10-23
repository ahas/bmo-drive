---
name: bmo-drive
description: Walk, read, write and share files with the bmo-drive MCP server. Use when a task needs the files of a machine's volumes, its S3-compatible store or another machine's volumes through the drive service rather than a local shell, when a file has to be handed to someone as a download link, or when building or checking a caller of the drive service.
---

# bmo-drive

The drive service walks files across places: the volumes of the machine it stands on, that machine's
S3-compatible store and the volumes of the other machines its node list holds, which are the
administrator's alone, and each owner's own BMO drive. The `bmo-drive` MCP server
reaches the service through its own client, one tool for each route a harness needs.

## Starting the server

Register the server with the harness as a stdio MCP server:

```
bmo-drive mcp
```

It reaches the service at the settings' `server.listen` with the settings' token. `--address <url>` names
another service; the token is still the settings' (`BMO_DRIVE_SERVER__TOKEN` sets it from the environment).
It acts for the administrator's owner key unless `--owner <key>` or `BMO_DRIVE_OWNER` names another; any
other owner reaches its own drive alone.

## Places first

Every tool but `drive_places` and `drive_share` names a place by its `placeName`, so start with
`drive_places`. It lists the owner's BMO drive (`drive`), then, for the administrator, this machine's
volumes (`volume:<mount>`) and its store (`garage`); with `nodeName`
it lists that node's volumes (`node:<nodeName>:<mount>`) instead, reading the node over the network. A place
whose `reason` is set cannot be opened now; its `reasonCode` says why.

## Walking and reading

- `drive_entries` lists the folder at `path` (the place's top where none is given) a page at a time:
  `after` is how many entries were already read, `limit` how many to read, and `totalCount` how many the
  folder holds. `sort` (`name`, `byteCount`, `changedAt`), `order` and `folders` shape the listing.
- A path is names joined by `/` from the place's top, never with `.` or `..`.
- `drive_read` answers a file's bytes in base64, at most 4 MiB from `start`. Read a longer file a part at
  a time, moving `start` on by `byteCount` until it reaches `fileByteCount`.

## Changing

- `drive_write` writes a whole file from base64 `content`, replacing what stands there; `conflict` set to
  `refuse`, `rename` or `skip` asks otherwise.
- `drive_folder` makes a folder, `drive_copy` copies entries into a folder of the same place or of the place
  `toPlaceName` names, `drive_move` renames or moves an entry inside its place, and `drive_delete` deletes
  an entry, a folder with everything in it. To move between places, copy and then delete what was copied.
- A name already taken is refused with `conflict` and the colliding `names` unless `conflict` says to
  `replace`, `rename` (keep both) or `skip`.
- Some locations are protected and refuse every change with `protected`; reading them is untouched.
- A write into the BMO drive of an owner with a quota is refused with `quota-exceeded` (413) where it would
  take the drive past its quota; the refusal carries `usedBytes` and `quotaBytes`.

## Sharing

`drive_share` makes a link to one file or folder, until `expiresAt` (epoch milliseconds) or with no end
where none is given; asking again for the same entry hands back the link that stands. Hand the person the
link's `url`: it opens a page with a download, a folder downloading as its zip.

## Refusals

A refused call comes back as a tool error whose text is the service's `status`, `code` and `error`. The
codes are the service's (`docs/api.md`, 「Refusals」): `not-found`, `conflict`, `bad-request`,
`upstream-failed` for a node or the store that did not answer, and the service's own rule codes such as
`outside-place`, `place-top`, `is-folder`, `protected`, `quota-exceeded` and `storage-unready`.
