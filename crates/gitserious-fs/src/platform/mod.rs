#[cfg(any(test, windows))]
pub(crate) mod windows;
#[cfg(unix)]
pub(crate) mod xdg;

use gitserious_app::GlobalPaths;

use crate::GlobalPathError;

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
