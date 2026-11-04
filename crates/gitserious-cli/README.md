# gitserious-cli

This crate is an internal component of `gitserious`.

The Rust API exposed here is unstable and may have breaking changes in any
release. The supported public interface is the `gitserious` command-line tool.

It owns CLI parsing, command dispatch, and user-facing output coordination.

The taxonomy-first public surface is:

```text
gitserious
gitserious config
gitserious config list
gitserious config show <taxonomy>
gitserious config verify --project
gitserious init [--taxonomy <taxonomy>]
gitserious commit [--taxonomy <taxonomy>] [--type <type>]
```

Configuration mutations belong to the reviewed TUI. The list and show commands
remain read-only for scripts and inspection.

`config verify --project` checks the repository's `gitserious.toml` without
writing anything, reading the Library, or needing a terminal, so any CI system
can run it. It exits 0 when the file is valid, 1 when it is invalid or missing,
2 for a usage error, and 3 when the repository or file cannot be read.

Bare `gitserious` opens the unified Home TUI in a terminal and prints help when
redirected. Bare `config` enters the shell at Config; `commit` still enters
authoring directly. The shell Help pages receive CLI-owned presentation data.
