# gitserious-fs

This crate is an internal component of `gitserious`.

The Rust API exposed here is unstable and may have breaking changes in any
release. The supported public interface is the `gitserious` command-line tool.

This crate implements the application-owned global path-resolution and
directory-creation ports. The global configuration directory follows the XDG
Base Directory convention on Unix-family targets and the native AppData
convention on Windows.

| Purpose | Unix-family targets | Native Windows |
| --- | --- | --- |
| Config | `$XDG_CONFIG_HOME/gitserious` | `%APPDATA%\gitserious\config` |

An unset, empty, or relative `XDG_CONFIG_HOME` falls back to
`$HOME/.config/gitserious`. Native Windows ignores XDG variables and uses the
Roaming AppData Known Folder.

The global `config.toml` stores the default taxonomy, ordered available set,
and global-only custom taxonomy aggregates. Repository `gitserious.toml` stores
only optional default and available overrides. Generated `gitserious.lock`
stores the complete resolved taxonomies so commit authoring remains portable.
All three files use the taxonomy-first version-one format and reject the earlier
pre-release `templates`, `typesets`, and `active-template` shape explicitly.

A binary compiled inside WSL is a Linux binary and therefore follows XDG. A
native Windows executable launched from WSL still follows Windows AppData.

Path resolution is side-effect free. Callers explicitly create only the
resolved directory needed immediately before a write; there is no eager
create-all operation. Newly created Unix directories request mode `0700`, while
existing permissions remain unchanged. Windows directories inherit native
ACLs.

Repository-local storage is established separately from these global
conventions.
