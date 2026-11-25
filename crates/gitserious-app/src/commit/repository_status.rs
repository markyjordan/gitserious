use std::path::PathBuf;

use crate::RepositoryRoot;

/// Git's current branch identity, including repositories without a first commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BranchState {
    Named(String),
    Unborn(String),
    Detached(String),
}

/// One changed path; index and worktree codes are independent for partial staging.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusEntry {
    pub path: PathBuf,
    pub original_path: Option<PathBuf>,
    pub index: char,
    pub worktree: char,
    pub untracked: bool,
    pub conflicted: bool,
    pub submodule: String,
}

/// A Git operation stopped partway, which decides what kind of commit `git commit` makes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitOperation {
    Merge,
    CherryPick,
    Revert,
    Rebase,
}

impl GitOperation {
    /// The Git command that continues or aborts the operation.
    #[must_use]
    pub const fn command(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::CherryPick => "cherry-pick",
            Self::Revert => "revert",
            Self::Rebase => "rebase",
        }
    }
}

/// Read-only worktree snapshot used by the Commit overview.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryStatus {
    pub root: RepositoryRoot,
    pub branch: BranchState,
    pub entries: Vec<StatusEntry>,
    pub operation: Option<GitOperation>,
}

impl RepositoryStatus {
    #[must_use]
    pub fn has_staged_changes(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| !entry.untracked && entry.index != '.' && !entry.conflicted)
    }

    /// The operation during which `git commit` would write a commit gitserious does not
    /// author. A rebase stop makes a normal commit, so it never counts.
    #[must_use]
    pub fn foreign_commit_operation(&self) -> Option<GitOperation> {
        self.operation
            .filter(|operation| *operation != GitOperation::Rebase)
    }

    /// Why authoring cannot start in this state: an operation whose commit gitserious does
    /// not author, or a rebase stopped on conflicts.
    #[must_use]
    pub fn authoring_refusal(&self) -> Option<String> {
        if let Some(operation) = self.foreign_commit_operation() {
            return Some(operation_refusal(operation));
        }
        let conflicts = self.conflicted_paths();
        (self.operation == Some(GitOperation::Rebase) && !conflicts.is_empty()).then(|| {
            let paths: Vec<_> = conflicts
                .iter()
                .map(|path| path.to_string_lossy())
                .collect();
            format!(
                "A rebase stopped on conflicts in {}. Resolve them, then run `git rebase --continue`.",
                paths.join(", ")
            )
        })
    }

    /// The unmerged paths, in Git's order.
    #[must_use]
    pub fn conflicted_paths(&self) -> Vec<PathBuf> {
        self.entries
            .iter()
            .filter(|entry| entry.conflicted)
            .map(|entry| entry.path.clone())
            .collect()
    }
}

/// Names `operation` and how to finish or stop it, since its commit is not one gitserious
/// authors.
#[must_use]
pub(crate) fn operation_refusal(operation: GitOperation) -> String {
    let command = operation.command();
    format!(
        "A {command} is in progress, and gitserious does not author its commit. \
         Finish it with `git {command} --continue` or stop it with `git {command} --abort`."
    )
}

/// Reads Git status without modifying the working tree or index.
pub trait RepositoryStatusReader {
    fn read(&self, root: &RepositoryRoot) -> Result<RepositoryStatus, String>;
}
