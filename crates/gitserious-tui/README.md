# gitserious-tui

This crate is an internal component of `gitserious`.

The Rust API exposed here is unstable and may have breaking changes in any
release. The supported public interface is the `gitserious` command-line tool.

This crate implements terminal interaction ports with Ratatui. It owns terminal
setup, rendering, event handling, and restoration; commit policy and workflow
coordination remain in `gitserious-app`.
