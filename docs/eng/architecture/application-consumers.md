# Application interfaces and consumers

This inventory records the App crate-root exports and their migration constraints at `b8c03d1a46a56cced76bbdc760e76eb9437f52ea`. It complements [the Cargo dependency baseline](workspace-baseline.md).

## Export index

All 71 crate-root public re-exports from `crates/gitserious-app/src/lib.rs` appear below. Definitions name the defining module, not only a re-exporting module.

Consumer columns are a source-token index by crate, reviewed against the use-case flows below. They are candidates, not compiler-proven symbol resolution or a call graph. Inline test modules are excluded from the production index where declared as `#[cfg(test)] mod tests`. Test columns can include a name in an import, callback, assertion, or inline test. A generic name can also match a different symbol. Absence of an external candidate does not establish dead code. Public methods on exported types remain part of those types' contracts and are not separate crate-root exports.

| Export | Kind | Definition | External production-source candidates | Test-source candidates |
|---|---|---|---|---|
| `AuthoredCommit` | struct | [commit/taxonomy_authoring_context.rs](../../../crates/gitserious-app/src/commit/taxonomy_authoring_context.rs) | `gitserious-tui` | `gitserious`, `gitserious-app`, `gitserious-tui` |
| `BranchState` | enum | [commit/repository_status.rs](../../../crates/gitserious-app/src/commit/repository_status.rs) | `gitserious-fs`, `gitserious-tui` | `gitserious-fs`, `gitserious-tui` |
| `CommitAuthoringContext` | struct | [commit/taxonomy_authoring_context.rs](../../../crates/gitserious-app/src/commit/taxonomy_authoring_context.rs) | `gitserious-tui` | `gitserious`, `gitserious-app`, `gitserious-tui` |
| `CommitAuthoringOutcome` | enum | [commit/taxonomy_authoring_context.rs](../../../crates/gitserious-app/src/commit/taxonomy_authoring_context.rs) | No external source candidate | `gitserious`, `gitserious-app` |
| `CommitDraftAuthor` | trait | [commit/commit_draft_author.rs](../../../crates/gitserious-app/src/commit/commit_draft_author.rs) | No external source candidate | `gitserious`, `gitserious-app` |
| `CommitDraftAuthorOutcome` | enum | [commit/commit_draft_author.rs](../../../crates/gitserious-app/src/commit/commit_draft_author.rs) | `gitserious-tui` | `gitserious`, `gitserious-app`, `gitserious-tui` |
| `CommitOutcome` | enum | [commit/create_taxonomy_commit.rs](../../../crates/gitserious-app/src/commit/create_taxonomy_commit.rs) | No external source candidate | `gitserious`, `gitserious-app` |
| `CommitOutput` | struct | [commit/commit_writer.rs](../../../crates/gitserious-app/src/commit/commit_writer.rs) | `gitserious-fs`, `gitserious-tui` | `gitserious-app`, `gitserious-tui` |
| `CommitTaxonomy` | struct | [commit/taxonomy_authoring_context.rs](../../../crates/gitserious-app/src/commit/taxonomy_authoring_context.rs) | `gitserious-tui` | `gitserious-app`, `gitserious-tui` |
| `CommitWriter` | trait | [commit/commit_writer.rs](../../../crates/gitserious-app/src/commit/commit_writer.rs) | `gitserious-fs` | `gitserious-app`, `gitserious-fs` |
| `CreateCommitError` | enum | [commit/create_taxonomy_commit.rs](../../../crates/gitserious-app/src/commit/create_taxonomy_commit.rs) | No external source candidate | `gitserious-app` |
| `RepositoryStatus` | struct | [commit/repository_status.rs](../../../crates/gitserious-app/src/commit/repository_status.rs) | `gitserious-fs`, `gitserious-tui` | `gitserious-fs`, `gitserious-tui` |
| `RepositoryStatusReader` | trait | [commit/repository_status.rs](../../../crates/gitserious-app/src/commit/repository_status.rs) | `gitserious-fs` | `gitserious-fs` |
| `StatusEntry` | struct | [commit/repository_status.rs](../../../crates/gitserious-app/src/commit/repository_status.rs) | `gitserious-fs`, `gitserious-tui` | `gitserious-fs`, `gitserious-tui` |
| `create_commit_with_taxonomy` | fn | [commit/create_taxonomy_commit.rs](../../../crates/gitserious-app/src/commit/create_taxonomy_commit.rs) | No external source candidate | `gitserious`, `gitserious-app` |
| `prepare_commit_with_taxonomy` | fn | [commit/create_taxonomy_commit.rs](../../../crates/gitserious-app/src/commit/create_taxonomy_commit.rs) | No external source candidate | No test-source candidate |
| `validate_reviewed_commit` | fn | [commit/create_taxonomy_commit.rs](../../../crates/gitserious-app/src/commit/create_taxonomy_commit.rs) | No external source candidate | No test-source candidate |
| `Collision` | enum | [config/library_draft/mod.rs](../../../crates/gitserious-app/src/config/library_draft/mod.rs) | `gitserious-fs`, `gitserious-tui` | `gitserious-app`, `gitserious-fs`, `gitserious-tui` |
| `ConfigurationDestination` | enum | [config/taxonomy_configuration_editor.rs](../../../crates/gitserious-app/src/config/taxonomy_configuration_editor.rs) | `gitserious-tui` | `gitserious-app`, `gitserious-tui` |
| `ConfigurationEditor` | trait | [config/taxonomy_configuration_editor.rs](../../../crates/gitserious-app/src/config/taxonomy_configuration_editor.rs) | `gitserious-cli`, `gitserious-tui` | `gitserious-app`, `gitserious-cli`, `gitserious-tui` |
| `ConfigurationError` | enum | [config/taxonomy_policy.rs](../../../crates/gitserious-app/src/config/taxonomy_policy.rs) | `gitserious-fs` | `gitserious-app` |
| `ConfigurationSession` | struct | [config/taxonomy_configuration_editor.rs](../../../crates/gitserious-app/src/config/taxonomy_configuration_editor.rs) | `gitserious-tui` | `gitserious-app`, `gitserious-tui` |
| `ConfigurationWorkspace` | trait | [config/taxonomy_configuration_editor.rs](../../../crates/gitserious-app/src/config/taxonomy_configuration_editor.rs) | `gitserious-tui` | `gitserious-app`, `gitserious-tui` |
| `CopyIn` | enum | [config/library_draft/mod.rs](../../../crates/gitserious-app/src/config/library_draft/mod.rs) | `gitserious-tui` | `gitserious-app`, `gitserious-tui` |
| `DraftEdit` | enum | [config/library_draft/mod.rs](../../../crates/gitserious-app/src/config/library_draft/mod.rs) | `gitserious-tui` | `gitserious-app`, `gitserious-fs`, `gitserious-tui` |
| `DraftField` | enum | [config/library_draft/mod.rs](../../../crates/gitserious-app/src/config/library_draft/mod.rs) | `gitserious-tui` | `gitserious-app`, `gitserious-fs`, `gitserious-tui` |
| `EditRefusal` | enum | [config/library_draft/mod.rs](../../../crates/gitserious-app/src/config/library_draft/mod.rs) | `gitserious-tui` | `gitserious-app`, `gitserious-fs`, `gitserious-tui` |
| `GlobalConfiguration` | struct | [config/taxonomy_policy.rs](../../../crates/gitserious-app/src/config/taxonomy_policy.rs) | `gitserious-fs`, `gitserious-tui` | `gitserious`, `gitserious-app`, `gitserious-cli`, `gitserious-fs`, `gitserious-tui` |
| `GlobalConfigurationStore` | trait | [config/taxonomy_ports.rs](../../../crates/gitserious-app/src/config/taxonomy_ports.rs) | `gitserious-cli`, `gitserious-fs` | `gitserious`, `gitserious-app`, `gitserious-cli`, `gitserious-fs` |
| `LibraryDraft` | struct | [config/library_draft/mod.rs](../../../crates/gitserious-app/src/config/library_draft/mod.rs) | `gitserious-tui` | `gitserious-app`, `gitserious-fs`, `gitserious-tui` |
| `LibraryTaxonomy` | struct | [config/library_taxonomy.rs](../../../crates/gitserious-app/src/config/library_taxonomy.rs) | `gitserious-fs`, `gitserious-tui` | `gitserious`, `gitserious-app`, `gitserious-fs`, `gitserious-tui` |
| `NodeId` | enum | [config/library_draft/mod.rs](../../../crates/gitserious-app/src/config/library_draft/mod.rs) | `gitserious-tui` | `gitserious-app`, `gitserious-tui` |
| `NodeRef` | enum | [config/library_draft/mod.rs](../../../crates/gitserious-app/src/config/library_draft/mod.rs) | `gitserious-tui` | `gitserious-tui` |
| `Policy` | enum | [config/library_draft/mod.rs](../../../crates/gitserious-app/src/config/library_draft/mod.rs) | `gitserious-tui` | `gitserious-app`, `gitserious-tui` |
| `Problem` | struct | [config/library_draft/problems.rs](../../../crates/gitserious-app/src/config/library_draft/problems.rs) | `gitserious-tui` | `gitserious-app`, `gitserious-tui` |
| `ProblemKind` | enum | [config/library_draft/problems.rs](../../../crates/gitserious-app/src/config/library_draft/problems.rs) | `gitserious-tui` | `gitserious-app`, `gitserious-tui` |
| `ProblemSite` | enum | [config/library_draft/problems.rs](../../../crates/gitserious-app/src/config/library_draft/problems.rs) | `gitserious-tui` | `gitserious-app`, `gitserious-tui` |
| `ProjectConfiguration` | struct | [config/taxonomy_policy.rs](../../../crates/gitserious-app/src/config/taxonomy_policy.rs) | `gitserious-fs`, `gitserious-tui` | `gitserious`, `gitserious-app`, `gitserious-cli`, `gitserious-fs`, `gitserious-tui` |
| `ProjectConfigurationStore` | trait | [config/taxonomy_ports.rs](../../../crates/gitserious-app/src/config/taxonomy_ports.rs) | `gitserious-cli`, `gitserious-fs` | `gitserious-app`, `gitserious-cli`, `gitserious-fs` |
| `ProjectRevision` | struct | [config/taxonomy_policy.rs](../../../crates/gitserious-app/src/config/taxonomy_policy.rs) | `gitserious-fs` | `gitserious-app`, `gitserious-cli`, `gitserious-fs`, `gitserious-tui` |
| `PropertyNode` | struct | [config/library_draft/mod.rs](../../../crates/gitserious-app/src/config/library_draft/mod.rs) | `gitserious-tui` | `gitserious-tui` |
| `Review` | struct | [config/taxonomy_configuration_editor.rs](../../../crates/gitserious-app/src/config/taxonomy_configuration_editor.rs) | `gitserious-tui` | `gitserious-tui` |
| `StoredProject` | struct | [config/taxonomy_policy.rs](../../../crates/gitserious-app/src/config/taxonomy_policy.rs) | `gitserious-fs` | `gitserious-app`, `gitserious-cli`, `gitserious-tui` |
| `TaxonomyCatalog` | struct | [config/taxonomy_policy.rs](../../../crates/gitserious-app/src/config/taxonomy_policy.rs) | No external source candidate | `gitserious-app` |
| `TaxonomyDate` | struct | [config/taxonomy_date.rs](../../../crates/gitserious-app/src/config/taxonomy_date.rs) | No external source candidate | `gitserious-app` |
| `TaxonomyNode` | struct | [config/library_draft/mod.rs](../../../crates/gitserious-app/src/config/library_draft/mod.rs) | `gitserious-tui` | `gitserious-tui` |
| `TaxonomyOrigin` | enum | [config/taxonomy_policy.rs](../../../crates/gitserious-app/src/config/taxonomy_policy.rs) | `gitserious-tui` | `gitserious-app`, `gitserious-tui` |
| `TypeNode` | struct | [config/library_draft/mod.rs](../../../crates/gitserious-app/src/config/library_draft/mod.rs) | `gitserious-tui` | `gitserious-tui` |
| `edit_configuration` | fn | [config/taxonomy_configuration_editor.rs](../../../crates/gitserious-app/src/config/taxonomy_configuration_editor.rs) | `gitserious-cli` | `gitserious-app`, `gitserious-cli` |
| `fingerprint_taxonomy` | fn | [config/taxonomy_policy.rs](../../../crates/gitserious-app/src/config/taxonomy_policy.rs) | No external source candidate | `gitserious-app` |
| `DirectoryCreator` | trait | [directory_creator.rs](../../../crates/gitserious-app/src/directory_creator.rs) | `gitserious-fs` | `gitserious-fs` |
| `GlobalPathResolver` | trait | [global_path_resolver.rs](../../../crates/gitserious-app/src/global_path_resolver.rs) | `gitserious-fs` | `gitserious-app` |
| `GlobalPaths` | struct | [global_paths.rs](../../../crates/gitserious-app/src/global_paths.rs) | `gitserious-fs` | `gitserious-app`, `gitserious-fs` |
| `InitOutcome` | struct | [initialize_taxonomy_project.rs](../../../crates/gitserious-app/src/initialize_taxonomy_project.rs) | `gitserious-cli` | `gitserious-app`, `gitserious-cli`, `gitserious-fs` |
| `InitStatus` | enum | [initialize_taxonomy_project.rs](../../../crates/gitserious-app/src/initialize_taxonomy_project.rs) | `gitserious-cli` | `gitserious-app`, `gitserious-cli`, `gitserious-fs` |
| `InitializeProjectError` | enum | [initialize_taxonomy_project.rs](../../../crates/gitserious-app/src/initialize_taxonomy_project.rs) | No external source candidate | `gitserious-app`, `gitserious-fs` |
| `initial_project_configuration` | fn | [initialize_taxonomy_project.rs](../../../crates/gitserious-app/src/initialize_taxonomy_project.rs) | No external source candidate | `gitserious-app`, `gitserious-tui` |
| `initialize_project` | fn | [initialize_taxonomy_project.rs](../../../crates/gitserious-app/src/initialize_taxonomy_project.rs) | `gitserious-cli`, `gitserious-tui` | `gitserious`, `gitserious-app`, `gitserious-cli`, `gitserious-fs`, `gitserious-tui` |
| `HelpLine` | type | [interactive.rs](../../../crates/gitserious-app/src/interactive.rs) | `gitserious-cli` | `gitserious-cli`, `gitserious-tui` |
| `HelpPage` | struct | [interactive.rs](../../../crates/gitserious-app/src/interactive.rs) | `gitserious`, `gitserious-cli`, `gitserious-tui` | `gitserious`, `gitserious-cli`, `gitserious-tui` |
| `HelpSpan` | struct | [interactive.rs](../../../crates/gitserious-app/src/interactive.rs) | `gitserious-cli` | `gitserious`, `gitserious-cli`, `gitserious-tui` |
| `HomeState` | enum | [interactive.rs](../../../crates/gitserious-app/src/interactive.rs) | `gitserious-tui` | `gitserious`, `gitserious-tui` |
| `InteractiveUi` | trait | [interactive.rs](../../../crates/gitserious-app/src/interactive.rs) | `gitserious-tui` | `gitserious`, `gitserious-tui` |
| `InteractiveWorkspace` | trait | [interactive.rs](../../../crates/gitserious-app/src/interactive.rs) | `gitserious-tui` | `gitserious`, `gitserious-tui` |
| `run_interactive` | fn | [interactive.rs](../../../crates/gitserious-app/src/interactive.rs) | `gitserious` | `gitserious` |
| `RepositoryInitializer` | trait | [repository_locator.rs](../../../crates/gitserious-app/src/repository_locator.rs) | `gitserious-fs` | `gitserious` |
| `RepositoryLocator` | trait | [repository_locator.rs](../../../crates/gitserious-app/src/repository_locator.rs) | `gitserious-cli`, `gitserious-fs` | `gitserious-app`, `gitserious-cli`, `gitserious-fs` |
| `RepositoryRoot` | struct | [repository_locator.rs](../../../crates/gitserious-app/src/repository_locator.rs) | `gitserious-fs`, `gitserious-tui` | `gitserious-app`, `gitserious-cli`, `gitserious-fs`, `gitserious-tui` |
| `RepositoryRootError` | struct | [repository_locator.rs](../../../crates/gitserious-app/src/repository_locator.rs) | `gitserious-fs` | No test-source candidate |
| `resolve_global_paths` | fn | [resolve_global_paths.rs](../../../crates/gitserious-app/src/resolve_global_paths.rs) | `gitserious` | `gitserious-app`, `gitserious-fs` |
| `StorageDirectory` | struct | [storage_directory.rs](../../../crates/gitserious-app/src/storage_directory.rs) | `gitserious-fs` | No test-source candidate |

