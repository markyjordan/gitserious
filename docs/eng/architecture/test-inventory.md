# Test inventory

This inventory is the evidence record of the test architecture stack, #468 to #482. It classified every Rust test in the workspace at `5740240` so that the stack could move each test into the crate that owns its behavior. It now records that baseline, one inventory row for each module or file group at `f6a082c`, the last code change of the stack, the findings and their outcomes, and the items the stack left open. [Testing](testing.md) is the reference for the layout and conventions that resulted.

[Application interfaces and consumers](application-consumers.md) records an older source baseline, `b8c03d1` from 2026-11-09, 142 commits before `5740240`. Its export index and use-case notes are not rebaselined and are not authoritative for this work. Its test ownership table describes `f6a082c`. [The workspace dependency baseline](workspace-baseline.md) also records `b8c03d1`. At `5740240`, FS, TUI, and App itself declared a development dependency on App for the `test-support` feature. #469, #470, and #471 removed all three, so at `f6a082c` no workspace member declares a development dependency on another member.

## The baseline passed with 607 tests on macOS

All four components of `scripts/ci/run-quality.sh` passed on the host below. No test failed and none was ignored, so the stack started with no known failure.

| Item | Value |
|---|---|
| Commit | `5740240` on `marky/tests`, clean tree |
| Host | macOS 26.7.1 (25G241), x86_64, Intel Core i7-9750H, 12 logical CPUs |
| Toolchain | `rustc 1.96.0 (ac68faa20 2026-05-25)` and `cargo 1.96.0`, pinned by `rust-toolchain.toml` |

| Component | Result | Wall time | Notes |
|---|---|---|---|
| `fmt` | pass | 1 s | `cargo fmt --all --check` |
| `check` | pass | 18 s | Every Rust source compiles, and the crate dependency check passes. The script skips the dead-code scan on macOS and runs it on the Linux CI leg, so this baseline does not cover it. |
| `lint` | pass | 11 s | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` |
| `test` | pass | 187 s | 42 s of it compiles. The TUI lib target takes 95 s of the rest. |

The repository has no Markdown format or link check in `scripts/` or `.github/workflows/`. A one-off script outside the repository checks this file and the other architecture pages for broken relative links and anchors.

## The stack ends with 605 tests

At `f6a082c`, on the same host and toolchain with a clean tree, `fmt`, `check`, `lint`, and `test` pass. No test fails and none is ignored. `test` took 128 s of wall time with the build already cached, and the TUI lib target took 93.74 s of it. `check` again skipped the dead-code scan, which runs only on Linux.

The workspace lost 2 tests. #481 deleted 2 redundant CLI tests. #478 moved 9 tests from the executable to FS, so FS gained 9 and the executable lost 9.

### Counts per crate

The measured count is what `cargo test -- --list` lists on this host. The static count is the number of `#[test]` attributes in the source. Two tests are gated to another platform. `preserves_non_utf8_unix_worktree_paths` in `crates/gitserious-fs/tests/git_repository.rs` runs only on Linux. `native_windows_adapter_returns_absolute_known_folder_paths` in `crates/gitserious-fs/src/platform/windows.rs` runs only on Windows. Every lib crate builds a doc-test target, and none has a doc test.

| Crate | Unit | Integration | Doc | Measured | Static | Measured at `5740240` |
|---|---|---|---|---|---|---|
| `gitserious-core` | 26 | 32 | 0 | 58 | 58 | 58 |
| `gitserious-app` | 28 | 78 | 0 | 106 | 106 | 106 |
| `gitserious-fs` | 18 | 76 | 0 | 94 | 96 | 85 |
| `gitserious-cli` | 8 | 3 | 0 | 11 | 11 | 13 |
| `gitserious-tui` | 285 | 0 | 0 | 285 | 285 | 285 |
| `gitserious` | 0 | 51 | 0 | 51 | 51 | 60 |
| Total | 365 | 240 | 0 | 605 | 607 | 607 |

The 18 FS unit tests are 6 parser tests in `src/` and 12 path resolution tests in `src/platform/`. At `5740240`, the FS lib held 23 unit tests, because `src/lib.rs` compiled `tests/unit/` into it. #473 moved 5 of those to the `tests/directory.rs` integration target. The executable's `src/main.rs` target has no tests.

### Counts and harness time per target

Harness time is the `finished in` value from the `test` run at `f6a082c`. It excludes compilation.

| Crate | Target | Tests | Harness time |
|---|---|---|---|
| `gitserious-core` | lib | 26 | 0.03 s |
| `gitserious-core` | `tests/commit.rs` | 12 | 0.00 s |
| `gitserious-core` | `tests/commit_provenance.rs` | 6 | 0.00 s |
| `gitserious-core` | `tests/domain_bundles.rs` | 3 | 0.00 s |
| `gitserious-core` | `tests/draft_responses.rs` | 6 | 0.00 s |
| `gitserious-core` | `tests/taxonomy_configuration.rs` | 5 | 0.00 s |
| `gitserious-app` | lib | 28 | 0.02 s |
| `gitserious-app` | `tests/commit.rs` | 2 | 0.00 s |
| `gitserious-app` | `tests/commit_services.rs` | 28 | 0.01 s |
| `gitserious-app` | `tests/configuration.rs` | 24 | 0.01 s |
| `gitserious-app` | `tests/global_storage.rs` | 3 | 0.00 s |
| `gitserious-app` | `tests/init.rs` | 6 | 0.00 s |
| `gitserious-app` | `tests/project_configuration.rs` | 1 | 0.00 s |
| `gitserious-app` | `tests/project_services.rs` | 12 | 0.00 s |
| `gitserious-app` | `tests/taxonomy_policy_v1.rs` | 2 | 0.00 s |
| `gitserious-fs` | lib | 18 | 0.00 s |
| `gitserious-fs` | `tests/app_services.rs` | 9 | 1.99 s |
| `gitserious-fs` | `tests/commit.rs` | 6 | 1.56 s |
| `gitserious-fs` | `tests/directory.rs` | 5 | 0.00 s |
| `gitserious-fs` | `tests/git_draft.rs` | 2 | 0.46 s |
| `gitserious-fs` | `tests/git_editor.rs` | 1 | 0.00 s |
| `gitserious-fs` | `tests/git_index.rs` | 4 | 0.62 s |
| `gitserious-fs` | `tests/git_repository.rs` | 6 | 0.58 s |
| `gitserious-fs` | `tests/git_status.rs` | 5 | 2.57 s |
| `gitserious-fs` | `tests/git_worktree.rs` | 3 | 10.21 s |
| `gitserious-fs` | `tests/global_configuration.rs` | 13 | 0.18 s |
| `gitserious-fs` | `tests/project_state.rs` | 8 | 0.80 s |
| `gitserious-fs` | `tests/project_store.rs` | 10 | 0.37 s |
| `gitserious-fs` | `tests/taxonomy_storage_v1.rs` | 4 | 0.17 s |
| `gitserious-cli` | lib | 8 | 0.01 s |
| `gitserious-cli` | `tests/invocation.rs` | 3 | 0.01 s |
| `gitserious-tui` | lib | 285 | 93.74 s |
| `gitserious` | bin `src/main.rs` | 0 | 0.00 s |
| `gitserious` | `tests/commit.rs` | 7 | 2.26 s |
| `gitserious` | `tests/config.rs` | 4 | 0.61 s |
| `gitserious` | `tests/help.rs` | 15 | 0.28 s |
| `gitserious` | `tests/home.rs` | 4 | 0.50 s |
| `gitserious` | `tests/init.rs` | 10 | 0.88 s |
| `gitserious` | `tests/terminal_help.rs` | 11 | 3.61 s |

