use crate::RepositoryRoot;

/// One commit the message can cite. `sha` is the abbreviated hash.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CitedCommit {
    pub sha: String,
    pub subject: String,
}

/// The worktree paths and commits a message can cite.
pub trait WorktreePaths {
    /// Files in `root`, with each parent directory.
    ///
    /// # Errors
    ///
    /// Returns the reader error when the worktree cannot be listed. The Commit
    /// page then offers no paths and still opens the editor.
    fn paths(&self, root: &RepositoryRoot) -> Result<Vec<String>, String>;

    /// Recent commits on `HEAD`, newest first.
    ///
    /// # Errors
    ///
    /// Returns the reader error when history cannot be listed. The Commit page
    /// then offers no commits and still opens the editor.
    fn commits(&self, _root: &RepositoryRoot) -> Result<Vec<CitedCommit>, String> {
        Ok(Vec::new())
    }
}
