use std::fs::DirBuilder;
use std::io;
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

/// Creates one resolved storage directory and its missing parents.
///
/// Calling this function is the explicit boundary between side-effect-free
/// path resolution and filesystem mutation. Existing directories and their
/// permissions are left unchanged.
///
/// # Errors
///
/// Returns the underlying filesystem error when the directory or a required
/// parent cannot be created.
pub fn ensure_directory(directory: &StorageDirectory) -> io::Result<()> {
    let mut builder = DirBuilder::new();
    builder.recursive(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;

        builder.mode(0o700);
    }

    builder.create(directory.as_path())
}
