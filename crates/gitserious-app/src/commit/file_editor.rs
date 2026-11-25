use std::path::Path;

use crate::RepositoryRoot;

/// Opens a file of the repository in the developer's editor and waits until it closes.
pub trait FileEditor {
    /// Edits `path`, relative to `root`, in the editor Git would use.
    ///
    /// # Errors
    ///
    /// Returns why the editor could not start or how it failed.
    fn edit(&self, root: &RepositoryRoot, path: &Path) -> Result<(), String>;
}
