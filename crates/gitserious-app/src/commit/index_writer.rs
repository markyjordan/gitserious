use std::path::PathBuf;

use crate::RepositoryRoot;

/// Changes what the next commit contains. It is the only port that writes the index.
pub trait IndexWriter {
    /// Stages every change to tracked files, deletions included.
    ///
    /// # Errors
    ///
    /// Returns Git's message when the index cannot be written.
    fn stage_tracked(&self, root: &RepositoryRoot) -> Result<(), String>;

    /// Stages `paths` as the worktree holds them, untracked files and deletions included.
    ///
    /// # Errors
    ///
    /// Returns Git's message when the index cannot be written.
    fn stage(&self, root: &RepositoryRoot, paths: &[PathBuf]) -> Result<(), String>;

    /// Returns `paths` in the index to `HEAD`, or drops them from the index of a branch with
    /// no commit yet. A rename unstages by naming both of its paths.
    ///
    /// # Errors
    ///
    /// Returns Git's message when the index cannot be written.
    fn unstage(&self, root: &RepositoryRoot, paths: &[PathBuf]) -> Result<(), String>;
}
