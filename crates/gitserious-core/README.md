# gitserious-core

This crate is an internal component of `gitserious`.

The Rust API exposed here is unstable and may have breaking changes in any
release. The supported public interface is the `gitserious` command-line tool.

This crate owns the core domain model for Git workflow policy, commit-message
taxonomies, templates, validation, and canonical message rendering.

## Canonical commit messages

Validated drafts render their header and schema-defined properties in schema
order:

```text
feat(parser): reject invalid tokens

intent:
make parser failures actionable

decision:
report the invalid token
preserve its source location
```

Each property value begins at column 1. Rendering does not add indentation, so
any leading whitespace is authored content and is preserved exactly. Blank
nonrequired properties are omitted, repeatable values retain authored order,
and every rendered message ends with a newline. Property and breaking-change
prose wraps at 80 Unicode display columns. Wrapping prefers authored whitespace,
breaks an overlong token only at grapheme boundaries, and preserves explicit
authored line breaks. Conventional Commit headers and property labels remain
structural single lines.

Internal whitespace in a scope is normalized to a single hyphen only when the
canonical message is rendered. For example, an authored `tui editor` scope is
reviewed and written as `tui-editor`; the typed draft retains the authored
scope.

Drafts may also carry an optional breaking-change description. When present,
the canonical renderer adds `!` immediately before the header colon and emits
the Conventional Commits footer after the body:

```text
feat(parser)!: replace the token API

BREAKING CHANGE: callers must use TokenStream
remove calls to the legacy parser
```

The first line follows the uppercase footer token, multiline continuation text
remains unindented, and blank breaking-change fields are omitted entirely.

## Built-in catalog

`built_in_configuration()` exposes ordered `taxonomies()`, `typesets()`, and
`templates()` collections and identity-based lookups. Typeset lookup includes
the taxonomy identity. The singular `taxonomy()`, `typeset()`, and `template()`
accessors continue to identify the original Conventional/default bundle;
catalog consumers must use the collections to discover all built-ins.

## Explicit property responses

`CommitDraft::from_responses` preserves `PropertyResponse` values and explicit
conditional applicability, including responses that have no rendered value.
`validate_commit_draft_report` returns blocking errors and nonblocking
recommended-property warnings from the shared property validator. Canonical
rendering revalidates the draft and refuses invalid applicability, unknown
properties, missing required values, or incorrect multiplicity. Repeatable
values remain in authored order within schema-ordered properties.

An applicable conditional property requires a value; a not-applicable property
rejects a value. An omitted conditional decision blocks response-based drafts,
even if all other responses are complete. `CommitDraft::new` remains the legacy
constructor for existing adapters and does not require conditional decisions.
An empty explicit response list still opts into the stricter contract. Adapter
migration to explicit responses belongs to the subsequent commit/TUI work.

## Schema provenance

`CommitProvenance` owns one immutable `ResolvedTaxonomy` and its semantic
`Fingerprint`. The application must compute that fingerprint from the exact
resolved schema. Core does not hash policy or independently verify the digest.
The fingerprint value is shared with the application's existing public re-export;
its `sha256:` representation and application-level computation remain unchanged.

`render_commit_message_with_provenance(&provenance, &draft)` selects the draft's
change type within that schema, validates it, and appends these four trailers:

```text
Gitserious-Template: <id>@<version>
Gitserious-Taxonomy: <id>@<version>
Gitserious-Typeset: <taxonomy>/<typeset>@<version>
Gitserious-Schema: sha256:<resolved-schema-fingerprint>
```

The same schema supplies validation and trailer identities, so overlapping type
names cannot select a definition from another template. The trailers follow the
canonical message after a blank line, in this fixed order, with a final newline.
They remain unwrapped structural lines; body and breaking-change prose retains
its existing 80-column wrapping. Identity and version values come from resolved
policy, never authored property fields. Provenance identifies the schema rather
than validating the truth of the author's statements.

The existing renderer remains available and does not add provenance implicitly.
Runtime use of the new renderer belongs to the subsequent COMMIT checkpoint.
