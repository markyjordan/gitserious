# Configure taxonomy policy

Use this guide when you need repository commit policy that pins complete taxonomies, not a loose global default alone.

Commands assume a built `gitserious` binary on your `PATH`.

## Inspect installed taxonomies

List every installed taxonomy with origin, version, and whether it is the global default or available to new projects.

```text
gitserious config list
```

Show one taxonomy in full, including types and property requirement and multiplicity labels.

```text
gitserious config show research
```

`list` and `show` read the global library. They do not print project overrides.

## Open the configuration TUI

Start an interactive session from a Git work tree.

```text
gitserious config
```

Both stdin and stdout must be terminals. Press `m` to switch between Project and Library.

## Manage the global library

On Library, press `c` or run `configure global` from `/` commands.

Space toggles membership in the available set. Enter sets the default. The default must stay available. Alt+Up and Alt+Down reorder available entries.

Ctrl+S stages the draft. Ctrl+S again opens review. Enter applies. Esc cancels without writing.

Create, edit, fork, and delete custom taxonomies from Library. Built-ins are read-only. Fork them instead. Staging a taxonomy still leaves the available set unchanged until you edit Global settings.

Property forms include Policy and Multiplicity. Multiplicity is `single` or `multiple`. Left and Right change the focused choice.

## Initialize a project

From a repository root, create `gitserious.toml` and `gitserious.lock`.

```text
gitserious init
gitserious init --taxonomy research
```

The lock stores complete portable taxonomy snapshots for commit authoring.

## Refresh project policy after a library change

Global edits do not rewrite project locks by themselves. When the captured lock would differ, `gitserious init` refuses to replace the lock and prints a repair path.

1. Run `gitserious config`.
2. Open Project.
3. Review the update, then Apply or Refresh through the reviewed save path.

Commit authoring stays on the pinned lock until that review succeeds. A linked worktree uses its own root files. A stale lock there blocks commit on that worktree.

## Override project selection

On Project, open Configure when the project is initialized. Override the default, the available set, or both. Each field inherits from Global when you leave it unset.

Ctrl+S stages. Review, then Enter to apply. Project apply rewrites authored overrides and the portable lock together.

## Author a commit with pinned policy

```text
gitserious commit
gitserious commit --taxonomy research --type data
```

Without `--taxonomy`, authoring starts from the project lock default. Temporary taxonomy selection for one commit does not rewrite project files.
