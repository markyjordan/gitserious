use std::path::{Path, PathBuf};

/// A directory selected by an application storage-path resolver.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageDirectory(PathBuf);

impl StorageDirectory {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self(path)
    }

    /// Returns the selected filesystem path.
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