## Application operation boundaries

| Interface | Operations | Current producers and consumers |
|---|---|---|
| `InteractiveWorkspace` | `home`, `initialize`, `library_taxonomies`, `configuration`, `repository_status`, `prepare_commit`, `submit_commit` | Private `InteractiveServices` implements the facade. TUI root consumes all operations. Commit view directly consumes status and preparation. Binary integration callback UIs exercise the facade. |
| `InteractiveUi` | `run` | TUI `RatatuiInteractiveUi` implements it. App `run_interactive` invokes it. Binary composition supplies the concrete implementation. Integration tests supply callback UIs. |
| `ConfigurationWorkspace` | `load`, `save`, `library_path` | Private Config `Workspace` implements it. TUI config editor and review consume it through the facade or editor callback. App tests use `ConfigurationEditor` callbacks to reach it. |
| `ConfigurationEditor` | `edit` | TUI standalone taxonomy editor implements it. CLI Config fallback and App `edit_configuration` invoke the callback. App tests substitute scripts. |
| `RepositoryLocator` | `locate`, `is_not_repository`, associated `Error` | FS implements discovery. App commit, config, initialization, and interactive operations use it. CLI supplies invocation directories. Tests substitute locators. |
| `RepositoryInitializer` | `initialize` | FS implements Git initialization. Interactive initialization uses it only after a classified missing-repository result. Direct `initialize_project` does not accept this port. |
| `RepositoryStatusReader` | `read` | FS implements status reading. Interactive Home and Commit operations consume snapshots. Status refresh does not stage changes. |
| `CommitWriter` | `commit`, associated `Error` | FS implements Git writing. Interactive submission and the older commit use case consume it. Tests record writes or inject failure. |
| `CommitDraftAuthor` | `author`, `author_with_context`, associated `Error` | The older application commit use case invokes it. The unified shell uses embedded authoring instead. The old use case and author port remain tested and dead-code-allowlisted. |
| `ProjectConfigurationStore` | `load`, `create`, `replace`, `ensure_local_state`, associated `Error` | FS TOML store implements persistence. App commit, config, and initialization consume it. Revision and no-overwrite semantics live at this port. |
| `GlobalConfigurationStore` | `path`, `load`, `compare_and_swap`, associated `Error` | FS global TOML store implements persistence. App Config and interactive library queries consume it. CLI direct Init loads a snapshot before invoking initialization. |
| `GlobalPathResolver` | `resolve`, associated `Error` | FS system resolver implements it. App `resolve_global_paths` invokes it. Binary composition chooses the resolver. |
| `DirectoryCreator` | `ensure`, associated `Error` | FS storage-directory adapter implements it. The port remains independently tested. It is not a TUI lifecycle contract. |

