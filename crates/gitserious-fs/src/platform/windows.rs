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

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::path::PathBuf;

    #[cfg(windows)]
    use gitserious_app::resolve_global_paths;

    use super::resolve_from_root;
    use crate::GlobalPathError;
    #[cfg(windows)]
    use crate::SystemGlobalPathResolver;

    fn absolute_test_root(name: &str) -> PathBuf {
        #[cfg(windows)]
        {
            PathBuf::from(format!(r"C:\{name}"))
        }

        #[cfg(not(windows))]
        {
            PathBuf::from(format!("/{name}"))
        }
    }

    #[test]
    fn windows_config_lives_under_roaming_app_data() -> Result<(), Box<dyn Error>> {
        let roaming = absolute_test_root("Roaming");

        let paths = resolve_from_root(Some(&roaming))?;

        assert_eq!(
            paths.config().as_path(),
            roaming.join("gitserious").join("config")
        );
        Ok(())
    }

    #[test]
    fn windows_resolution_requires_the_roaming_root() {
        assert_eq!(
            resolve_from_root(None),
            Err(GlobalPathError::NativeDirectoriesUnavailable)
        );
    }

    #[test]
    fn windows_resolution_rejects_a_relative_roaming_root() {
        let relative = PathBuf::from("relative-app-data");

        assert_eq!(
            resolve_from_root(Some(&relative)),
            Err(GlobalPathError::RelativeNativeDirectory(relative))
        );
    }

    #[cfg(windows)]
    #[test]
    fn native_windows_adapter_returns_absolute_known_folder_paths() -> Result<(), Box<dyn Error>> {
        let paths = resolve_global_paths(&SystemGlobalPathResolver)?;

        assert!(paths.config().as_path().is_absolute());
        Ok(())
    }
}
