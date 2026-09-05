# gitserious-app

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