`lists_the_newest_commits_and_stops_after_two_hundred` makes `git_worktree.rs` the slowest FS target, because it creates 201 commits, one `git commit` subprocess each. That cause is inferred from the test body and was not profiled.

### Reproduce the counts

Run these commands from the repository root:

	cargo test --locked --workspace --all-targets --all-features -- --list
	cargo test --locked --workspace --doc -- --list

At `f6a082c`, the first command lists 605 lines that end in `: test`, and the second lists none. `cargo test --workspace -- --list` lists the same 605. A throwaway script outside the repository placed each listed test in its source file, compared the counts with the static `#[test]` counts, and checked that every test matches exactly one inventory row below. Every crate's row sum equals its measured total. Two seeded mistakes made the check fail, as expected. Dropping a row failed the TUI sum, and giving one file to two rows failed the Core sum.

## Inventory

Each row covers one module or file group at `f6a082c`. Locations are relative to the crate. Category says which Cargo target builds the tests and what kind of test they are. Isolation is `pure` when a test touches no shared state. It is `in-process` when the test uses fakes, `RefCell` state, or the `ratatui` `TestBackend`. It is `filesystem` when the test writes temporary files and `subprocess` when it runs `git` or the binary. Stack outcome says what the stack did to the row and which pull request did it. Coverage is `preserved` when the row protects its contract with no known gap and `missing` when a gap below applies.

### gitserious-core

| Location | Tests | Category | Contract protected | Dependencies | Isolation | Stack outcome | Coverage |
|---|---|---|---|---|---|---|---|
| `src/built_in.rs`, `src/configured_taxonomy.rs` | 4 | unit | The built-in catalog matches the version 1 contract. It is ordered, unique, and excludes `security`. The built-in taxonomies come in the order `conventional`, `research`, `infra-ops`. Snapshot lineage is informational. | none | pure | kept | preserved |
| `src/commit_type.rs`, `src/property.rs`, `src/schema_version.rs`, `src/taxonomy_provenance.rs` | 13 | unit | Value types reject blank descriptions, duplicate property keys, empty or whitespace values, and schema version 0. Provenance renders only taxonomy and schema trailers. | none | pure | kept | preserved |
| `src/identifier.rs` | 9 | unit, with one exhaustive enumeration | Identifiers accept lowercase ASCII kebab case and reject other shapes with the rule they break. Taxonomy identifiers reserve the version suffix. Labels format to identifiers, and one test checks every label over a small alphabet. | none | pure | kept | preserved |
| `tests/commit.rs` | 12 | integration | Drafts enforce header limits and reject duplicate keys. Rendering follows schema order, wraps only at grapheme boundaries, and adds the breaking-change `!` and footer. Validation reports unknown and missing properties with quoted names. | `unicode-width` | pure | kept | preserved |
| `tests/commit_provenance.rs` | 6 | integration | Provenance trailers keep qualified identities, versions, and order without changing body bytes. Malformed or injectable fingerprints are rejected. | none | pure | kept | preserved |
| `tests/domain_bundles.rs`, `tests/taxonomy_configuration.rs` | 8 | integration | The `research` and `infra-ops` schemas keep their agreed requirements and leave `conventional` unchanged. Custom taxonomies keep identity and order, reject empty or duplicate types, and treat conditional contradictions as blocking. | none | pure | kept | preserved |
| `tests/draft_responses.rs` | 6 | integration | Explicit responses survive cloning, and each invalid applicability state blocks. Empty responses and legacy drafts keep distinct contracts. Duplicate or unknown valueless responses are rejected. | none | pure | kept | preserved |

### gitserious-app

