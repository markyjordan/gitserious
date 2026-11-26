# Testing

This page is the reference for where gitserious tests live, who owns each fixture, and what a change must pass. It describes the workspace at `f6a082c`, the last code change of the test architecture stack (#468 to #482). [The test inventory](test-inventory.md) is the evidence record behind it, with one row for each test module and the measurements that led to this layout.

## Each crate tests the behavior it owns

A test lives in the crate whose code decides the result it asserts. No test reaches into another workspace crate's private code or test files. The four in-process tests in `crates/gitserious/tests/commit.rs` are the one known exception, described under [Executable tests](#executable-tests).

| Crate | Where its tests live | What they protect | Fixtures |
|---|---|---|---|
| `gitserious-core` | Inline `#[cfg(test)]` modules in `src/` for value types, and `tests/*.rs` for the public API | Identifiers, taxonomies, drafts, commit rendering, and provenance | None. Every test is pure. |
| `gitserious-app` | `src/config/library_draft/tests.rs` and inline modules in `src/config/` for private logic, and `tests/*.rs` for workflows | Use cases called through App's public functions and ports, with in-process fakes of the ports | `tests/support/mod.rs` holds one configurable `RepositoryLocator` fake. Every other fake stays in its test file. |
| `gitserious-fs` | Inline modules in `src/` for porcelain parsers, `src/platform/xdg.rs` and `src/platform/windows.rs` for path resolution, and `tests/*.rs` for adapter contracts | Each adapter against real `git` and a real filesystem, and App's services wired to these adapters in `tests/app_services.rs` | `tests/support/mod.rs` holds the hermetic Git fixture. Temporary directories come from `tempfile`. |
| `gitserious-cli` | Inline modules in `src/cli.rs` and `src/output.rs`, and `tests/invocation.rs` | Argument parsing, destinations, exit codes, and the stream each answer goes to | None |
| `gitserious-tui` | Inline modules and `*_tests.rs` files in the lib target, `src/root/tests/` for the root shell, and `src/config/model_check/` for the Config model check | State, rendering on a `ratatui` `TestBackend`, and key and mouse interaction | `src/config/test_support.rs`, `src/config/library_lookup.rs`, and `src/root/tests/support.rs` |
| `gitserious` | `tests/*.rs` | The built binary as a process, with its help output, exit codes, terminal routing, and the files that `init`, `config`, and `commit` write | `tests/support/mod.rs` for homes and Git, and `tests/support/terminal.rs` for pseudo terminals |

### TUI fixtures

Every TUI test compiles into the lib target, because the code under test is crate-private.

- `src/config/test_support.rs` holds the Config fixtures that three or more test files share. They are `Workspace`, `footer_text`, `create_taxonomy`, `key`, `team`, `fixture`, `press`, `select`, and the `Result` alias. Twelve files import it, and two of them are model check files.
- `src/config/library_lookup.rs` holds `LibraryLookup`, a test-only trait that finds a Library node by up to three names. It is `pub(super)` and compiled only under `#[cfg(test)]`.
- `src/root/tests/support.rs` holds the root shell's fake workspaces. Three root test files keep one more fake each, because only that file needs its behavior.
- `src/commit/author/tests.rs` owns the authoring tests and their helpers. `tests/picker.rs` and `tests/citation_picker.rs` hold the two pickers' tests. `composer/tests.rs` imports helpers from `author/tests.rs`, the one place where a test file serves another.

### Executable tests

Every test in `crates/gitserious/tests/` that spawns the built binary does so through `support::Home`. `Home` gives every command its own `HOME` and XDG directories and a hermetic Git environment, so the binary never reads the developer's Library or Git settings. `home.rs` copies the binary under another name and spawns the copy with `Home::command_for`. The Unix-only `terminal_help.rs` drives the binary in a pseudo terminal with `portable-pty` and `vt100`.

`tests/help.rs` includes `src/help.rs` with `#[path]`, so the help tests run the production `help_pages`. The binary crate has no lib target, so an integration test cannot import `help_pages` by its module path.

Four tests in `tests/commit.rs` call App's commit services with FS adapters in process instead of spawning the binary. Three of them run the binary's `init` to set up the repository. They are a known exception, and the inventory records them as deferred.

## Fixture ownership

- A fixture moves into a shared module only when three or more test files in one crate would each define the same fixture. Two copies stay in their files.
- A shared test module holds only fixtures that at least two test files use. A fixture with one consumer lives in that consumer. At `f6a082c`, `with_stale_fingerprint` and `assert_project_shape` in `crates/gitserious/tests/support/mod.rs` break this rule, and [the inventory's open items](test-inventory.md#open-items) list them.
- Fixtures that change for different reasons stay apart, even when they look alike. App's store fakes and `FakeError` each record, fail, or count in their own way. The locator in `tests/init.rs` records its calls, and the one in `tests/configuration.rs` is never called.
- No workspace crate exists for test support. Each crate's fixtures live in that crate.
- No crate exports a test-only public item or declares a Cargo feature for tests. Inside a library, test-only items use `#[cfg(test)]` and `pub(super)` or `pub(crate)` visibility.
- No workspace crate declares a development dependency on another workspace crate. `scripts/ci/check-crate-dependencies.py` rejects any workspace edge outside its `PERMITTED` table, a self edge included. It does not reject a development dependency along an edge that the table lists, such as FS on App, so review enforces that case.
- No source file compiles a file from another crate with `#[path]` or `include!`. The same script rejects that inclusion.

The FS fixture in `crates/gitserious-fs/tests/support/mod.rs` and the executable fixture in `crates/gitserious/tests/support/mod.rs` are two copies on purpose. An integration test cannot import another crate's `tests/` directory, and sharing would need a test-support crate, a public test-only API, or a cross-crate include, which the rules above forbid. The copies also differ. The executable's `repository()` creates its directory with a space in the name and keeps Git's default branch, its `git` reports stdout as well as stderr, and only it has `Home`.

## Unit and integration tests

- A test that needs a private item is a unit test in a `#[cfg(test)] mod tests` inside the module it tests, or in a sibling `*_tests.rs` or `tests.rs` file that module declares.
- A test that needs only the public API is an integration test in the crate's `tests/` directory. `crates/gitserious-fs/tests/directory.rs` is an example.
- No `tests/` file compiles into a lib target. `src/` never includes a file from `tests/`.
- A test that runs the binary belongs to `crates/gitserious/tests/`. A test that wires App's services to FS adapters in one process belongs to `crates/gitserious-fs/tests/`.
- A test that writes files uses a `tempfile` directory. A test that runs Git uses its crate's hermetic Git fixture.
- A production module may expose a `#[cfg(test)]` accessor for its tests. The Config editor's `observe` in `src/config/taxonomies/mod.rs` is the shared view of editor state that ten Config test files and the model check read.

## Hermetic Git

Both Git fixtures apply the same rules to the Git commands that tests run themselves.

- Each command sets `GIT_CONFIG_GLOBAL` and `GIT_CONFIG_SYSTEM` to `/dev/null` and removes `GIT_DIR`, `GIT_WORK_TREE`, and `GIT_INDEX_FILE`.
- Each repository sets a fixed `user.name` and `user.email`, `commit.gpgsign=false`, and `core.hooksPath=.git/hooks` in its local configuration.
- The repository constructor refuses to create a repository while `GIT_DIR`, `GIT_WORK_TREE`, or `GIT_INDEX_FILE` is set. FS's `init` and the executable's `repository` return an error that names the variable.

The refusal exists because FS adapters that a test calls in process do not get the same isolation. They run Git with the test process's environment. That covers the FS tests of Git adapters, `crates/gitserious-fs/tests/app_services.rs`, and the four in-process tests in `crates/gitserious/tests/commit.rs`. Clearing a variable from the process environment needs `unsafe` code, which `unsafe_code = "forbid"` in the workspace manifest rules out. A binary that a test spawns through `Home` gets the isolated environment.

The repository's local settings outrank the developer's global and system settings, so the fixed identity, signing, and hooks also reach the adapters. A leaked `GIT_DIR` still reaches them. Under a leaked `GIT_DIR`, as under `git rebase --exec`, a test that builds its repository through the fixture stops at the refusal instead of writing into the repository that `GIT_DIR` names. Five tests in `crates/gitserious-fs/tests/app_services.rs` start from a plain directory, so they fail at an assertion instead, as #478 records.

Whether FS adapters should honor an inherited `GIT_DIR` is an open product decision. The fixtures do not decide it.

The fixtures do not clear `GIT_CONFIG_PARAMETERS` or `GIT_CONFIG_COUNT`. A value exported in either variable still reaches every Git command that the tests and the adapters run.

## The Config model check

`crates/gitserious-tui/src/config/model_check/` drives the real Config `State` through every key sequence from 17 seeds, up to each seed's depth. It checks 14 properties on every transition and each seed's reach goals after its exploration.

| File | Lines | Contents |
|---|---|---|
| `mod.rs` | 36 | The module docs, which say what the check covers and what it leaves to interaction tests, and the one test, `every_bounded_key_sequence_keeps_the_config_editor_properties` |
| `engine.rs` | 258 | The key alphabet in `INPUTS` and `NAMED_ONLY`. `replay` rebuilds the workspace and replays every key. `replay_all` runs replays on one worker thread per logical CPU. `start`, `advance`, and `input_indices` reach a seed's start, and `explore` runs the breadth-first search and deduplicates states by their `Debug` text. |
| `seeds.rs` | 410 | `Seed`, `Start`, `StartMode`, `Alphabet`, `Reach`, `COPY_KEYS`, the 17 `SEEDS`, and the seed workspaces built on `test_support::Workspace` |
| `snapshot.rs` | 127 | `Snapshot`, which captures the screen, the `observe` view, the Library draft, the stored Library, dialogs, and the frame text, and the start predicates `at_outline`, `selects`, and `editing` |
| `properties.rs` | 747 | The `Check` type, the `PROPERTIES` table, and each property with its helpers |
| `diagnostics.rs` | 78 | `check`, which reports the seed, the keys to its start, the keys after it, and the screens before and after a failing key, and `check_reaches`, which reports each reach goal that no explored state met |

Run the check alone with its coverage lines:

	cargo test -p gitserious-tui every_bounded_key_sequence -- --nocapture

The test prints one line per seed on stderr with its depth, states, transitions, and time. The check takes most of the TUI lib target's run time. The `gitserious-verify-config-editor` agent skill, which lives outside this repository, says which Config editor changes need this check and how to read a failure.

## Verification

`scripts/ci/run-quality.sh` runs one component of `scripts/ci/run-rust-quality.sh`. CI runs `check` and `test` on Ubuntu, macOS, and Windows.

| Component | Commands |
|---|---|
| `fmt` | `cargo fmt --all --check` |
| `check` | `cargo metadata --locked --no-deps`, then `cargo check --locked --workspace --all-targets --all-features`. Outside Windows it then runs `scripts/ci/check-compiled-sources.sh` and `scripts/ci/check-crate-dependencies.py`. On Linux only, it runs `scripts/ci/check-dead-code.sh`. |
| `lint` | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` |
| `test` | `cargo test --locked --workspace --all-targets --all-features`, then `cargo test --locked --workspace --doc` |
| `release` | `cargo build --locked --workspace --all-targets --all-features --release`, then `cargo package --locked --workspace --list` |

`check` skips the dead-code scan on macOS and Windows, so a local pass on a Mac does not cover it. `check-dead-code.sh` looks for `allow(dead_code)` only under `crates/*/src`. The `#![allow(dead_code)]` at the top of the FS and executable `tests/support/mod.rs` files therefore hides an unused fixture from every check.

A change that moves, adds, or removes tests reports the test list before and after. On macOS at `f6a082c`, `cargo test --workspace -- --list` lists 605 tests. No crate has a doc test.

| Crate | Unit | Integration | Total |
|---|---|---|---|
| `gitserious-core` | 26 | 32 | 58 |
| `gitserious-app` | 28 | 78 | 106 |
| `gitserious-fs` | 18 | 76 | 94 |
| `gitserious-cli` | 8 | 3 | 11 |
| `gitserious-tui` | 285 | 0 | 285 |
| `gitserious` | 0 | 51 | 51 |
| Total | 365 | 240 | 605 |

Two tests do not run on macOS. `preserves_non_utf8_unix_worktree_paths` in `crates/gitserious-fs/tests/git_repository.rs` runs only on Linux, and `native_windows_adapter_returns_absolute_known_folder_paths` in `crates/gitserious-fs/src/platform/windows.rs` runs only on Windows. Windows also skips every Unix-only test. The Linux and Windows totals were not measured.
