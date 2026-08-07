use std::path::{Path, PathBuf};

use crate::{GlobalPathError, GlobalPaths};

const APPLICATION_DIRECTORY: &str = "gitserious";

#[cfg(windows)]
pub(crate) fn resolve() -> Result<GlobalPaths, GlobalPathError> {
    let directories =
        directories::BaseDirs::new().ok_or(GlobalPathError::NativeDirectoriesUnavailable)?;
    resolve_from_roots(
        Some(directories.config_dir()),
        Some(directories.cache_dir()),
    )
}

pub(crate) fn resolve_from_roots(
    roaming_app_data: Option<&Path>,
    local_app_data: Option<&Path>,
) -> Result<GlobalPaths, GlobalPathError> {
    let roaming_app_data = validate_root(roaming_app_data)?;
    let local_app_data = validate_root(local_app_data)?;
    let roaming_application = roaming_app_data.join(APPLICATION_DIRECTORY);
    let local_application = local_app_data.join(APPLICATION_DIRECTORY);

    Ok(GlobalPaths::new(
        roaming_application.join("config"),
        roaming_application.join("data"),
        local_application.join("state"),
        local_application.join("cache"),
    ))
}

fn validate_root(root: Option<&Path>) -> Result<PathBuf, GlobalPathError> {
    match root {
        Some(path) if path.is_absolute() => Ok(path.to_path_buf()),
        Some(path) => Err(GlobalPathError::RelativeNativeDirectory(path.to_path_buf())),
        None => Err(GlobalPathError::NativeDirectoriesUnavailable),
    }
}
