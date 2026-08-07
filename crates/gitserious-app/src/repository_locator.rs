use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

/// An absolute Git worktree root selected by an outbound adapter.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RepositoryRoot(PathBuf);

impl RepositoryRoot {
    /// Creates an absolute repository root.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryRootError`] when `path` is relative.
    pub fn new(path: PathBuf) -> Result<Self, RepositoryRootError> {
        if path.is_relative() {
            return Err(RepositoryRootError(path));
        }
        Ok(Self(path))
    }

    /// Returns the absolute root path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for RepositoryRoot {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

/// A relative path supplied as a repository root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryRootError(PathBuf);

impl RepositoryRootError {
    /// Returns the rejected path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Display for RepositoryRootError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "repository root must be absolute: {}",
            self.0.display()
        )
    }
}

impl Error for RepositoryRootError {}

/// Finds the enclosing Git worktree for an invocation directory.
pub trait RepositoryLocator {
    /// The adapter-specific discovery failure.
    type Error;

    /// Locates the worktree containing `start`.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when no usable worktree can be
    /// discovered.
    fn locate(&self, start: &Path) -> Result<RepositoryRoot, Self::Error>;
}
