# Working in bmo-drive

## The three standing principles

These hold every bmo repository, and every change here (archive document #301, its principles):

1. **Each service runs on its own, with no context bleeding in from other repositories:** it builds,
   tests and runs from its own repository, data and settings, and never reads another repository's
   code, files, data folders or tables.
2. **Services talk to each other only through documented APIs** (HTTP routes, events, a documented
   command), never by importing another service's code.
3. **References run one way:** lower code uses the base, and the base never knows what is below it (no
   per-provider branch such as `if telegram`, no list of the units below, no dependency on any service);
   where two repositories must know the same policy value, the lowest service in the dependency
   direction owns it and the ones above read it through that service's API; and each repository holds
   only what it uses itself, since the same thing kept in several places is the wrong structure.

## The documents come first

- `docs/` is the canonical statement of what the service does; the code is one realisation of it.
  Before changing code, read the document covering that area: `docs/files.md` for the file viewer and
  its share links, `docs/api.md` for the routes, the command, the harness plugin, the calls and the
  settings, with `docs/openapi.json` generated from the code beside it.
- A change that would make a document wrong or incomplete repairs the document first, stating what is
  true now, and the code is written to match.
- A document states what is true now: no change history, no rationale told through an earlier design,
  no pasted source, signature or config fragment, and no file-and-line pins.

## Boundaries

- **Every other unit is reached through its API,** never through its code, its tables or its files.
  The routes this service serves and every call it makes to another unit are listed in `docs/api.md`.
- **A client of another service is one module named for that service** under `src/clients/`, holding
  only the calls this repository makes, so it can be swapped for that service's own client.
- **Dependencies:** crates from the registry pinned in `Cargo.toml`, and among other repositories only
  the base libraries `bmo-chat`, `bmo-chat-client`, `bmo-console-face` and `bmo-ui`.
- **The plumbing knows no route.** A route registers itself beside its own answer; adding one changes no
  file but its own.
- **No lookup table standing in for polymorphism.** A value that differs per place or per kind of
  machine belongs to that place or that machine.
- **No hand-written parser for a general format.** TOML, JSON, HTTP headers, command lines and their kin
  are read by a maintained library.

## Language

- Everything in this repository is English: code, comments, strings, tests, documents, commit messages
  and branch names. No Korean in code or tests, written plainly or as escapes; `korean-ledger.json`
  names none. The README's Korean, Japanese and Chinese translations at the root (`README.ko.md`, `README.ja.md`,
  `README.zh-CN.md`) are the one exception.
- What the service answers is an English sentence for a log and a kebab-case code a screen shows in its
  own language's words (`docs/api.md`, 「Refusals」). The screen's words stand in the console plugin
  bundle's own language pack, `client/i18n/`, one file to a locale; its Korean file is data a screen
  shows, not code.
- No personal data: no user names, home directories, machine names, internal addresses or names of any
  agent (`PERSONAL-DATA.md`). What the service needs at runtime comes from its settings, never from the
  code.

## Layout

- `Cargo.toml` is the only manifest at the root, one crate split into its own modules, and no directory
  is named after its parent.
- The console plugin bundle stands in `client/`, a project of its own.
- Tests live in `tests/`, one test target, `tests/main.rs`, each file of tests a module of it (bmo-chat's
  `docs/conventions.md`, 「Rust builds」); no test module stands in `src/`. The repository carries the
  behaviour tests standing so far (README.md); the testbed's `personal-data` check holds the personal-data rule.
  The bundle's browser tests are the bundle project's own and stand in `client/tests/`, run by its check
  script.

## The stages of a change

1. **Writing:** the only check is `cargo check --all-targets`, and for the bundle its typecheck.
2. **Review:** read the whole diff against the document governing it, then `cargo fmt --all --check`
   and `cargo clippy --all-targets -- -D warnings`.
3. **Verification:** the tests touched, then the whole suite once, output written to a file and the
   file read. A module's tests run by its name filter, `cargo test --test main <module>::`; the whole suite
   is `cargo test` with the bundle's check.
