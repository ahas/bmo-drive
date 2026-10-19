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
| `deploy/` | The service's user unit |
| `docs/` | The canonical statements of what the service does |

At the root stand the Cargo manifest, `VERSION`, `testbed.toml`, `rustfmt.toml`, `CLAUDE.md`,
`korean-ledger.json`, `settings.toml` and `settings.local.toml.example`, the example of the local
settings a deployment gives.

## Documents

| Document | States |
|---|---|
| `docs/files.md` | The file viewer: the machines, the places, the fence, reading, changing, protected locations and share links |
| `docs/api.md` | The routes, their answers and refusals, the command, the calls to other units, the settings and what the console does |
| `docs/inventory.md` | What this repository takes from the old repository, with each set's count and source |

## Version

`VERSION` holds the repository's version in semver, the crate's own. `docs/api.md`, 「Versions」, says
which change moves which part.
