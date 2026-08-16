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

behavior:
report the invalid token
preserve its source location
```

Each property value begins at column 1. Rendering does not add indentation, so
any leading whitespace is authored content and is preserved exactly. Blank
nonrequired properties are omitted, repeatable values retain authored order,
and every rendered message ends with a newline.

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
