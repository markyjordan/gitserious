# gitserious-app

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