## Use-case flow and removal constraints

The executable assembles FS adapters, a standalone configuration editor, and a CLI `ShellLauncher`. CLI currently parses and dispatches. Its shell request reaches binary `Application::open`, which maps `ShellStart` into TUI `InitialPage` and calls App `run_interactive`. App constructs interactive services and launches the supplied UI. Moving either dispatch or launch must preserve these current callback consumers until they migrate.

`prepare_commit_with_taxonomy` loads project-owned policy. `validate_reviewed_commit` checks the chosen taxonomy, draft rules, and exact reviewed bytes. The active facade's `submit_commit` prepares again and therefore reloads persisted policy and eligibility. `create_commit_with_taxonomy` prepares once and validates against its captured policy after its author returns. The existing captured-policy test is not evidence of freshness in the active path.

`edit_configuration` supplies a private `ConfigurationWorkspace` to `ConfigurationEditor`. Global save uses snapshot comparison and suppresses unchanged writes. Project save uses create or revision-guarded replacement and returns the new revision in its session. Removing the editor callback requires migrating the tests that reach the private workspace through it.

Direct `initialize_project` requires a repository and returns `AlreadyInitialized` without rewriting an existing policy. Interactive `initialize` can create Git after a classified missing repository, then attempts project creation from an explicit selection. Repeated interactive creation fails without overwriting the file. These are separate preserved contracts, not one workflow yet.

