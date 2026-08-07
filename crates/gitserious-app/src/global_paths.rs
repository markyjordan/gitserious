use std::path::PathBuf;

use crate::StorageDirectory;

/// User-scoped storage directories selected for one application invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalPaths {
    config: StorageDirectory,
    data: StorageDirectory,
    state: StorageDirectory,
    cache: StorageDirectory,
}

impl GlobalPaths {
    /// Creates an owned global-storage snapshot from adapter-selected paths.
    #[must_use]
    pub fn new(config: PathBuf, data: PathBuf, state: PathBuf, cache: PathBuf) -> Self {
        Self {
            config: StorageDirectory::new(config),
            data: StorageDirectory::new(data),
            state: StorageDirectory::new(state),
            cache: StorageDirectory::new(cache),
        }
    }

    /// Returns the user-authored configuration directory.
    #[must_use]
    pub const fn config(&self) -> &StorageDirectory {
        &self.config
    }

    /// Returns the durable, portable application-data directory.
    #[must_use]
    pub const fn data(&self) -> &StorageDirectory {
        &self.data
    }

    /// Returns the durable, machine-local application-state directory.
    #[must_use]
    pub const fn state(&self) -> &StorageDirectory {
        &self.state
    }

    /// Returns the disposable derived-data directory.
    #[must_use]
    pub const fn cache(&self) -> &StorageDirectory {
        &self.cache
    }
}
