# gitserious-cli

This crate is an internal component of `gitserious`.

The Rust API exposed here is unstable and may have breaking changes in any
release. The supported public interface is the `gitserious` command-line tool.

It owns CLI parsing, command dispatch, and user-facing output coordination.

The taxonomy-first public surface is:

```text
gitserious
gitserious init
gitserious config
gitserious commit
gitserious help [<command>]
```

Configuration belongs to the reviewed TUI.

Bare `gitserious` opens the unified Home TUI in a terminal and prints the
concise help when redirected. Bare `config` enters the shell at Config; `commit`
still enters authoring directly.

Help has two tiers. `-h` and `--help` print the same concise help.
`gitserious help [<command>]` prints the detailed help, through `$PAGER` or
`less` when stdout is a terminal; `--no-pager` prints it directly. The shell
Help pages show the same text the CLI prints.
