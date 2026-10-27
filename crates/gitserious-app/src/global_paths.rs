use std::path::PathBuf;

use crate::StorageDirectory;

/// The user-scoped configuration directory selected for one application invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalPaths {
    config: StorageDirectory,
}

impl GlobalPaths {
    /// Creates an owned global-storage snapshot from an adapter-selected path.
    #[must_use]
    pub fn new(config: PathBuf) -> Self {
        Self {
            config: StorageDirectory::new(config),
        }
    }

    /// Returns the user-authored configuration directory.
    #[must_use]
    pub const fn config(&self) -> &StorageDirectory {
        &self.config
    }
}
