# Git writes

gitserious changes a repository's Git state through three application ports, each with one Git adapter in `gitserious-fs`. Every other Git call reads. Check this list before adding a Git command that is not a read.

| Port | Adapter | Writes | Git commands |
|---|---|---|---|
| [`RepositoryInitializer`](../../../crates/gitserious-app/src/repository_locator.rs) | [`GitRepositoryInitializer`](../../../crates/gitserious-fs/src/git.rs) | A new repository | `git init` |
| [`IndexWriter`](../../../crates/gitserious-app/src/commit/index_writer.rs) | [`GitIndexWriter`](../../../crates/gitserious-fs/src/git_index.rs) | The index | `git add --update`, `git add --all -- <paths>`, `git restore --staged -- <paths>`, and `git rm --cached -- <paths>` on a branch with no commit |
| [`CommitWriter`](../../../crates/gitserious-app/src/commit/commit_writer.rs) | [`GitCommitWriter`](../../../crates/gitserious-fs/src/git_commit.rs) | A commit of the staged index | `git commit --file <message> --cleanup=verbatim` |

`IndexWriter` is the only code that writes the index. It names paths literally (`--literal-pathspecs`), so a file named `a[1]` never stages `a1`, and it treats an empty path list as nothing to do rather than as the whole worktree.

The reads keep the index untouched as well. `GitRepositoryStatusReader` and `GitWorktreePaths` run with `GIT_OPTIONAL_LOCKS=0`, so a status read never refreshes the index or takes its lock.

`gitserious.toml` and `.gitserious/` are worktree files, not Git state. `TomlProjectStore` writes them, and Git sees them like any other file. Committing them is the developer's choice.
