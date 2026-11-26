use std::cell::Cell;
use std::path::Path;

use gitserious_app::{MissingWorkTree, RepositoryLocator, RepositoryRoot};

/// The failure Git reports for a directory outside any repository.
pub const NOT_A_REPOSITORY: &str = "not a git repository";

/// Finds `root` from any directory until a test sets a failure. Only
/// [`NOT_A_REPOSITORY`] reads as a directory outside Git.
pub struct Locator {
    pub root: RepositoryRoot,
    pub failure: Cell<Option<&'static str>>,
}

impl Locator {
    pub fn new(root: RepositoryRoot) -> Self {
        Self {
            root,
            failure: Cell::new(None),
        }
    }
}

impl RepositoryLocator for Locator {
    type Error = &'static str;

    fn locate(&self, _: &Path) -> Result<RepositoryRoot, Self::Error> {
        self.failure
            .get()
            .map_or_else(|| Ok(self.root.clone()), Err)
    }

    fn missing_work_tree(error: &Self::Error) -> Option<MissingWorkTree> {
        (*error == NOT_A_REPOSITORY).then_some(MissingWorkTree::NotRepository)
    }
}
