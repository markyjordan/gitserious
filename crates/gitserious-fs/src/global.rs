use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;

use gitserious_app::{GlobalPathResolver, GlobalPaths};

/// System-environment adapter for resolving global storage paths.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemGlobalPathResolver;

impl GlobalPathResolver for SystemGlobalPathResolver {
    type Error = GlobalPathError;

    fn resolve(&self) -> Result<GlobalPaths, Self::Error> {
        crate::platform::resolve()
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
