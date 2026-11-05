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

Help has two tiers. `-h` prints the concise help, through `$PAGER` or `less`
when stdin and stdout are terminals; `less` prints help that fits one screen
and exits. In a terminal, `--help` and `gitserious help [<command>]` open the
shell at Help, on the command's page or the Overview. Redirected,
`gitserious <command> --help` and `gitserious help [<command>]` print the
detailed help, and `gitserious --help` prints the concise help. Printed help
wraps to the terminal width, up to 100 columns. Every usage line shows
`[OPTIONS]`, and `-h` and `--help` are listed under `Global options:` because
every command takes them. The shell Help pages show what `gitserious -h` and
`gitserious help <command>` print at the pane's width, with headings and names
in bold.
