# v0.1 atomic delivery plan

The product contract is [ROADMAP.md](ROADMAP.md). This ledger records the
taxonomy-first delivery sequence for the first release.

## Working and publication rules

- Preserve the existing worktree paths and exact topic branch names.
- Keep each commit buildable and pair behavior with the tests that establish it.
- Audit the staged diff, tests, documentation, and complete Gitserious commit
  message immediately before each signed commit.
- Push and open PRs only when explicitly requested. Never self-approve trusted
  automation.

## Dependency order

1. Core taxonomy aggregate and two-trailer provenance.
2. Application policy, portable locks, and persistence ports.
3. Strict taxonomy-first global/project TOML adapters.
4. Taxonomy-only CLI, initialization, and commit selection.
5. Shared terminal visual system and the configuration screen stack.
6. Documentation, process tests, and release verification.

## Ownership boundaries

- Core owns taxonomy and property invariants plus canonical message rendering.
- Application owns global/project resolution, staged sessions, semantic
  fingerprints, portable policy, and persistence ports.
- Filesystem owns strict TOML, safe paths, compare-and-swap, atomic replacement,
  and rollback.
- CLI owns taxonomy-only commands and actionable policy diagnostics.
- TUI owns terminal input, responsive rendering, screen navigation, and reviewed
  interaction. It does not implement persistence rules.
- The Git writer remains responsible only for committing the exact approved
  message through ordinary Git hooks and signing behavior.

## Planned commits

### Taxonomy policy

```text
feat(config): replace template bundles with taxonomy policy

intent:
Users should configure one understandable commit taxonomy rather than coordinate
taxonomies, typesets, and templates as independent resources.

decision:
Make taxonomy the complete aggregate, resolve global defaults with per-field
project overrides, and pin complete available taxonomies in the project lock.

constraints:
Keep built-ins immutable, forks snapshot-only, project locks portable, and
configuration saves guarded. Preserve the superseded source uncompiled for manual
verification.
```

### Terminal interface

```text
feat(tui): rebuild configuration around focused taxonomy views

intent:
Configuration should explain effective project behavior before exposing editing
controls.

decision:
Add project-first left navigation, focused taxonomy browsing, progressive detail,
full-screen CRUD children, and semantic review using the commit TUI visual system.

constraints:
Only review may write. Preserve draft state on errors, require explicit scope
transitions, support mouse and paste input, and block hidden actions when the
terminal is too small.
```

### Documentation and verification

```text
docs(config): document taxonomy-first project policy

reason:
The release documentation still described the superseded selectable template and
typeset model.

decision:
Document global library ownership, independent project overrides, portable locks,
taxonomy CLI selection, focused TUI navigation, and two-trailer provenance.
```

## Acceptance

- All built-ins resolve in order as `conventional`, `research`, and
  `infra-ops`.
- Global and project selection obey independent fallback and default-in-available
  invariants.
- Custom taxonomy CRUD and snapshot forking preserve ordering and versions.
- Portable locks author commits without the originating global custom taxonomy.
- Project-config/lock mismatches fail with the path through
  `gitserious config` → Project → Review/Apply.
- The TUI writes only after semantic review, preserves failed drafts, restores
  navigation position, and renders correctly at minimum and wide sizes.
- Locked tests, formatting, warnings-denied Clippy, rustdoc, `just ci`, diff
  checks, and manual PTY walkthroughs pass before publication.
