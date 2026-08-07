use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;

use crate::StorageDirectory;

/// Resolved user-scoped storage directories for gitserious.
///
/// Values are owned snapshots. Resolving paths performs no filesystem I/O and
/// never creates directories.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalPaths {
    config: StorageDirectory,
    data: StorageDirectory,
    state: StorageDirectory,
    cache: StorageDirectory,
}

impl GlobalPaths {
    /// Resolves global storage paths for the current operating system.
    ///
    /// Unix-family targets use XDG Base Directory environment variables and
    /// HOME-based fallbacks. Native Windows targets use Roaming and Local
    /// `AppData` Known Folders.
    ///
    /// # Errors
    ///
    /// Returns [`GlobalPathError`] when the platform cannot provide an
    /// absolute base directory required by its storage convention.
    pub fn resolve() -> Result<Self, GlobalPathError> {
        crate::platform::resolve()
    }

    pub(crate) fn new(config: PathBuf, data: PathBuf, state: PathBuf, cache: PathBuf) -> Self {
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

/// Failure to resolve a platform storage convention.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GlobalPathError {
    /// HOME was missing or empty when an XDG fallback required it.
    HomeUnavailable,
    /// HOME was relative and therefore cannot anchor an XDG fallback.
    RelativeHome(PathBuf),
    /// Windows did not provide the required Roaming or Local `AppData` roots.
    NativeDirectoriesUnavailable,
    /// A Windows Known Folder root was unexpectedly relative.
    RelativeNativeDirectory(PathBuf),
    /// The compilation target has no established global storage convention.
    UnsupportedPlatform,
}

impl Display for GlobalPathError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::HomeUnavailable => formatter
                .write_str("HOME is required when an XDG home is unset, empty, or relative"),
            Self::RelativeHome(path) => write!(
                formatter,
                "HOME must be absolute when used for an XDG fallback: {}",
                path.display()
            ),
            Self::NativeDirectoriesUnavailable => {
                formatter.write_str("Windows Roaming and Local AppData directories are required")
            }
            Self::RelativeNativeDirectory(path) => write!(
                formatter,
                "a Windows AppData directory was unexpectedly relative: {}",
                path.display()
            ),
            Self::UnsupportedPlatform => {
                formatter.write_str("this platform has no global storage convention")
            }
        }
    }
}

impl Error for GlobalPathError {}
