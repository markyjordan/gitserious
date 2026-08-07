#[cfg(windows)]
pub(crate) mod windows;
#[cfg(any(test, unix))]
pub(crate) mod xdg;

use crate::{GlobalPathError, GlobalPaths};

pub(crate) fn resolve() -> Result<GlobalPaths, GlobalPathError> {
    #[cfg(unix)]
    {
        return xdg::resolve();
    }

    #[cfg(windows)]
    {
        return windows::resolve();
    }

    #[allow(unreachable_code)]
    Err(GlobalPathError::UnsupportedPlatform)
}
