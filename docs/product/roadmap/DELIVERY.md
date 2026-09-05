# v0.1 atomic delivery plan

The product and interface contract is [ROADMAP.md](ROADMAP.md). This ledger records
planned work and proposed messages, not claims that the features already exist.

## Working and publication rules

- Keep the existing worktree paths and exact topic branch names listed below.
- Before reusing a lane, compare its history with merged PRs and current `dev`.
  Preserve uncommitted material; old squash-merged commits are not a backlog to
  merge again. Refresh only the lanes needed for the next checkpoint.
- Each atomic commit implements one behavior or decision with the tests and
  documentation needed to establish it. Every intermediate commit must build and
  pass relevant checks. Do not separate inseparable reader/writer migrations.
- A separate test commit is appropriate for cross-workflow regression contracts,
  not for withholding the tests needed to establish a production change.
- Audit the staged diff, behavior, tests, docs, and every message property before
  committing. These complete messages are planned starting points; amend stale
  unpushed claims when implementation changes. Reassess conditional applicability.
- Wrap durable-property and breaking-change prose at 80 Unicode display columns.
  Preserve headings and blank lines. Generate provenance from actual resolved
  policy when supported; never fabricate a version or schema fingerprint.
- At each completed checkpoint, validate, commit, push the existing topic branch,
  and open a PR into `dev` with behavior, evidence, and dependencies. Verify local
  HEAD, the live pushed branch, and PR head match before handing off the link.
- PRs are for maintainer review. Dependency checkpoints must merge into `dev`
  before dependent implementation integrates them. No self-approval or automatic
  merge is authorized by this delivery plan.
- Refresh dependent topics from the merged integration state. Avoid cross-topic
  merges and repeated cherry-picking of unfinished work. Later batches in a reused
  worktree receive new PRs after the earlier checkpoint merges.
- Do not create empty commits for verification, publication, or worktree refreshes.

## Checkpoints and ownership

| Checkpoint | Worktree | Existing topic branch | Commits | Depends on |
| --- | --- | --- | --- | --- |
| A | DOCS | `marky/docs/project-docs` | D1 | None |
| B | CORE | `marky/feat/core` | K1-K3 | D1 |
| C1 | COMMIT-TYPE | `marky/feat/commit-type` | B1-B2 | K1 |
| C2 | CONFIG | `marky/feat/config` | F1-F2 | K1 |
| D1 | CONFIG | `marky/feat/config` | F3-F6 | F1-F2 |
| D2 | COMMIT | `marky/feat/commit` | C1-C2 | F2, K2-K3 |
| E | TUI | `marky/feat/tui` | U1-U3 | B1-B2, C1-C2 |
| F | README | `marky/docs/readme` | R1-R2 | F6, U3 |
| F | DOCS | `marky/docs/project-docs` | D2 | F6, U3 |
| F | RELEASE | `marky/chore/release-script-tests` | L1-L3 | Product, then final documentation |
| G | DEV, main, release/0.1 | Existing promotion process | Verification/publication | All preceding checkpoints |

Worktree labels refer to the existing sibling `gitserious-<LABEL>` directories.
IDs in dependency lists identify commits; checkpoint names only group PRs. Commits
within a checkpoint stay ordered, and downstream work consumes the merged
checkpoint even if its direct prerequisite is an earlier commit in that batch.

Critical path: CORE -> CONFIG backend -> COMMIT -> TUI -> release verification.
COMMIT-TYPE and CONFIG backend can proceed independently. After F2, CONFIG's TUI
and COMMIT can proceed independently. Documentation drafting and release-tooling
inspection may start early; final walkthroughs wait for integrated behavior.

Ownership boundaries:

- CORE owns generic models, validation, and canonical rendering. COMMIT-TYPE owns
  concrete built-in vocabulary, guidance, and schema fixtures.
- CONFIG owns configuration end to end, including its separate TUI module, CLI
  dispatch, lock persistence, and the related `init` migration in F2.
- COMMIT owns application orchestration and commit CLI dispatch. Include minimal
  adapter updates needed to keep intermediate commits buildable. K2 introduces
  the shared path without breaking adapters that have not yet adopted it.
- TUI owns commit authoring interaction. CONFIG screens stay separate from the
  existing authoring module; sequence shared exports and binary wiring via `dev`.
- RELEASE owns changelog and release-mechanics corrections. README and DOCS avoid
  competing edits there. DEV integrates; fixes return through the owning topic.

## Acceptance by concern

- Catalog: complete schemas, reserved identities, deterministic ordering, and
  unchanged Conventional definitions.
- Configuration: both destinations, arbitrary bundle forks, invalid dependencies,
  import conflicts, cancellation, concurrent saves, and atomic lock upgrades.
- Commit: overlapping type identifiers, temporary overrides, global independence,
  and exact reviewed-message persistence with correct provenance.
- TUI: all requirement levels, explicit applicability, repeated values, discard
  confirmation, long custom names, and structural label collisions.
