# gitserious-cli

This crate is an internal component of `gitserious`.

The Rust API exposed here is unstable and may have breaking changes in any
release. The supported public interface is the `gitserious` command-line tool.

It owns CLI parsing, command dispatch, and user-facing output coordination.

The taxonomy-first public surface is:

```text
gitserious config
gitserious config list
gitserious config show <taxonomy>
gitserious init [--taxonomy <taxonomy>]
gitserious commit [--taxonomy <taxonomy>] [--type <type>]
```

Configuration mutations belong to the reviewed TUI. The list and show commands
remain read-only for scripts and inspection.

`gitserious init` creates `gitserious.toml` and `gitserious.lock`. It also
creates a missing lock beside an existing config. When a library change would
capture a different lock, init leaves the existing files unchanged. The error
tells you to run `gitserious config`, open Project, and review Apply/Refresh.
