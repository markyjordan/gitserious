use crate::RepositoryRoot;

/// The worktree paths a commit message can cite.
pub trait WorktreePaths {
    /// Files in `root`, with each parent directory.
    ///
    /// # Errors
    ///
    /// Returns the reader error when the worktree cannot be listed. The Commit
    /// page then offers no paths and still opens the editor.
    fn paths(&self, root: &RepositoryRoot) -> Result<Vec<String>, String>;
}