- Release: fresh-install walkthroughs, repository-required quality checks, all six
  workspace crates in dependency order, four native targets, and the existing
  mandatory release rehearsal and publication approvals.

INIT and XDG capabilities are reused; required lock/init changes belong to F2.
CI, AUDIT, SECURITY, and DEVENV activate only for concrete blockers. COMMUNITY is
optional polish. EVALS starts after v0.1, MCP v2 belongs in a v0.1 patch, and Lore
belongs to v0.2. These lanes do not receive artificial placeholder commits.

## Planned atomic commit messages

Each entry includes a complete message under the existing Conventional typeset.
The behavior is specified by the subject, durable properties, and roadmap. Include
schema fixtures with each new domain bundle, filesystem and migration changes
with F2, application/CLI wiring with F3 and C1, and review/Git integration with C2.

### D1: DOCS

Depends on: None.

```text
docs(roadmap): define the v0.1 commit and configuration release

reason:
The earlier roadmap describes enforcement and distribution assumptions that no
longer match the selected release. Record the commit workflow, three default
bundles, and custom configuration as the release boundary, with evals
afterward, MCP v2 in a patch, and Lore in v0.2.
```

### K1: CORE

Depends on: D1.

```text
refactor(core): represent built-ins as a configuration catalog

motivation:
A singleton Conventional bundle prevents additional built-ins from using the
existing configuration model.

decision:
Represent built-ins as collections of taxonomies, typesets, and templates, and
resolve reserved identities through that catalog.

invariant:
Preserve the Conventional definitions, ordering, and default template
identity, including existing project resolution.
```

### K2: CORE

Depends on: K1.

```text
feat(core): validate explicit property applicability in drafts

intent:
Custom typesets need consistent requirement semantics throughout authoring and
commit validation.

decision:
Carry conditional applicability with authored responses and use the shared
property validator for required, recommended, conditional, and repeatable
properties.

constraints:
Keep existing adapters operational until they adopt explicit responses.
Recommended omissions remain warnings rather than blocking errors.
```

### K3: CORE

Depends on: K2.

```text
feat(core): render resolved schema provenance in commit messages

intent:
A commit type alone cannot identify its meaning when multiple taxonomies reuse
the same identifier.

decision:
Render template, taxonomy, typeset versions, and a resolved schema fingerprint
as deterministic Gitserious trailers.

constraints:
Keep provenance separate from authored properties. Preserve message body
wrapping and breaking-change rendering, and keep structural trailers on single
lines.
```

### B1: COMMIT-TYPE

Depends on: K1.

```text
feat(taxonomy): add the ml-research default bundle

intent:
Research commits need to preserve hypotheses, experimental choices, and
interpretations that software change categories do not distinguish.

decision:
Add the agreed ml-research taxonomy, default typeset, and template through the
generic built-in catalog.

constraints:
Preserve the agreed property ordering and requirement levels, including the
required result for reproduction work. Leave Conventional unchanged.
```

### B2: COMMIT-TYPE

Depends on: B1.

```text
feat(taxonomy): add the infra-ops default bundle

intent:
Operational changes need to preserve deployment, recovery, capacity, and risk
context using categories suited to those decisions.

decision:
Add the agreed infra-ops taxonomy, default typeset, and template through the
generic built-in catalog.

constraints:
Use the agreed complete schemas without adding taxonomy composition or
additional utility types. Leave existing bundles unchanged.
```

### F1: CONFIG

Depends on: K1.

```text
feat(config): fork arbitrary template bundles

intent:
Users need editable starting points from every available template.

decision:
Generalize forking to copy a selected template and its taxonomy and typeset
under new custom identities in the requested destination.

constraints:
Keep built-ins immutable, preserve dependency relationships, and reject
identity collisions without partially saving the copied bundle.
```

### F2: CONFIG

Depends on: F1.

```text
feat(config): lock every selectable project template

intent:
Per-commit selection must use reproducible project policy while keeping the
project's active template as its default.

decision:
Resolve built-ins and project custom templates into the generated lock, and
upgrade existing locks through the initialization workflow.

constraints:
Keep authored configuration compatible and project policy independent of
mutable global definitions. Update configuration and lock state atomically,
preserving existing files when migration fails.
```

### F3: CONFIG

Depends on: F2.

```text
feat(config): open a global and project configuration browser

intent:
Users need to discover their configuration and understand which storage
destination an operation will affect.

decision:
Open a configuration TUI from the bare config command with explicit global and
project destinations, definition inspection, and a shared editing session that
reviews changes before applying them.

constraints:
Retain existing explicit CLI subcommands. Browsing and cancellation must not
alter stored configuration.
```

### F4: CONFIG

Depends on: F3.

```text
feat(config): author custom taxonomies in the configuration TUI

intent:
Users need to define change categories without manually editing TOML.

decision:
Add creation, editing, ordering, and deletion of custom taxonomy definitions
to the reviewed configuration session.

constraints:
Keep identities immutable, advance versions for semantic changes, and reject
invalid or dangling references before saving.
```

