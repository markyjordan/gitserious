use std::path::Path;

use gitserious_app::GlobalPaths;

use crate::GlobalPathError;

const APPLICATION_DIRECTORY: &str = "gitserious";

#[cfg(windows)]
pub(crate) fn resolve() -> Result<GlobalPaths, GlobalPathError> {
    let directories =
        directories::BaseDirs::new().ok_or(GlobalPathError::NativeDirectoriesUnavailable)?;
    resolve_from_root(Some(directories.config_dir()))
}

pub(crate) fn resolve_from_root(
    roaming_app_data: Option<&Path>,
) -> Result<GlobalPaths, GlobalPathError> {
    match roaming_app_data {
        Some(path) if path.is_absolute() => Ok(GlobalPaths::new(
            path.join(APPLICATION_DIRECTORY).join("config"),
        )),
        Some(path) => Err(GlobalPathError::RelativeNativeDirectory(path.to_path_buf())),
        None => Err(GlobalPathError::NativeDirectoriesUnavailable),
    }
}
