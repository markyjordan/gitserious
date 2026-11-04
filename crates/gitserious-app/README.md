# gitserious-app

This crate owns taxonomy policy, application workflows, persistence ports, and
semantic fingerprinting. Its Rust API is internal and unstable; the supported
interface is the `gitserious` CLI.

## Taxonomy policy

A taxonomy is the complete selectable authoring aggregate: identity, semantic
version, description, optional fork lineage, ordered commit types, and each
type's ordered durable properties. Built-ins are immutable. Custom taxonomies
live only in the global user library. A fork is a version-one snapshot with
informational source identity and version.

Global configuration contains the default taxonomy, ordered available set, and
custom library.

## Self-contained project configuration

`ProjectConfiguration` is a project default plus the complete ordered taxonomy
definitions the project offers. Its constructor is the whole resolver: it checks
the format version, a nonempty set of unique identities, and default membership,
without reading the Library or the built-ins. A project may hold its own copy of
a built-in identity, and that copy wins. Initialization copies the Library's
default environment once; later Library edits never reach an existing project.

Configuration sessions stage one persistence destination at a time. Global
changes use whole-snapshot compare-and-swap. A Project session opens without the
Library and remembers the `ProjectRevision` it read. Apply writes
`gitserious.toml` through `ProjectConfigurationStore` only while that revision is
still on disk, so an external edit is never overwritten. Review is required
before the adapter is called, and save failures retain the draft.
