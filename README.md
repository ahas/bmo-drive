# bmo-drive

The file viewer as one service with its own API: the console's Files screen walking this machine's
drives, its Garage and the nodes' volumes, reading and changing what stands there, and the share links
that hand one file or folder to someone with no login. Its own command, `bmo-drive file share`, makes
those links from a shell. Every other unit is reached through its API, as `docs/api.md` lists.

## Tests are deferred

No test is carried, written or run in this repository yet. The tests are written and run here once the
minimal services stand on the new repositories; until then the only check is the build check, `cargo
check --all-targets` and the bundle's typecheck. `docs/inventory.md` lists the tests the old behaviour
was held to.

## Layout

| Folder | Holds |
|---|---|
| `src/` | The service and its command, one crate split into its own modules |
| `migrations/` | The share links' store, its table made by these statements in order |
| `client/` | The console plugin bundle: the Files screen and the file share page, a project of its own |
| `deploy/` | The service's user unit |
| `docs/` | The canonical statements of what the service does |

At the root stand the Cargo manifest, `VERSION`, `testbed.toml`, `rustfmt.toml`, `CLAUDE.md`,
`korean-ledger.json`, `settings.toml` and `settings.local.toml.example`, the example of the local
settings a deployment gives.

## The console plugin bundle

`client/` holds the console's Files screen and its file share page as a Nuxt layer on top of bmo-ui,
packed as `bmo-drive-client`. A host console extends it as it extends bmo-ui, and owes it:

- bmo-ui itself, at the version the bundle's peer names; for the bundle's own install and typecheck a
  packed copy stands in `client/vendor/`;
- its i18n module and a language pack answering the keys the screen reads: the `files`, `common` and
  `screens` keys, and `files.error.<code>` for each refusal code of `docs/api.md`;
- the title-bar pieces `DeployMark`, `TodoOpenButton` and `UsageMeters`, declared in `client/host/` by
  the props the screen hands them;
- the routes under `/api/files/` and `/api/share/files/` passed on to this service (`docs/api.md`, 「What
  the console does」).

The screen calls only those routes, and says a refusal by its code in the pack's words. The bundle's
check is `bun install --frozen-lockfile && bun run typecheck` inside `client/`.

## Documents

| Document | States |
|---|---|
| `docs/files.md` | The file viewer: the machines, the places, the fence, reading, changing, protected locations and share links |
| `docs/api.md` | The routes, their answers and refusals, the command, the calls to other units, the settings and what the console does |
| `docs/inventory.md` | What this repository takes from the old repository, with each set's count and source |

## Version

`VERSION` holds the repository's version in semver, the crate's own. `docs/api.md`, 「Versions」, says
which change moves which part.
