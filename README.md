# bmo-drive

The file viewer as one service with its own API: this machine's drives, its Garage and the nodes'
volumes walked as places by the administrator, each owner's own BMO drive with its quota, what stands there
read and changed, and the share links that hand one file or folder to someone with no token. Beside the service the binary carries its command, `bmo-drive file share`,
its client and its harness plugin, the MCP server `bmo-drive mcp` with the `bmo-drive` skill. Every other
unit is reached through its API, as `docs/api.md` lists.

## Tests

The repository carries the repository-wide personal-data check in `tests/personal_data.rs`
(`PERSONAL-DATA.md`), the import of the old archive's share links in `tests/import.rs`, and the owners and
their two branches in `tests/owners.rs`: each owner's drive its own alone, the quota judged before bytes
arrive, the machines the administrator's alone, and share links opening in both branches; and the console
bundle in `tests/console.rs`: the manifest read through bmo-ui's own reading of it, the guard's refusals,
and the module's path following its bytes. That one runs bmo-ui's reading with Bun inside `client/`,
installing the bundle's locked packages first where they are not installed. `cargo test` runs them all.
The rest of the behaviour tests are deferred: they are written and run here once the minimal services stand
on the new repositories, and until then the build check, `cargo check --all-targets`, and the bundle's
typecheck stand beside them. `docs/inventory.md` lists the tests the old behaviour was held to.

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
| `.graph/` | The code graph: its settings and its nodes, one feature to a node. The area files are generated in the checkout and untracked, so a fresh checkout builds them again, while `settings.toml` is the repository's configuration and stays tracked |

At the root stand the Cargo manifest, `VERSION`, `testbed.toml`, `rustfmt.toml`, `CLAUDE.md`,
`PERSONAL-DATA.md`, `korean-ledger.json`, `settings.toml`, `settings.local.toml.example` (the example of
the local settings a deployment gives) and `LICENSE`.

## The console plugin bundle

`client/` holds the Files screen and its file share page on top of bmo-ui, by bmo-ui's console plugin
contract (bmo-ui's `docs/console-bundles.md`): the bundle `drive`, its screens under `/files`, calling the
service `drive` alone, the node list among what it reads there. Its words stand in its own pack,
`client/i18n/`, one file to a locale, under `drive.*`, with `drive.error.<code>` for each refusal code of
`docs/api.md`. Its sources are one set, built two ways:

- **The served bundle.** `bun run bundle` builds `client/dist/`: the module `bundle.js`, whose default
  export holds the two pages' routes, the stylesheet `bundle.css`, the utility classes the screens use
  built on the layer's tokens, and `words.json`, the language pack as one object keyed by locale. The
  module leaves every import of `vue`, `vue-router`, `vue-i18n`, `nuxt/app`, `@nuxt/ui` and `bmo-ui`
  unbundled, written as the package's own path, and carries the rest; the components and the Vue and Nuxt
  functions the pages use without importing them are imported in the build. The build stands in
  `client/console/`. The service answers the folder's files with its manifest (`docs/api.md`, 「The console
  bundle」), and a deployment builds the folder in each release.
- **The layer pack.** `bun pm pack` inside `client/` packs `bmo-drive-client`, the same pages as a Nuxt
  layer, for a console that builds its bundles into itself; it finds the bundle by
  `client/console-bundle.json` in the pack, which names it, its screen path, its service and its language
  pack. Such a console extends it as it extends bmo-ui and relays the service `drive`.

Either way the console owes the bundle bmo-ui at the version the bundle's peer names; a packed copy stands
in `client/vendor/` for the bundle's own install, build and typecheck. The screen's types of the service's
answers are generated from `docs/openapi.json` into `client/shared/openapi.ts` by `bun run api`. The
bundle's check is `bun install --frozen-lockfile && bun run typecheck` inside `client/`.

## Documents

| Document | States |
|---|---|
| `docs/files.md` | The file viewer: the machines, the places, the fence, reading, changing, protected locations and share links |
| `docs/api.md` | The routes, their answers and refusals, the events, the command, the harness plugin, the client, the calls to other units and the settings |
| `docs/openapi.json` | The same contract in machine form, generated by `bmo-drive openapi` |
| `docs/inventory.md` | What this repository takes from the old repository, with each set's count and source |
| `PERSONAL-DATA.md` | What may never stand in this repository: a person's name, address, path, device or network details, a room member's, a company's, or a line of an actual conversation, and the check that holds it |

## Version

`VERSION` holds the repository's version, the crate's own; `docs/api.md`, 「Versions」, says how it moves.

## License

MIT. The full text stands in `LICENSE`.
