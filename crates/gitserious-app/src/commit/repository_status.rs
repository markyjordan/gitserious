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

/// Read-only worktree snapshot used by the Commit overview.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryStatus {
    pub root: RepositoryRoot,
    pub branch: BranchState,
    pub entries: Vec<StatusEntry>,
}

impl RepositoryStatus {
    #[must_use]
    pub fn has_staged_changes(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| !entry.untracked && entry.index != '.' && !entry.conflicted)
    }

    #[must_use]
    pub fn has_conflicts(&self) -> bool {
        self.entries.iter().any(|entry| entry.conflicted)
    }
}

/// Reads Git status without modifying the working tree or index.
pub trait RepositoryStatusReader {
    fn read(&self, root: &RepositoryRoot) -> Result<RepositoryStatus, String>;
}