Help content is created by CLI, represented by App `HelpPage`, `HelpLine`, and `HelpSpan`, and rendered by TUI. Width-dependent rendering and strong styling belong to presentation despite currently passing through App. No relocation is performed by this inventory.

## Test ownership and source coupling

| Current mechanism | Evidence | Removal constraint |
|---|---|---|
| TUI imports App test support by source path | `crates/gitserious-tui/src/lib.rs` includes `../../gitserious-app/tests/support/library_lookup.rs` | F05 needs deliberate fixture ownership. A Cargo graph does not expose this coupling. |
| FS imports the same App helper | `crates/gitserious-fs/tests/global_configuration.rs` | Include this consumer when changing the helper's location. |
| Author implementation included in TUI tests | `crates/gitserious-tui/tests/unit/mod.rs` includes `author/state.rs` and `author/render.rs` | Preserve discovery and behavior tests while removing inclusion in F05. |
| Broad TUI fake services implement unrelated Config methods | `crates/gitserious-tui/src/shell/mod.rs` test module | Capability migration should narrow required test setup. AX does not invent a shared fake framework. |
| Config tests reach workspace through editor callbacks | `crates/gitserious-app/tests/configuration.rs`, `project_configuration.rs` | C07 must migrate these callers before callback removal. |
| Process fixtures own real Git and isolated homes | `crates/gitserious/tests/support/mod.rs` | Terminal fixtures stay executable-test owned. No reverse dependency from TUI tests to the executable. |