| Location | Tests | Category | Contract protected | Dependencies | Isolation | Stack outcome | Coverage |
|---|---|---|---|---|---|---|---|
| `src/config/library_draft/tests.rs` | 22 | unit | `LibraryDraft` keeps node ids across rebuilds and builds back unchanged. It refuses edits to built-ins, stored names, and fields past their limits. Forks and copies keep lineage, moves stop at either end, problems name every failing field, and Library dates follow saved content. | a private `find_path` that walks up to three names | pure | kept. The feature's `LibraryLookup` became a private helper (#471). | preserved |
| `src/config/taxonomy_date.rs`, `src/config/taxonomy_policy.rs` | 6 | unit | Dates round trip as a calendar day or a UTC timestamp. Fingerprints stay stable without examples and ignore release metadata on custom records. Defaults list every built-in in order. | none | pure | kept | preserved |
| `tests/commit.rs`, `tests/commit_services.rs` | 30 | integration | `CommitServices` writes one reviewed message, and only from a staged, conflict-free index. It rechecks policy and provenance after approval, stops during a merge, cherry-pick, or revert, and reports a writer failure after one attempt. `CommitAuthoringContext` rejects ambiguous initial selections. | 11 fakes of 8 ports defined in the files, and `support::Locator` | in-process | kept, on the shared locator fake (#472) | preserved |
| `tests/configuration.rs`, `tests/taxonomy_policy_v1.rs` | 26 | integration | `GlobalConfiguration` rejects shadowed, duplicate, and dangling taxonomies. Library sessions keep custom taxonomies unversioned, refuse apply while problems exist, save by compare-and-swap, and keep the draft after a store failure. | `FakeGlobalStore`, `NoRepository`, `NoProject`, a private `find_path` in `configuration.rs`, and a private `taxonomy` lookup in `taxonomy_policy_v1.rs` | in-process | kept, with local lookups (#471). Folding `taxonomy_policy_v1.rs` into `configuration.rs` is deferred. | preserved. Overlap with `library_draft/tests.rs` on forks was not checked. |
| `tests/init.rs` | 6 | integration | `initialize_project` creates an absent project once with complete definitions, never rewrites an existing one, and keeps locator and store failures distinct. One test checks that the fingerprint changes for every semantic dimension. | `FakeLocator`, `FakeStore` | in-process | kept. Moving the fingerprint test beside the other fingerprint tests is deferred. | preserved |
| `tests/project_configuration.rs`, `tests/project_services.rs` | 13 | integration | `ProjectServices` reports missing, broken, uninitialized, unreadable, and initialized projects. Only an untracked policy file at the root counts as local. Initialization creates Git only when no repository exists and refuses to overwrite a project. A failed project load attempts no write. | `support::Locator`, `Store`, `Status`, `Initializer`, `Project`, `BrokenLibrary` | in-process | kept, on the shared locator fake (#472) | preserved |
| `tests/global_storage.rs` | 3 | integration | `resolve_global_paths` returns the adapter's snapshot, calls the adapter once, and keeps its error. | `RecordingResolver` | pure | kept | preserved |

### gitserious-fs

| Location | Tests | Category | Contract protected | Dependencies | Isolation | Stack outcome | Coverage |
|---|---|---|---|---|---|---|---|
| `src/git.rs`, `src/git_status.rs`, `src/git_worktree.rs` | 6 | unit | Parsers name the repository Git refuses for dubious ownership. They read partial staging, renames, conflicts, unborn and detached heads, and Unix non-UTF-8 paths from porcelain output, and they read commit rows and parent directories. | none | pure | kept | preserved |
| `src/platform/xdg.rs`, `src/platform/windows.rs` | 12, plus 1 that runs only on Windows | unit | Windows paths live under the roaming application data root, which must be absolute. XDG resolution uses `XDG_CONFIG_HOME` or falls back under `HOME`. It ignores empty and relative values, reports a missing or relative `HOME`, keeps non-Unicode paths, and creates nothing. | `FakeEnvironment` and the private `resolve_from` and `Environment` in `xdg.rs`, the private `resolve_from_root` in `windows.rs`, `tempfile` | pure, except one filesystem test. `system_adapter_uses_xdg_on_unix` reads the process `HOME`. | moved from `tests/unit/global.rs` into the platform modules (#473) | preserved |
| `tests/directory.rs` | 5 | integration | `LocalDirectoryCreator` creates only the config directory, recursively and idempotently. It reports a file collision, requests mode 0700 for new directories, and leaves existing modes unchanged. | `tempfile` | filesystem | moved from `tests/unit/directory.rs`, with `TestDirectory` replaced by `tempfile` (#473) | preserved |
| `tests/commit.rs` | 6 | integration | `GitCommitWriter` commits the exact reviewed message and only the staged index. It names the commit it created, runs the `pre-commit` and `commit-msg` hooks, keeps Git's diagnostics, and supports paths with spaces. | real `git` through `tests/support`, `tempfile`, hook scripts | subprocess | kept, on the hermetic Git fixture (#474) | preserved. #474 closed gap G7. |
| `tests/git_draft.rs` | 2 | integration | `GitDraftStore` round trips under `.git`, stays out of `git status`, discards idempotently, and keeps one draft per linked worktree. | real `git` through `tests/support`, `tempfile` | subprocess, filesystem | kept, on the hermetic Git fixture (#474). The tests for gap G1 are deferred. | missing. See gap G1. |
| `tests/git_editor.rs`, `tests/git_index.rs`, `tests/git_status.rs`, `tests/git_worktree.rs` | 13 | integration | `GitEditor` edits the file with Git's editor and reports its failure. `GitIndexWriter` stages tracked changes and stages or unstages one path, a rename, or a path on an unborn branch. Status and operation reads match real repositories, including conflicts, stopped operations, and linked worktrees. Worktree listing skips ignored files and stops after 200 commits. | real `git` through `tests/support`, `tempfile` | subprocess | kept, on the hermetic Git fixture (#474) | preserved |
| `tests/git_repository.rs` | 6, plus 1 that runs only on Linux | integration | `GitRepositoryLocator` finds the root from nested directories and linked worktrees and picks the innermost repository. It says why a directory has no work tree, never offers `git init` at the filesystem root, and keeps paths with spaces. | real `git` through `tests/support`, `tempfile` | subprocess | kept, on the hermetic Git fixture (#474) | missing on macOS. See gap G6. |
| `tests/global_configuration.rs` | 13 | integration | `TomlGlobalConfigurationStore` loads the built-in default without creating storage and round trips by compare-and-swap. It rejects stale snapshots, unknown fields, nonregular files, non-directory roots, and symlinked files. It requests mode 0700, leaves no temporary files, and round trips Library dates. | `tempfile`, one inline taxonomy lookup | filesystem | kept, with the inline lookup (#469). The tests for gaps G2 to G5 are deferred. | missing |
| `tests/project_state.rs`, `tests/project_store.rs` | 18 | integration | `TomlProjectStore` initializes one file with ordered, complete taxonomies. Re-initialization preserves bytes, local state, and a custom ignore file. It refuses non-regular and symlinked paths and never regenerates an invalid project. `replace` writes only over the expected revision and refuses a stale or removed file. Errors name the TOML line and column and Git conflict markers. | real `git` through `tests/support`, `tempfile` | filesystem, subprocess | kept, on the hermetic Git fixture (#474). The tests for gap G4 are deferred. | missing |
| `tests/taxonomy_storage_v1.rs` | 4 | integration | Both stores read format version 1. A legacy global shape fails with an actionable message. Retired examples load without a write and disappear on save, and custom versions disappear at both boundaries. | `tempfile` | filesystem | kept | preserved |
| `tests/app_services.rs` | 9 | integration | App services wired to FS adapters in one process. Commit submission requires staged changes and can retry after a hook rejects it. Project services follow a directory from no repository to a tracked project. A failed or partial initialization writes only what it should. | real `git` through `tests/support`, `GitCommitWriter`, `TomlProjectStore`, a `Library` fake, local `head_exists` and `assert_project_shape` | subprocess | moved from `crates/gitserious/tests/interactive.rs` (#478) | preserved |

### gitserious-cli

| Location | Tests | Category | Contract protected | Dependencies | Isolation | Stack outcome | Coverage |
|---|---|---|---|---|---|---|---|
| `src/cli.rs` | 6 | unit | `parse` maps each command to a destination. `help` opens the TUI on a terminal and prints when redirected. `-h` and `-V` print on stdout. Misplaced root flags and other usage errors reject with exit code 2 and name the way out. | private `HelpText` | pure | kept | preserved |
| `src/output.rs` | 2 | unit | `write_answer` writes each answer to one stream with its exit code. An error is one prefixed line and fails even when the stream cannot be written. | in-memory writers | pure | kept | preserved |
| `tests/invocation.rs` | 3 | integration | The public `parse` and `write_answer` succeed for help and version and reject unknown commands, extra arguments, and legacy flags with exit code 2. | none | pure | kept 3. #481 deleted `init_initializes_whatever_the_terminal` and `commit_and_config_open_the_shell_whatever_the_terminal`, which repeated `commands_select_a_destination_and_commit_does_not_author` in `src/cli.rs`. | preserved |

### gitserious-tui

| Location | Tests | Category | Contract protected | Dependencies | Isolation | Stack outcome | Coverage |
|---|---|---|---|---|---|---|---|
| `src/config/model_check/` (6 files) | 1 | property-model | Every key sequence up to each seed's depth keeps 14 Config editor properties, and each seed meets its reach goals. See [the model check](#the-model-check-mixed-six-responsibilities-in-one-file). | `Workspace` from `config/test_support.rs`, `LibraryLookup`, the `observe` hook in `config/taxonomies/mod.rs`, `TestBackend` | in-process, one worker thread per logical CPU | split from `src/config/model_check.rs` into six files (#482) | preserved, with the P4 limit under [Open items](#open-items) |
| `src/config/tests.rs` | 43 | unit, component | The Config Library outline, inspector, rail, header, footers, command palette, notices, and edit and remove flows render and route as the footer hints, at the minimum size and larger sizes. | `config/test_support.rs` and 19 file-local helpers | in-process | kept. Its shared helpers moved to `config/test_support.rs` (#476). Not split by area. | preserved |
| `src/config/direct_tests.rs` | 18 | unit, component | Direct persistence edits save once on `ctrl+w` and restore on Esc. A failed save keeps the fields for a retry. Copy, reorder, and delete roll back on failure. A stale Library cannot overwrite another session. A hidden edit pauses on a terminal that is too small. | a store wrapper over `test_support::team` that counts saves and fails on demand | in-process | kept, on `config/test_support.rs` (#476) | preserved. The 5 hidden-edit tests overlap `root/tests/config_tests.rs` but assert store writes, not root routing. |
| `src/config/library_*_tests.rs` (5 files) | 19 | unit, component | Library rows name their origin, type count, and version, and narrow rows drop the count first. Each row takes exactly the keys its footer hints. Policies read as plain words. Type rows show property tables, and edits show focus cues and counters. | `fixture`, `press`, and `select` from `config/test_support.rs` | in-process | kept, on `config/test_support.rs` (#476) | preserved |
| `src/config/copy_tests.rs`, `src/config/alert_tests.rs` | 6 | unit, component | `c` copies a property to the chosen types, and each copy changes apart from its source. It refuses invalid targets with a sentence. The copy picker, delete dialog, and command palette center on the terminal. | `team` from `config/test_support.rs` | in-process | kept, on `config/test_support.rs` (#476) | preserved |
| `src/config/project_preview_tests.rs` | 3 | unit, component | The Project section shows installed definitions in the Library layout, never mutates them, and hands initialization to the root. | `Workspace` from `config/test_support.rs` | in-process | kept, on `config/test_support.rs` (#476) | preserved |
| `src/config/library_header.rs` | 2 | unit | The Library path reads from `~` under the home directory and keeps its end when it is too long. | none | pure | kept | preserved |
| `src/config/taxonomies/` (3 files) | 15 | unit | The outline counts hidden rows, opens at most one root, remembers open types, and parts from the inspector 60 to 40. Focused fields stay visible. Columns cut on whole graphemes. The inspector shows lineage, dates, and type tables. | `LibraryLookup` from `config/library_lookup.rs`, `TestBackend` | in-process | kept, with the local lookup (#470) | preserved |
| `src/commit/author/tests.rs`, `src/commit/author/tests/picker.rs`, `src/commit/author/tests/citation_picker.rs` | 48 | unit, component | Authoring switches taxonomies through a confirmed discard. Review shows the exact bytes and shares the 80-column boundary with the encoder. Untouched and dirty cancellation take distinct confirmations. The `@` citation list opens, filters, inserts paths and short SHAs, and draws inside the composer. The pickers wrap, scroll, and take clicks. | key and render helpers, `presentation_definition` exported to `composer/tests.rs` | in-process | kept. #477 moved the 24 citation picker tests to `tests/citation_picker.rs`. | preserved |
| `src/commit/author/composer/` (2 files) | 26 | unit | The composer wraps scope and subject, keeps undo history per field, and moves through wrapped body rows. It shows requirement markers, guidance, and counters, and it rewraps on resize. | helpers from `author/tests.rs` | in-process | kept | preserved |
| `src/commit/author/state.rs` | 5 | unit | The commit dialog keeps the draft and cancels by mouse. An empty required property is named by its key. A reopened review keeps the exact bytes. The taxonomy picker wraps in both directions. | `TestBackend` | in-process | kept | preserved |
| `src/commit/mod.rs`, `src/commit/status_tree/mod.rs` | 5 | unit | The Commit page cuts a long staged list and keeps its key visible. Review names an operation first, then conflicts, then an empty index. Tree keys stay inside the rows, and clicks select the row drawn there. Partial staging appears in both groups. | `#[cfg(test)]` accessors in `commit/mod.rs` | in-process | kept | preserved |
| `src/root/tests/navigation_tests.rs`, `src/root/tests/help_tests.rs` | 24 | integration, built in the lib target | The root draws one footer band and switches sections only with Tab and Shift+Tab. Palettes are modal. Quit goes through its confirmation from every page. Key releases change nothing. Help pages take focus, scroll, and keep the way back at the minimum size. | fake workspaces from `root/tests/support.rs` | in-process | kept | preserved |
| `src/root/tests/commit_tests.rs`, `src/root/tests/staging_tests.rs`, `src/root/tests/draft_tests.rs` | 30 | integration, built in the lib target | Commit opens on the right stage and keeps rejected reviews resumable. Space and `a` stage and unstage exactly the rows their hints name. Focus rereads status. A draft left on quit comes back until a commit or a confirmed discard. | `root/tests/support.rs` fakes, plus one fake workspace each in `staging_tests.rs` and `draft_tests.rs` | in-process | kept | preserved |
| `src/root/tests/config_tests.rs`, `src/root/tests/home_tests.rs`, `src/root/tests/project_overview_tests.rs`, `src/root/tests/setup_tests.rs` | 26 | integration, built in the lib target | Home offers only the step its state allows and rereads the project on focus. A Config edit locks the tabs and quits through one discard confirmation. Home, Project, and Commit open one initialization editor that writes nothing on Esc. | fake workspaces from `root/tests/support.rs`, plus one in `setup_tests.rs` | in-process | kept | preserved |
| `src/home/init_editor.rs` | 7 | unit | The initialize editor starts with the built-ins included and `conventional` first. Space, Enter, and Alt with an arrow include, make the default, and reorder. The last included taxonomy stays. The list scrolls, and the initialize row lists what it writes. | `TestBackend` | in-process | kept | preserved |
| `src/help/page.rs`, `src/shared/components/confirmation_dialog.rs`, `src/shared/paths.rs`, `src/theme.rs` | 7 | unit | Help spans are strong only while SGR bold is on. Dialog buttons are clickable and lose their targets on resize. Paths break after separators. Crowded footers drop arrow hints first. | none | pure, or in-process with `TestBackend` | kept | preserved |

### gitserious

| Location | Tests | Category | Contract protected | Dependencies | Isolation | Stack outcome | Coverage |
|---|---|---|---|---|---|---|---|
| `tests/help.rs` | 15 | integration | The binary prints the concise help for `-h` and `--help` and command help as the description, then the usage. Help wraps at `COLUMNS`, up to 100. `help nope` and misplaced root flags are usage errors. Each TUI help page matches what `help <command>` prints at the same width. | the binary through `support::Home`, `help_pages` from `src/help.rs` through `#[path]`, `gitserious_cli::{help_topics, styled_help}`, `gitserious_tui::HelpPage` | subprocess | kept. The binary runs through `support::Home` (#479). The 3 tests that copied `help_pages` now run the production function (#480). | preserved. `root_flags_with_a_command_are_usage_errors_that_name_the_way_out` repeats the CLI unit test of the same name and adds the process exit code. |
| `tests/home.rs` | 4 | integration | A bare run without a terminal prints the concise help and creates no project files. Help never pages without a terminal. Help names `gitserious` whatever the executable is called. | the binary through `support::Home` and `Home::command_for`, `tempfile` | subprocess | kept, run through `support::Home` (#479) | preserved |
| `tests/init.rs`, `tests/config.rs`, `tests/commit.rs` | 21 | integration | `init` writes exact version-control files, never rewrites or regenerates a project, resolves linked worktrees, and fails without writing when Git, a repository, or a readable Library is missing. `config` without a terminal loads nothing. `commit` without a terminal stops before authoring, and Git stores the exact reviewed message. | the binary, `support::{Home, git, repository, head_exists, assert_project_shape}`, and for 4 `commit.rs` tests App's commit services with `TomlProjectStore` and `GitCommitWriter` | subprocess. The 4 `commit.rs` tests also run FS adapters in process. | kept, on the hermetic Git environment (#479). Moving the 4 in-process tests is deferred. | preserved |
| `tests/terminal_help.rs` | 11, Unix only | integration | In a pseudo terminal, help flags print without paging and `help` opens the TUI. Each command opens its first page without authoring or Git writes. Commit opens the type picker, focus rereads Git, and a draft left on quit comes back. Three tests check the terminal fixture itself. | `portable-pty`, `vt100`, `support::Home`, `support/terminal.rs` | subprocess, terminal | kept | preserved |

## Findings

Each finding describes the workspace at `5740240`, corrected where later work showed the original text was wrong, and ends with its outcome.

### Tests that sat outside the crate or module that owns them

- `crates/gitserious/tests/interactive.rs` never ran the binary. Its 9 tests wired App services to FS adapters in process. `gitserious-fs` already depends on `gitserious-app`, so these tests belonged in `crates/gitserious-fs/tests/`. #478 moved them to `crates/gitserious-fs/tests/app_services.rs`.
- The inventory missed 4 more in-process tests in `crates/gitserious/tests/commit.rs`. `git_stores_the_exact_reviewed_taxonomy_message`, `project_owned_conventional_replaces_the_built_in_definition`, `a_merge_in_progress_never_reaches_git_commit`, and `a_first_commit_stages_its_files_without_git_add` call App's commit services with FS adapters. Three of them run the binary's `init` for setup. Moving them is deferred.
- Three tests in `crates/gitserious/tests/help.rs`, not two, built their pages with `interactive_help`. They were `only_the_concise_help_points_to_the_help_command`, `each_help_page_shows_what_its_help_command_prints_at_the_same_width`, and `help_pages_mark_headings_and_names_strong_and_leave_descriptions_plain`. That function copied `help_pages` from `crates/gitserious/src/main.rs`, so the tests still passed when the real wiring changed. #480 moved `help_pages` unchanged to `crates/gitserious/src/help.rs`, and `tests/help.rs` includes that file.
- `crates/gitserious-fs/tests/unit/` held 17 unit tests that `crates/gitserious-fs/src/lib.rs` compiled into the lib with `#[path = "../tests/unit/mod.rs"]`. #473 moved 12 into `src/platform/xdg.rs` and `src/platform/windows.rs` and 5 into `tests/directory.rs`, and removed the directory and the `#[path]` line.
- `gitserious-app` exported `pub mod test_support` behind a feature so that FS and TUI tests could share one App helper. Every component of `scripts/ci/run-rust-quality.sh` passes `--all-features`, so check, lint, test, and the release build compiled the module into the App lib. #471 removed the feature and the module.
- `taxonomy_fingerprint_changes_for_every_semantic_dimension` in `crates/gitserious-app/tests/init.rs` protects `fingerprint_taxonomy` in `src/config/taxonomy_policy.rs`, not initialization. It still sits in `tests/init.rs`, and moving it is deferred.
- In TUI, `config/tests.rs`, `config/copy_tests.rs`, `config/library_rows_tests.rs`, and `commit/author/tests.rs` were test modules that also acted as fixture modules for their siblings. #476 moved the Config fixtures to `config/test_support.rs`, so no Config test file imports another. `composer/tests.rs` still imports helpers from `commit/author/tests.rs`, which #477 left in place.
- TUI production modules carry `#[cfg(test)]` hooks. They are 5 accessors in `commit/mod.rs`, `render` and `CONTENT_X` in `config/mod.rs`, `observe` with `Observation`, `ObservedMode`, and `ObservedBaseline` in `config/taxonomies/mod.rs` (lines 904 to 1016), `Row::names` in `config/taxonomies/outline.rs`, `area` in `root/navigation.rs`, and `built_in_context` in `commit/author/mod.rs`. Ten Config test files and `config/model_check/snapshot.rs` read `observe`, so it is the Config tests' shared view of editor state, not a model-check-only hook. The stack kept them.

### The App test-support feature was the only cross-crate test dependency

At `5740240`, these lines declared and enabled the feature:

- `crates/gitserious-app/Cargo.toml:19` declared `test-support = []`.
- `crates/gitserious-app/Cargo.toml:22` enabled it for App's own integration tests through a self dev-dependency.
- `crates/gitserious-app/src/lib.rs:14-15` compiled `pub mod test_support` under `#[cfg(any(test, feature = "test-support"))]`.
- `crates/gitserious-fs/Cargo.toml:20` enabled it for FS tests.
- `crates/gitserious-tui/Cargo.toml:21` enabled it for TUI tests.
- `scripts/ci/check-crate-dependencies.py:29-30` permitted the App self edge only for this feature.

`test_support` held one trait, `LibraryLookup`, with one method. `find_path` turned a path of 1 to 3 names (taxonomy, type, property) into a `NodeId`. It used only public `LibraryDraft` accessors (`taxonomies`, `types`, `properties`, `name`, and `id`), so any crate could define the same 25 lines locally. The 13 consumer files made 49 calls. At `f6a082c`, none of the lines above exist, and `check-crate-dependencies.py` rejects an App self edge.

| Consumer at `5740240` | Calls | Longest path | What it needs | Lookup at `f6a082c` |
|---|---|---|---|---|
| `crates/gitserious-app/src/config/library_draft/tests.rs` | 3 | 3 names, through its `find` wrapper | An in-crate test. Plain `#[cfg(test)]` covers it, with no feature. | a private `find_path` in the file (#471) |
| `crates/gitserious-app/tests/configuration.rs` | 5 | 2 names | A local helper in App's integration tests | a private `find_path` in the file that walks up to three names (#471) |
| `crates/gitserious-app/tests/taxonomy_policy_v1.rs` | 2 | 1 name | A taxonomy lookup by name | a private `taxonomy` function in the file (#471) |
| `crates/gitserious-fs/tests/global_configuration.rs` | 1 | 1 name | A taxonomy lookup by name, one expression long | inline in the test (#469) |
| `crates/gitserious-tui/src/config/alert_tests.rs` | 1 | 1 name | A taxonomy lookup | `config/library_lookup.rs` (#470) |
| `crates/gitserious-tui/src/config/copy_tests.rs` | 6 | 3 names | Property lookup | `config/library_lookup.rs` (#470) |
| `crates/gitserious-tui/src/config/direct_tests.rs` | 3 | 3 names | Prefixes of a property path | `config/library_lookup.rs` (#470) |
| `crates/gitserious-tui/src/config/library_keys_tests.rs` | 1 | a path from a table | Any depth | `config/library_lookup.rs` (#470) |
| `crates/gitserious-tui/src/config/library_rows_tests.rs` | 2 | a path from its caller | Any depth | `config/library_lookup.rs` (#470). One call moved to `config/test_support.rs` (#476). |
| `crates/gitserious-tui/src/config/model_check.rs` | 2 | the cursor path from `Observation` | Any depth | `config/library_lookup.rs` (#470), now called from `model_check/properties.rs` and `model_check/seeds.rs` (#482) |
| `crates/gitserious-tui/src/config/taxonomies/inspector.rs` | 7 | 3 names | Property lookup | `config/library_lookup.rs` (#470) |
| `crates/gitserious-tui/src/config/taxonomies/layout_tests.rs` | 4 | 2 names | Type lookup | `config/library_lookup.rs` (#470) |
| `crates/gitserious-tui/src/config/tests.rs` | 12 | 3 names | Property lookup and prefixes of a path | `config/library_lookup.rs` (#470) |

No other test reached into another workspace crate. The executable's tests import the five library crates through its normal dependencies.

### Duplicated and unnecessary fixtures

The rule of three decided each candidate. Consolidate when three or more copies do the same job. Leave copies that differ in behavior or number fewer than three.

These were consolidated:

- FS Git helpers. Seven FS test files defined `fn git`, and seven defined `fn repository`. They disagreed on isolation. `git_draft.rs`, `git_index.rs`, `git_status.rs`, and `git_worktree.rs` set `GIT_CONFIG_GLOBAL` and `GIT_CONFIG_SYSTEM` to `/dev/null`. The other five files that ran Git set neither. No FS test removed `GIT_DIR`, `GIT_WORK_TREE`, or `GIT_INDEX_FILE`. #474 added `crates/gitserious-fs/tests/support/mod.rs`, and all nine files use it. `tests/app_services.rs` uses it too since #478.
- App `RepositoryLocator` fakes. There were five, `Locator` in `commit_services.rs`, `NoRepository` in `configuration.rs`, `FakeLocator` in `init.rs`, `Locator` in `project_configuration.rs`, and `Locator` in `project_services.rs`. The inventory predicted that one configurable fake would replace all five. Only three could share it. `init.rs` asserts its fake's calls and error, and `configuration.rs` never calls its fake. #472 added `crates/gitserious-app/tests/support/mod.rs` with one `Locator` for `commit_services.rs`, `project_configuration.rs`, and `project_services.rs`.
- TUI Config helpers. `Workspace`, `footer_text`, and `create_taxonomy` came from `config/tests.rs`, `team` from `copy_tests.rs`, and `fixture`, `press`, and `select` from `library_rows_tests.rs`. #476 moved them, with `key` and the `Result` alias, to `config/test_support.rs`.
- The executable's `git` helper in `tests/interactive.rs` repeated `support::git` without the repository variable cleanup. #478 replaced it with FS's fixture when the file moved.

These were left alone:

- App `ProjectConfigurationStore` fakes (5) and `GlobalConfigurationStore` fakes (3). Each records calls, fails per operation, tracks revisions, or always fails in its own way. A shared fake would need a switch for each test and hide what the test sets up.
- App `FakeError`. Only the copies in `configuration.rs` and `project_configuration.rs` match. The ones in `init.rs` and `global_storage.rs` are different enums.
- FS `has_temporary_files`, with 2 copies. It reaches three copies only if the tests for gap G4 add one.
- TUI one-line event builders such as `key`, `ctrl`, `click`, and `press`, which 11 test files define. Each is one line and keeps its file readable alone.
- TUI root fake workspaces. There are seven, four in `root/tests/support.rs` and one each in `draft_tests.rs`, `setup_tests.rs`, and `staging_tests.rs`. `Unavailable`, `Ready`, and `HomeWorkspace` in `support.rs` already forward to `Services` except for the behavior their tests need.

FS `TestDirectory` in `tests/unit/support.rs` had one copy, so it was unnecessary rather than duplicated. It hand-rolled a unique temporary directory, while `tempfile` is already a normal FS dependency. #473 deleted it.

### Coverage gaps

The FS gaps come from reading `crates/gitserious-fs/src/` against the tests. None was proven by a failing test.

- G1, open. The draft store has no test that loads a corrupt draft or one with unknown fields, though `DraftWire` sets `deny_unknown_fields`. No test checks that `save` leaves no temporary file or that an unreadable draft reports its path. `src/git_draft.rs` follows a symlinked draft path, unlike the project and global stores, and no test pins either policy.
- G2, open. No FS test makes a configuration, project, or draft file unreadable. A search for `PermissionDenied` and mode `0o000` in the FS tests finds nothing.
- G3, a known limit. `compare_and_swap` in `taxonomy_global.rs` reads the file again after it writes the temporary file, and `replace` in `taxonomy_project.rs` calls `ensure_current` twice. Tests cover only the first check. A test of the second check needs a writer that runs between the two reads, which the current code gives tests no way to inject.
- G4, open. `global_configuration.rs` and `project_state.rs` assert that no temporary files remain. `project_store.rs` never does, after a create collision, a stale replace, or a replace of a removed file.
- G5, open. Tests cover a symlinked global configuration file and a symlinked project file, `.gitserious` directory, and `.gitignore`. No test covers a symlinked global configuration directory.
- G6, a known limit. The non-UTF-8 worktree path test runs only on Linux, so macOS runs 6 of the 7 tests in `git_repository.rs`. Paths with spaces are covered for repository location and commits.
- G7, closed by #474. `crates/gitserious-fs/tests/commit.rs` committed with the developer's global Git configuration and did not set `commit.gpgsign` to `false`. Under a hostile global configuration, 6 FS tests failed before #474 and none after it. Under an exported `GIT_DIR`, the old tests wrote a test identity and 203 commits into a decoy repository. The fixture now refuses to start instead. These results are recorded in #474.

One gap sat outside FS. At `5740240`, every test in `crates/gitserious/tests/help.rs` and `crates/gitserious/tests/home.rs` ran the binary with the developer's `HOME` and XDG variables. With `HOME` unset, all 15 help tests and all 4 home tests failed, as recorded in #479. The inventory said that a run could read the developer's Library. That was not borne out. Before #479, a run with a malformed Library under a temporary `HOME` failed none of the 4 home tests. #479 runs all 19 tests through `support::Home`.

## The model check mixed six responsibilities in one file

At `5740240`, `crates/gitserious-tui/src/config/model_check.rs` was 1,591 lines with one test. It drives the real Config `State` through every key sequence from 17 seeds and checks 14 properties on every transition. The line ranges below refer to that file. #482 split it into `crates/gitserious-tui/src/config/model_check/`, and [Testing](testing.md#the-config-model-check) lists the new files.

| Responsibility | Lines at `5740240` | Contents | File at `f6a082c` |
|---|---|---|---|
| Scope | 1 to 11 | Module docs that say what the check covers and leaves to interaction tests | `mod.rs` |
| Engine | 38 to 95, 1321 to 1348, 1380 to 1417, 1419 to 1575 | The key alphabet in `INPUTS`, `NAMED_ONLY`, and `COPY_KEYS`. `replay` rebuilds the workspace and replays every key. `replay_all` runs replays on one worker thread per logical CPU. `start`, `advance`, and `input_indices` walk to a seed's start, and `explore` runs the breadth-first search, deduplicating states by their `Debug` text. | `engine.rs`, except `COPY_KEYS`, which is in `seeds.rs` |
| Seeds | 97 to 138, 140 to 347, 349 to 387, 393 to 488 | Seed types (`Seed`, `Start`, `StartMode`, `Alphabet`, and `Reach`), the 17 `SEEDS`, start predicates, and seed workspaces built on `Workspace` from `config/tests.rs` | `seeds.rs`, with the shared start predicates `at_outline`, `selects`, and `editing` in `snapshot.rs` and `Workspace` from `config/test_support.rs` |
| Snapshots | 490 to 578 | `Snapshot` captures the screen, the `Observation` hooks from `config/taxonomies/mod.rs`, the Library draft, the stored Library, dialogs, and the frame text. | `snapshot.rs` |
| Properties | 580 to 640, 642 to 1319 | The `Check` type, the `PROPERTIES` table, and each property with its helpers | `properties.rs` |
| Diagnostics | 1350 to 1378, 1547 to 1569 | `check` reports the seed, the keys to its start, the keys after it, and the screens before and after the failing key. `replay_all` turns a panic into a failure. `explore` reports reach expectations that no state met. | `diagnostics.rs`, with the reach report in `check_reaches`. The panic handling stays in `replay_all` in `engine.rs`. |
| Coverage reporting | 1577 to 1591 | The test prints each seed's depth, states, transitions, and time on stderr, visible under `--nocapture`. | `mod.rs` |

#482 records that exploration order, seeds, depth, fingerprints, and every property are unchanged. Nine runs printed the same 17 coverage lines, at 5,109 states and 12,442 transitions. One run at `f6a082c` with `--nocapture` printed the same totals.

### Run time

At `5740240`, three runs of the one test, with the binary built once and run with `--exact` and `--nocapture`, took 94.35 s, 92.86 s, and 103.78 s of wall time. User CPU time was 451 s, 476 s, and 501 s. The other 284 TUI tests took 6.25 s, 6.81 s, and 6.64 s with the model check skipped. The model check is about 93 percent of the TUI lib target's wall time. One seed, `a new taxonomy in EDIT on its Name`, explores 2,693 states and 5,771 transitions and takes 33.0 s to 37.0 s, about a third of the run.

At `f6a082c`, one run of `cargo test -p gitserious-tui every_bounded_key_sequence -- --nocapture` took 94.23 s of harness time. #482 ran 3 alternated pairs of the original and the split. The split ran 7 to 9 percent faster in each pair, which is inside the 12 percent spread between runs, so #482 claims neither a speedup nor a regression.

### Replay from the initial state is a visible cost

To measure replay, a copy of the tree at `5740240` outside the repository added counters and timers to `replay` and `explore` and ran the test once. That run took 100.64 s of wall time and 429 s of user CPU time.

| Measure | Value |
|---|---|
| Replays | 12,579. That is 12,442 explored transitions plus 137 replays that walk to seed starts. |
| Key events replayed | 110,500, an average of 8.8 per replay |
| Key events that are new transitions | 12,442, or 11.3 percent of the key events replayed |
| Worker thread time in workspace setup and the first draw | 32.0 s |
| Worker thread time handling and redrawing replayed keys | 365.2 s |
| Worker thread time capturing snapshots | 12.5 s |
| Main thread time in property checks | 27.6 s |
| Main thread time formatting state fingerprints | 18.2 s |

Replay is a visible cost. Handling and redrawing keys is the largest share of thread time, and 88.7 percent of those key events repeat a prefix that an earlier replay already ran.

Replay is not the whole wall time. `explore` runs `check` and the fingerprint `format!` on the main thread, after each level's parallel replays return. Those two took about 46 s, about half of the wall time. Removing prefix replay would shorten only the parallel phase, so the run would still take at least about 46 s unless the checks also move off the main thread. That floor is inferred from the code and the counters, not measured.

These numbers have limits. Thread time is elapsed time summed per thread, not CPU time. The instrumented run is one sample. It formats a new state's fingerprint twice, which adds main-thread work that the original does not do. The three uninstrumented runs spread 10.9 s, about 12 percent, so a change smaller than that does not show in one run. All runs use the debug test profile that CI uses. The host is a desktop with other work on it, and after the runs its load average was 2.8 over one minute and 6.5 over fifteen. A performance comparison needs at least 5 alternating runs per side. No pull request in the stack optimizes the check.

## Migration map

The plan numbered the stack PR1 to PR13. The table maps each planned item to its GitHub pull request. Done means the item landed as planned. Skipped means a pull request decided against it and says why. Deferred means no pull request in #468 to #482 did it, and it stays open.

| Finding | Planned | Outcome |
|---|---|---|
| Record the baseline and this inventory. | PR1 | done in #468 |
| Replace `LibraryLookup` in `crates/gitserious-fs/tests/global_configuration.rs` and remove `crates/gitserious-fs/Cargo.toml:20`. | PR2, FS local lookup | done in #469 |
| Give TUI a test-only path lookup for its 9 consumer files and remove `crates/gitserious-tui/Cargo.toml:21`. | PR3, TUI local lookup | done in #470, as `crates/gitserious-tui/src/config/library_lookup.rs` |
| Remove `test-support` from `crates/gitserious-app/Cargo.toml:19` and `:22`, make `test_support` test-only or local, give `tests/configuration.rs` and `tests/taxonomy_policy_v1.rs` a local helper, and remove the self edge in `scripts/ci/check-crate-dependencies.py:29-30`. | PR4, App drops the feature | done in #471. Three App files keep private lookups instead of a shared module, because only two files could share one and their depths differ. |
| Share one configurable `RepositoryLocator` fake across 5 App files. | PR5, App fixtures | done in #472 for 3 files. `init.rs` and `configuration.rs` keep their own fakes. |
| Move the fingerprint test out of `tests/init.rs`. | PR5, App fixtures | deferred |
| Fold `tests/taxonomy_policy_v1.rs` into `tests/configuration.rs`. | PR5, App fixtures | deferred |
| Keep the App store fakes and `FakeError` as they are. | PR5, App fixtures | done. #472 left them in their files. |
| Move `tests/unit/` out of `tests/`, and replace `TestDirectory` with `tempfile`. | PR6, FS contracts | done in #473 |
| Add one isolated Git fixture for the 9 FS files that run Git, covering gap G7. | PR6, FS contracts | done in #474 |
| Add FS tests for gaps G1, G2, G4, and G5. Record G3 and G6 as known limits unless a seam for G3 is added. | PR7, FS contracts | deferred for G1, G2, G4, and G5. G3 and G6 are recorded as known limits above. |
| Move the shared Config helpers into one TUI test-support module. | PR8, TUI areas | done in #476, as `crates/gitserious-tui/src/config/test_support.rs` |
| Group Config tests by area. | PR8, TUI areas | skipped in #476. The 43 tests in `config/tests.rs` share 19 file-local helpers, and a split would move at least five into shared support or copy them. The other Config areas already had their own files. |
| Put the citation picker tests in their own file. Not in the plan. | none | done in #477 |
| Keep the root fake workspaces. | PR8, TUI areas | done. No pull request changed them. |
| Keep the model-check timings above as the baseline. | PR9, model-check baseline | skipped as a separate pull request. The timings in #468 are the baseline, and #482 measured the split against the original. |
| Split `model_check.rs` along the six responsibilities above, and give the `gitserious-verify-config-editor` skill the new paths. | PR10, model-check split | done in #482. The skill names `model_check/` and its files. |
| Act on run time only if the baseline asks for it. | PR11, model-check performance, conditional | deferred. Replay against serial property checks was not measured beyond the instrumented run above. |
| Move `tests/interactive.rs` to `crates/gitserious-fs/tests/` on the shared Git fixture. | PR12, executable fixtures | done in #478, as `crates/gitserious-fs/tests/app_services.rs` |
| Run `tests/home.rs` through `support::Home`. | PR12, executable fixtures | done in #479, for `tests/help.rs` as well |
| Make the help-page tests use the real `help_pages` wiring. | PR12, executable fixtures | done in #480, for 3 tests, not 2 |
| Delete or fold the 2 redundant tests in `crates/gitserious-cli/tests/invocation.rs`. | none | done in #481 |
| Fix the dev-dependency sentence in `workspace-baseline.md`, update the test ownership notes in `application-consumers.md`, and update this inventory to the final layout. Add [Testing](testing.md). | PR13, docs | done in this pull request |
| Rebaseline or retire the rest of `application-consumers.md`. | PR13, docs | deferred. Its export index still records `b8c03d1`. |

## Open items

These items stay open after the stack. Each says what is known and what was not verified.

- The 4 in-process tests in `crates/gitserious/tests/commit.rs` call App's commit services with FS adapters. They belong with `crates/gitserious-fs/tests/app_services.rs`. Moving them means replacing their setup through the binary's `init`.
- The P4 reach goal in `crates/gitserious-tui/src/config/model_check/seeds.rs` cannot detect `optional` leaving the policy cycle. #482 records that the mutation named for P4 passes, because the seed's `notes` property already starts at `optional`.
- The tests for FS gaps G1, G2, G4, and G5 do not exist. G3 and G6 are known limits.
- FS adapters run Git with the test process's environment, and the fixtures cannot clear it without `unsafe` code. Whether the adapters should honor an inherited `GIT_DIR` is an open product decision. Until it is made, the fixtures refuse to start under `GIT_DIR`, `GIT_WORK_TREE`, or `GIT_INDEX_FILE`.
- Neither Git fixture clears `GIT_CONFIG_PARAMETERS` or `GIT_CONFIG_COUNT`, so configuration exported in either variable still reaches the tests' Git commands and the adapters.
- `system_adapter_uses_xdg_on_unix` in `crates/gitserious-fs/src/platform/xdg.rs` resolves paths from the real process environment. With `HOME` and `XDG_CONFIG_HOME` unset, it fails with `HomeUnavailable`. That was measured at `f6a082c`.
- In `crates/gitserious/tests/support/mod.rs`, `with_stale_fingerprint` has no consumer, and `assert_project_shape` has one, `tests/init.rs`, since #478. The file's `#![allow(dead_code)]` hides both from the build and from `check-dead-code.sh`, which scans only `crates/*/src`.
- The stack was verified locally only on macOS. The Linux-only non-UTF-8 path test, the `cfg(windows)` code in `src/platform/windows.rs`, and the dead-code scan first run in CI.
- The model check's split between prefix replay and the serial property checks and fingerprints was measured once, in the instrumented run at `5740240`. No later run measured it.
