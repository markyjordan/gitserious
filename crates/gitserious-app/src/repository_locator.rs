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

    /// Why no usable Git worktree contains the start directory, or `None` when Git itself
    /// failed.
    fn missing_work_tree(error: &Self::Error) -> Option<MissingWorkTree>;
}

/// Why no usable Git work tree contains a directory, which decides what Commit and Home offer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MissingWorkTree {
    /// No repository contains the directory, so `git init` may create one there.
    NotRepository,
    /// No repository contains the directory, but it is a home directory or a filesystem
    /// root, where a repository would take in everything below it.
    HomeOrRoot,
    /// A bare repository, which has no work tree to commit from.
    Bare,
    /// Inside a repository's Git directory. `worktree` is its work tree when the layout
    /// names one.
    InsideGitDir { worktree: Option<PathBuf> },
    /// Git refuses a repository another user owns until it is marked safe.
    DubiousOwnership { repository: PathBuf },
}

/// Creates a Git repository in a directory that has none.
pub trait RepositoryInitializer {
    fn initialize(&self, directory: &Path) -> Result<(), String>;
}
