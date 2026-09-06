# gitserious-app

## Commit template snapshots

`create_commit_with_template` selects an explicit template or the project's
active default after verifying the complete lock. Only built-ins and project
custom templates are eligible. The author receives a `CommitAuthoringContext`
containing immutable resolved choices and returns an `AuthoredCommit` carrying
the chosen template identity. The returned type is validated within that schema.
Type preselection belongs to the initial template and cannot silently move to
another template, even when both contain the same type identifier.

`create_commit` remains the no-override entry point. Neither path changes
project configuration or consults mutable global templates while authoring.

## Reviewed configuration sessions

`ConfigurationSession` retains the original snapshot and staged edits for one
explicit global or project destination. Staging validates identity and version
rules while permitting incomplete relationships between definitions. Review
validates the complete catalog and project policy. `ConfigurationWorkspace`
loads snapshots and saves the reviewed state with compare-and-swap against the
original snapshot, so concurrent changes are never silently overwritten.

`ConfigurationEditor` owns the interaction. Recoverable workspace errors stay
inside that interaction so the draft can be repaired or retained. Cancelling a
session has no persistence effect. A successful save returns a clean session
without rereading mutable global or project state.

Sessions can stage a complete bundle fork, import a global template chain into a
project, and choose a project default. Imports reuse exact matching definitions
and reject conflicting identities before changing either the draft or selection.
Deleting an active custom template requires staging another default before save.
Deleting and recreating an original identity within one session is rejected so
its version history cannot be reset.

Selectable lock entries use actual template identities (`default` for the
built-in Conventional template). The active compatibility summary retains its
historical `conventional` identity. This lets a custom template independently use
the valid name `conventional`. Earlier lock collections using the old alias remain
readable and refresh through `gitserious init` without rewriting authored config.

This internal crate owns application workflows, configuration snapshots,
persistence ports, and semantic fingerprint computation. The supported public
interface is the `gitserious` CLI; the internal Rust API remains unstable.

## Forking reusable configuration

`fork_configuration` forks any built-in or global custom template into new global
identities. `fork_project_template` forks a built-in or project-local custom
source into the same project's custom configuration without changing the active
template. Import global templates first when using them in a project.

A fork copies the complete taxonomy/typeset/template chain. Descriptions, type
order, schema order, requirements, and multiplicity are preserved. New identities
start at version one; source definitions remain unchanged. Source resolution and
destination saving share one inspected snapshot and guarded persistence rejects
concurrent changes. Conflicting or reserved target identities do not partially
save a bundle.

`fork_configuration_edits` builds a batch from a catalog snapshot for reviewed
editing sessions. Applying that batch validates destination identities and
references. The Conventional-only fork helpers remain compatible entry points.
CLI/TUI exposure of arbitrary source selection belongs to the later configuration
interface checkpoint.
