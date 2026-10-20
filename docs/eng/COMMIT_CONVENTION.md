# Gitserious commit convention

Every commit in this repository uses the gitserious format that
[PR #73](https://github.com/markyjordan/gitserious/pull/73) established. The
convention is provisional. Change this file, not individual commits, when the
convention changes.

## Commit shape

A commit message has a subject line, a blank line, and one section for each
property that the commit type defines.

```text
fix(ci): explain missing automation prerequisites

cause:
Bare command discovery exits silently when verification tools are absent,
leaving contributors unable to distinguish setup failures from audit results.

decision:
Report the missing tool and its version-owning workflow before exiting.
Exercise absent prerequisites and failing installed tools in isolation.

constraints:
Preserve failure propagation from available tools and existing Rust guards.
```

- The subject is `<type>(<scope>): <summary>`. The scope is optional. The
  summary starts with a lowercase imperative verb and has no trailing period.
- `<type>` is a commit type from the built-in `conventional` taxonomy.
- Each section starts with the property key and a colon on its own line. The
  response follows as plain sentences on the next lines.
- One blank line separates sections. Wrap every line at 80 columns.
- Commits carry no trailers, such as `Co-Authored-By`, and no tool
  attribution.

## Property keys come from the taxonomy

The `conventional` taxonomy is the only list of commit types and property
keys. To print each type with its properties in order, run this command:

```sh
gitserious config show conventional
```

- Write the sections in the order that the taxonomy lists them.
- Always write a `Required` property.
- Write a `Conditional` property only when its rationale applies. For
  example, a `fix` includes `constraints` only when the obvious fix was
  unacceptable or specific behavior must be preserved.
- Write only keys that the commit type defines. A `docs` commit has only
  `reason`, so it has no `decision` or `constraints` section.

## One change per commit

Split a pull request into atomic commits, and give each commit the type that
fits its change. PR #73 has `fix`, `test`, and `ci` commits side by side. A
test that covers a fix goes in its own `test` commit, with `risk` and, when
the scenario is unusual, `rationale`.

## Pull request titles

The pull request title uses the subject format. A squash merge appends
` (#<number>)` and lists each commit as `* <subject>` followed by its body, so
the merged commit keeps every section.
