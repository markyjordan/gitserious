# gitserious-core

This crate owns validated commit-message values, taxonomy aggregates, durable
property rules, canonical rendering, and built-in authoring definitions. The
Rust API is internal and unstable; the supported interface is the CLI.

## Taxonomies

One taxonomy owns its ordered commit types and every type's ordered durable
properties. Commit types may intentionally define no durable properties.
Taxonomy identities are immutable, semantic edits advance the taxonomy version,
and optional fork lineage records only the source snapshot.

The built-in taxonomy order is:

1. `conventional`
2. `research`
3. `infra-ops`

All three are available by default, with Conventional selected. The `research`
and Infra Ops definitions retain the established domain-specific categories and
property requirements.

## Canonical messages

The Conventional Commit envelope remains `type(scope): subject`, followed by
schema-ordered property sections and an optional uppercase breaking-change
footer. Prose wraps to 80 Unicode display columns without splitting grapheme
clusters unnecessarily.

Taxonomy-aware rendering validates against the exact selected snapshot and
appends two structural trailers:

```text
Gitserious-Taxonomy: <id>@<version>
Gitserious-Schema: sha256:<taxonomy-fingerprint>
```

The fingerprint covers taxonomy identity, version, description, fork lineage,
commit-type schemas, property order, requirements, conditions, descriptions,
and multiplicity.
