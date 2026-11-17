# bmo-drive

The file viewer as one service with its own API: this machine's drives, its Garage and the nodes'
volumes walked as places by the administrator, each owner's own BMO drive with its quota, what stands there
read and changed, the share links that hand one file or folder to someone with no token, and the member
shares that let members of the reader's company read one. Beside the service the binary carries its command, `bmo-drive file share`,
its client and its harness plugin, the MCP server `bmo-drive mcp` with the `bmo-drive` skill. Every other
unit is reached through its API, as `docs/api.md` lists.

## Tests

The repository carries the repository-wide personal-data check in `tests/personal_data.rs`
(`PERSONAL-DATA.md`), the import of the old archive's share links in `tests/import.rs`, and the owners and
their two branches in `tests/owners.rs`: each owner's drive its own alone, the quota judged before bytes
arrive, the machines the administrator's alone, and share links opening in both branches; the console
face in `tests/console.rs`: the manifest the service serves read through bmo-ui's own reading on the release
`client/vendor/` holds, run with Bun by bmo-console-face, the face wired to the console's credential, and
the drive's own guard of every other route; links on a volume in
`tests/links.rs`: a path walking back into itself refused, a link to a folder beside walked once, each link's
target listed and a folder's zip ending over links; and the Files screen in `tests/screen.rs`, its modules run
with Bun the same way: where it opens, from the place memory, and the branch of what is shared with the owner,
each share a read-only place read through the member's own roads. `cargo test` runs them all. The bundle carries
its browser tests in `client/tests/`: the Files screen stands on its own in a small Nuxt host there, served by its
dev server with the bundle's own words and driven in Chromium against a stand-in of the service, and
`tests/browser/files-upload.test.ts` drops and pastes files into the open folder, pastes a picture the browser
names at the paste, pastes into the rename field and pastes entries copied on the screen. The bundle's check,
`bun run check` inside `client/` (its frozen install, its typecheck and its browser tests), stands beside them.
`docs/inventory.md` lists the tests the old behaviour was held to.

## Layout

| Folder | Holds |
|---|---|
| `src/` | The service, its command, its client and its MCP server, one crate split into its own modules |
| `migrations/` | The store, its tables made by these statements in order |
| `skills/` | The `bmo-drive` skill, the harness plugin's half beside the MCP server |
| `client/` | The console plugin bundle: the Files screen and the file share page, a project of its own, with its build in `client/console/` |
| `deploy/` | The service's user unit |
| `docs/` | The canonical statements of what the service does, and the OpenAPI document |
| `tests/` | The repository-wide personal-data check, `personal_data.rs`, and the behaviour tests standing so far |

At the root stand the Cargo manifest, `VERSION`, `testbed.toml`, `rustfmt.toml`, `CLAUDE.md`,
`PERSONAL-DATA.md`, `korean-ledger.json`, `settings.toml`, `settings.local.toml.example` (the example of
the local settings a deployment gives) and `LICENSE`.

## The console plugin bundle

`client/` holds the Files screen and its file share page on top of bmo-ui, a bundle of bmo-ui's console
plugin contract (bmo-ui's `docs/console-bundles.md`); what this service chooses within it is
`docs/api.md`, 「The console face」. Its words stand in its own pack, `client/i18n/`, one file to a locale,
under `drive.*`, with `drive.error.<code>` for each refusal code of `docs/api.md`.

- **The build.** `bun run bundle` builds `client/dist/` from the screens' sources on bmo-ui's bundle build,
  which leaves the libraries a console shares unbundled and writes the layer the bundle is built on: the
  module `bundle.js`, whose default export holds the two pages' routes, the stylesheet `bundle.css`, the
  utility classes the screens use on the layer's tokens, `words.json`, the language pack as one object keyed
  by locale, and `layer.json`. The components and the Vue and Nuxt functions the pages use without importing
  them are imported in the build, which stands in `client/console/`. A deployment builds the folder in each
  release, and the service answers its files with its manifest.
- **The layer.** `client/vendor/` holds bmo-ui's release file as bmo-ui's release verb wrote it, the one
  bmo-chat's `shared-versions.toml` pins; the bundle is installed, built and typechecked against it. The
  Nuxt configuration in `client/` serves the typecheck alone.
- **The types.** The screen's types of the service's answers are generated from `docs/openapi.json` into
  `client/shared/openapi.ts` by `bun run api`.

## Documents

| Document | States |
|---|---|
| `docs/files.md` | The file viewer: the machines, the places, the fence, reading, changing, protected locations, share links and member shares |
| `docs/api.md` | The routes, their answers and refusals, the events, the command, the harness plugin, the client, the calls to other units and the settings |
| `docs/openapi.json` | The same contract in machine form, generated by `bmo-drive openapi` |
| `docs/inventory.md` | What this repository takes from the old repository, with each set's count and source |
| `PERSONAL-DATA.md` | What may never stand in this repository: a person's name, address, path, device or network details, a room member's, a company's, or a line of an actual conversation, and the check that holds it |

## Version

`VERSION` holds the repository's version, the crate's own; `docs/api.md`, 「Versions」, says how it moves.

## License

MIT. The full text stands in `LICENSE`.