### F5: CONFIG

Depends on: F4.

```text
feat(config): author custom typesets in the configuration TUI

intent:
Users need to choose which durable information each change type asks an author
to preserve.

decision:
Provide editing for property keys, descriptions, ordering, requirement levels,
conditions, and multiplicity within taxonomy-bound typesets.

constraints:
Require complete taxonomy coverage, including intentional empty schemas.
Review related edits together so intermediate drafts need not be saved as
inconsistent configuration.
```

### F6: CONFIG

Depends on: F5.

```text
feat(config): manage template bundles in the configuration TUI

intent:
Users need to assemble reusable templates and bring selected global
configuration into repository-owned policy.

decision:
Expose template creation, editing, forking, import, selection, and deletion
through the reviewed configuration session.

constraints:
Import complete dependency chains without silent conflict replacement. Allow
import alone or import-and-select, and protect referenced definitions and the
active project template from invalid deletion.
```

### C1: COMMIT

Depends on: F2, K2-K3.

```text
feat(commit): resolve a template for each commit session

intent:
An author needs to choose a suitable template for one change without
reconfiguring the repository.

decision:
Carry resolved template identity through authoring and validation, add the
template option, and resolve type preselection within the selected template.

constraints:
Default to the active project template. Permit built-ins and project custom
templates only, and leave project configuration unchanged.
```

### C2: COMMIT

Depends on: C1.

```text
feat(commit): bind reviewed messages to their selected schema

intent:
The stored message must identify the schema used to interpret and validate its
properties.

decision:
Build provenance from the selected resolved template and use the same
canonical message for review and Git commit creation.

constraints:
Do not accept authored provenance overrides or re-resolve the message against
another template after review. Preserve ordinary Git hooks, signing behavior,
and cancellation.
```

### U1: TUI

Depends on: B1-B2, C1-C2.

```text
feat(tui): switch templates during commit type selection

intent:
Authors need to move between software, research, operational, and custom
change vocabularies within the commit workflow.

decision:
Display available templates and their resolved change types, starting with the
project default or explicit CLI selection.

constraints:
Distinguish templates that share a taxonomy or type identifier. Confirm
discard before a template change resets an edited draft.
```

### U2: TUI

Depends on: U1, K2.

```text
feat(tui): require explicit conditional applicability

intent:
The composer must distinguish an inapplicable property from a required
response that the author has left unanswered.

decision:
Provide applicable and not-applicable choices and use shared validation to
present blocking errors and nonblocking recommendations.

constraints:
Require a value for applicable conditions and reject contradictory values for
inapplicable conditions. Preserve explicit decisions when moving between
composition and review.
```

### U3: TUI

Depends on: U2.

```text
feat(tui): edit repeated property values

intent:
Custom typesets need a usable way to collect multiple independently authored
values for one property.

decision:
Allow adding, editing, and removing occurrences while preserving their
authored order through validation and canonical rendering.

constraints:
Prevent duplicates for single-valued properties and retain structural field
identity when custom names overlap editor labels.
```

### R1: README

Depends on: F6, U3.

```text
docs(readme): explain installation and template-based commits

reason:
The root README does not provide a usable path from installation to a
structured commit. Add verified examples for initialization, template
selection, composition, review, and the three built-in bundles.
```

### R2: README

Depends on: R1.

```text
docs(readme): explain reusable and project-local configuration

reason:
Users need to understand where custom definitions are stored and how global
templates become available in a project. Add verified walkthroughs for
configuration authoring, import, default selection, and temporary per-commit
selection.
```

### D2: DOCS

Depends on: F6, U3.

```text
docs(architecture): explain template resolution and commit provenance

reason:
The architecture documentation does not explain how reusable global
definitions become project-owned policy or how a temporary template selection
remains attributable in history. Document those boundaries and the
responsibilities of the domain, application, and adapters.
```

### L1: RELEASE

Depends on: B1-B2, F6, U3.

```text
test(release): verify the v0.1 installation contract

risk:
Release artifacts can pass superficial checks while omitting a workspace
dependency or failing the documented installed workflow.

rationale:
Exercise the current six-crate dependency order and native artifact contract,
including executable help, initialization, and configuration inspection from a
fresh installation.
```

### L2: RELEASE

Depends on: L1.

```text
docs(release): align publication guidance with the workspace

reason:
Release guidance still describes an earlier crate set and publication details.
Reconcile it with the current dependency-ordered packaging, native artifacts,
rehearsal process, and existing approval gates.
```

### L3: RELEASE

Depends on: L2, R2, D2.

```text
docs(changelog): describe the v0.1 commit and configuration release

reason:
The changelog describes scaffolding rather than the release users will
install. Record the completed commit and configuration workflows, their
supported boundaries, and the finalized release date.
```
