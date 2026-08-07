use std::path::{Path, PathBuf};

/// An application-owned directory resolved by gitserious storage policy.
///
/// Values can only be produced by this crate's path resolvers, preventing
/// filesystem adapters from accepting arbitrary caller paths.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageDirectory(PathBuf);

impl StorageDirectory {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self(path)
    }

    /// Returns the resolved filesystem path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for StorageDirectory {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}
