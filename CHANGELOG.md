# Changelog

## Unreleased

- Document taxonomy-first `gitserious config` for global libraries, project
  overrides, portable locks, and the Project repair path after a library change.
- Open Global default and available settings from Library (`configure global` / `c`).
- Author property multiplicity (`single` / `multiple`) in the configuration TUI.
- Refuse `gitserious init` when the project lock differs from the current library.
  Refresh through Project review instead.
- Print human requirement and multiplicity labels from `gitserious config show`.
- Restore taxonomy process and storage proofs for list, show, init, stale locks,
  and linked worktrees.
- Scaffolding pass for public v0.1.0 launch.

## [0.1.0] - TBD

- Add repository scaffolding, licensing metadata, and placeholder CLI entrypoint.
- Prepare package metadata and contribution defaults for initial release readiness.
- Define baseline `just` workflow and local scaffolding utilities for docs automation.
- Prepare internal component crates with unstable Rust APIs so the `gitserious`
  release binary has a reproducible crates.io dependency graph.

## [0.0.0] - Initial scaffold

- Initial workspace and crate layout.
