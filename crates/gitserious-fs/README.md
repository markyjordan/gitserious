# gitserious-fs

This crate is an internal component of `gitserious`.

The Rust API exposed here is unstable and may have breaking changes in any
release. The supported public interface is the `gitserious` command-line tool.

This crate implements the application-owned global path-resolution and
directory-creation ports. Global user storage follows XDG Base Directory
conventions on Unix-family targets and native AppData conventions on Windows.

| Purpose | Unix-family targets | Native Windows |
| --- | --- | --- |
| Config | `$XDG_CONFIG_HOME/gitserious` | `%APPDATA%\gitserious\config` |
| Data | `$XDG_DATA_HOME/gitserious` | `%APPDATA%\gitserious\data` |
| State | `$XDG_STATE_HOME/gitserious` | `%LOCALAPPDATA%\gitserious\state` |
| Cache | `$XDG_CACHE_HOME/gitserious` | `%LOCALAPPDATA%\gitserious\cache` |

Unset, empty, and relative XDG homes fall back beneath `HOME` to `.config`,
`.local/share`, `.local/state`, and `.cache`, respectively. Native Windows
ignores XDG variables and uses Known Folders.

A binary compiled inside WSL is a Linux binary and therefore follows XDG. A
native Windows executable launched from WSL still follows Windows AppData.

Path resolution is side-effect free. Callers explicitly create only the
resolved directory needed immediately before a write; there is no eager
create-all operation. Newly created Unix directories request mode `0700`, while
existing permissions remain unchanged. Windows directories inherit native
ACLs.

Repository-local storage is established separately from these global
conventions.