## Baseline behavior evidence

The following cases exist in source at the recorded baseline. Existence is not a passing test receipt. Each later PR records its checked head and command results. New AX test names should be added here when their implementations land rather than guessed ahead of time.

| Plan item | Owning layer and existing evidence | Gap or limit |
|---|---|---|
| A03 routing | CLI `commands_select_a_shell_page_and_commit_does_not_author`, `commit_opens_the_shell_on_the_commit_page`, `unknown_commands_and_extra_arguments_are_usage_errors` | Launcher-error presentation needs focused tests. |
| A04 Help | CLI `help_opens_the_shell_on_a_terminal_and_prints_when_redirected`; executable `help_wraps_each_paragraph_to_the_terminal_width_up_to_100_columns`, `each_help_page_shows_what_its_help_command_prints_at_the_same_width` | Injected terminal flags and redirected output do not prove actual PTY routing or pager-spawn fallback. |
| A05 entry | TUI `opening_commit_matches_navigating_there_and_does_not_author`, `a_help_entry_opens_help_in_the_sidebar_on_its_command_page`; executable nonterminal Commit tests | PR #278 already covers deterministic Commit equivalence. Real executable initial selection still needs evidence. |
| A06 eligibility | Executable `shell_services_gate_staging_validate_review_and_retry_git_rejection`; FS `real_status_preserves_index_and_reports_partial_staging_rename_untracked_and_detached`, `real_conflict_is_reported_without_refreshing_the_index` | Isolate the active application rejection matrix and eligibility loss before submission. |
| A07 freshness | App `missing_or_mismatched_reviewed_bytes_never_reach_git` protects the older use case | Active callback submission must observe policy changes. `approved_message_uses_the_captured_policy_even_if_the_project_changes` intentionally proves another contract. |
| A08 recovery | TUI `failed_submission_checkpoint_preserves_editor_and_exact_review_bytes`, `a_created_commit_returns_to_commit_and_a_rejected_one_keeps_the_review`; executable hook retry case | Pending-review navigation, actual resume cancellation, and no-commit-before-retry need explicit evidence. Hook rejection is Unix-only. |
| A09 configuration | App `reviewed_global_sessions_save_one_snapshot_and_reject_concurrent_changes`, `stale_or_failed_applies_leave_the_project_file_unchanged`, `first_save_creates_and_later_saves_replace_the_opened_revision`; FS `replace_refuses_a_stale_revision_and_keeps_the_external_edit` | Retry the same failed session and continue after a successful replacement. Do not duplicate create-then-replace coverage. |
| A10 initialization | App `existing_project_is_never_rewritten_even_when_the_library_changed`; executable `non_repository_fails_without_creating_project_state`, `home_initializes_git_and_gitserious_in_a_plain_directory`, `a_retry_after_a_partial_initialization_writes_only_the_project` | Repeated interactive create and non-missing locator failure need focused evidence. |

## Recheck the inventory

Read the crate-root `pub use` lists, follow each defining module, and search identifier references under `crates`. Classify inline test modules separately from runtime code. Reconcile generic names and callback uses manually. This is an inventory procedure, not a source-text architectural fitness check.

```sh
rg -n '^pub use' crates/gitserious-app/src/lib.rs crates/gitserious-app/src/commit/mod.rs crates/gitserious-app/src/config/mod.rs
rg -n 'InteractiveWorkspace|InteractiveUi|ConfigurationWorkspace|ConfigurationEditor|include!' crates
cargo test --locked -p gitserious-app -p gitserious-cli -p gitserious-tui -p gitserious --all-targets -- --list
```

Record a verification receipt as full head SHA, command, observed result, and artifact path. Do not mark Gate A complete from test discovery alone. Runtime tests, persistence tests, and required workspace checks must pass at the integration point.
