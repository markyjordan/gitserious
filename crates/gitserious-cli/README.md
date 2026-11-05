# gitserious-cli

This crate is an internal component of `gitserious`.

The Rust API exposed here is unstable and may have breaking changes in any
release. The supported public interface is the `gitserious` command-line tool.

It owns CLI parsing, command dispatch, and user-facing output coordination.

The taxonomy-first public surface is:

```text
gitserious
gitserious config
gitserious init
gitserious commit
```

Configuration belongs to the reviewed TUI.

Bare `gitserious` opens the unified Home TUI in a terminal and prints help when
redirected. Bare `config` enters the shell at Config; `commit` still enters
authoring directly. The shell Help pages receive CLI-owned presentation data.
