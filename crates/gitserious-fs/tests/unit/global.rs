use std::error::Error;
use std::path::{Path, PathBuf};

use gitserious_app::{GlobalPaths, resolve_global_paths};

use crate::platform::windows;
use crate::{GlobalPathError, SystemGlobalPathResolver};

fn assert_paths(paths: &GlobalPaths, config: &Path, data: &Path, state: &Path, cache: &Path) {
    assert_eq!(paths.config().as_path(), config);
    assert_eq!(paths.data().as_path(), data);
    assert_eq!(paths.state().as_path(), state);
    assert_eq!(paths.cache().as_path(), cache);
}

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
fn windows_roots_map_to_roaming_and_local_purposes() -> Result<(), Box<dyn Error>> {
    let roaming = absolute_test_root("Roaming");
    let local = absolute_test_root("Local");

    let paths = windows::resolve_from_roots(Some(&roaming), Some(&local))?;

    assert_paths(
        &paths,
        &roaming.join("gitserious/config"),
        &roaming.join("gitserious/data"),
        &local.join("gitserious/state"),
        &local.join("gitserious/cache"),
    );
    Ok(())
}

#[test]
fn windows_resolution_requires_both_known_folder_roots() {
    let roaming = absolute_test_root("Roaming");
    let local = absolute_test_root("Local");

    assert_eq!(
        windows::resolve_from_roots(None, Some(&local)),
        Err(GlobalPathError::NativeDirectoriesUnavailable)
    );
    assert_eq!(
        windows::resolve_from_roots(Some(&roaming), None),
        Err(GlobalPathError::NativeDirectoriesUnavailable)
    );
}

#[test]
fn windows_resolution_rejects_relative_known_folder_roots() {
    let absolute = absolute_test_root("AppData");
    let relative = PathBuf::from("relative-app-data");

    assert_eq!(
        windows::resolve_from_roots(Some(&relative), Some(&absolute)),
        Err(GlobalPathError::RelativeNativeDirectory(relative.clone()))
    );
    assert_eq!(
        windows::resolve_from_roots(Some(&absolute), Some(&relative)),
        Err(GlobalPathError::RelativeNativeDirectory(relative))
    );
}

#[cfg(windows)]
#[test]
fn native_windows_adapter_returns_absolute_known_folder_paths() -> Result<(), Box<dyn Error>> {
    let paths = resolve_global_paths(&SystemGlobalPathResolver)?;

    assert!(paths.config().as_path().is_absolute());
    assert!(paths.data().as_path().is_absolute());
    assert!(paths.state().as_path().is_absolute());
    assert!(paths.cache().as_path().is_absolute());
    Ok(())
}

#[cfg(unix)]
mod xdg_contract {
    use std::collections::HashMap;
    use std::ffi::OsString;

    use crate::platform::xdg::{self, Environment};
    use crate::tests::support::TestDirectory;

    use super::*;

    #[derive(Default)]
    struct FakeEnvironment {
        variables: HashMap<String, OsString>,
    }

    impl FakeEnvironment {
        fn with(mut self, name: &str, value: impl Into<OsString>) -> Self {
            self.variables.insert(name.to_owned(), value.into());
            self
        }

        fn set(&mut self, name: &str, value: impl Into<OsString>) {
            self.variables.insert(name.to_owned(), value.into());
        }
    }

    impl Environment for FakeEnvironment {
        fn variable(&self, name: &str) -> Option<OsString> {
            self.variables.get(name).cloned()
        }
    }

    fn environment_with_home() -> FakeEnvironment {
        FakeEnvironment::default().with("HOME", "/home/example")
    }

    #[test]
    fn absolute_xdg_overrides_do_not_require_home() -> Result<(), Box<dyn Error>> {
        let environment = FakeEnvironment::default()
            .with("XDG_CONFIG_HOME", "/xdg/config")
            .with("XDG_DATA_HOME", "/xdg/data")
            .with("XDG_STATE_HOME", "/xdg/state")
            .with("XDG_CACHE_HOME", "/xdg/cache");

        let paths = xdg::resolve_from(&environment)?;

        assert_paths(
            &paths,
            Path::new("/xdg/config/gitserious"),
            Path::new("/xdg/data/gitserious"),
            Path::new("/xdg/state/gitserious"),
            Path::new("/xdg/cache/gitserious"),
        );
        Ok(())
    }

    #[test]
    fn unset_xdg_homes_use_specified_home_fallbacks() -> Result<(), Box<dyn Error>> {
        let paths = xdg::resolve_from(&environment_with_home())?;

        assert_paths(
            &paths,
            Path::new("/home/example/.config/gitserious"),
            Path::new("/home/example/.local/share/gitserious"),
            Path::new("/home/example/.local/state/gitserious"),
            Path::new("/home/example/.cache/gitserious"),
        );
        Ok(())
    }

    #[test]
    fn each_xdg_override_is_resolved_independently() -> Result<(), Box<dyn Error>> {
        let cases = [
            ("XDG_CONFIG_HOME", 0),
            ("XDG_DATA_HOME", 1),
            ("XDG_STATE_HOME", 2),
            ("XDG_CACHE_HOME", 3),
        ];

        for (variable, selected) in cases {
            let environment = environment_with_home().with(variable, "/override");
            let paths = xdg::resolve_from(&environment)?;
            let actual = [paths.config(), paths.data(), paths.state(), paths.cache()];
            let fallbacks = [
                Path::new("/home/example/.config/gitserious"),
                Path::new("/home/example/.local/share/gitserious"),
                Path::new("/home/example/.local/state/gitserious"),
                Path::new("/home/example/.cache/gitserious"),
            ];

            for (index, directory) in actual.into_iter().enumerate() {
                if index == selected {
                    assert_eq!(directory.as_path(), Path::new("/override/gitserious"));
                } else {
                    assert_eq!(directory.as_path(), fallbacks[index]);
                }
            }
        }

        Ok(())
    }

    #[test]
    fn empty_and_relative_xdg_overrides_are_ignored() -> Result<(), Box<dyn Error>> {
        let environment = environment_with_home()
            .with("XDG_CONFIG_HOME", "")
            .with("XDG_DATA_HOME", "relative/data")
            .with("XDG_STATE_HOME", "")
            .with("XDG_CACHE_HOME", "relative/cache");

        let paths = xdg::resolve_from(&environment)?;

        assert_paths(
            &paths,
            Path::new("/home/example/.config/gitserious"),
            Path::new("/home/example/.local/share/gitserious"),
            Path::new("/home/example/.local/state/gitserious"),
            Path::new("/home/example/.cache/gitserious"),
        );
        Ok(())
    }

    #[test]
    fn unavailable_or_empty_home_is_reported() {
        let missing = FakeEnvironment::default();
        let empty = FakeEnvironment::default().with("HOME", "");

        assert_eq!(
            xdg::resolve_from(&missing),
            Err(GlobalPathError::HomeUnavailable)
        );
        assert_eq!(
            xdg::resolve_from(&empty),
            Err(GlobalPathError::HomeUnavailable)
        );
    }

    #[test]
    fn relative_home_is_reported_with_its_value() {
        let environment = FakeEnvironment::default().with("HOME", "relative/home");

        assert_eq!(
            xdg::resolve_from(&environment),
            Err(GlobalPathError::RelativeHome(PathBuf::from(
                "relative/home"
            )))
        );
    }

    #[test]
    fn resolved_paths_are_owned_environment_snapshots() -> Result<(), Box<dyn Error>> {
        let mut environment = environment_with_home();
        let paths = xdg::resolve_from(&environment)?;

        environment.set("HOME", "/home/changed");

        assert_eq!(
            paths.config().as_path(),
            Path::new("/home/example/.config/gitserious")
        );
        Ok(())
    }

    #[test]
    fn path_resolution_performs_no_filesystem_creation() -> Result<(), Box<dyn Error>> {
        let root = TestDirectory::new("resolution-no-io")?;
        let base = root.path().join("not-created");
        let environment = FakeEnvironment::default()
            .with("XDG_CONFIG_HOME", base.join("config").into_os_string())
            .with("XDG_DATA_HOME", base.join("data").into_os_string())
            .with("XDG_STATE_HOME", base.join("state").into_os_string())
            .with("XDG_CACHE_HOME", base.join("cache").into_os_string());

        let paths = xdg::resolve_from(&environment)?;

        assert!(!base.exists());
        assert!(!paths.config().as_path().exists());
        assert!(!paths.data().as_path().exists());
        assert!(!paths.state().as_path().exists());
        assert!(!paths.cache().as_path().exists());
        Ok(())
    }

    #[test]
    fn non_unicode_xdg_paths_are_preserved() -> Result<(), Box<dyn Error>> {
        use std::os::unix::ffi::OsStringExt;

        let base = PathBuf::from(OsString::from_vec(vec![b'/', b'x', 0xff]));
        let environment = FakeEnvironment::default()
            .with("XDG_CONFIG_HOME", base.clone().into_os_string())
            .with("XDG_DATA_HOME", "/xdg/data")
            .with("XDG_STATE_HOME", "/xdg/state")
            .with("XDG_CACHE_HOME", "/xdg/cache");

        let paths = xdg::resolve_from(&environment)?;

        assert_eq!(paths.config().as_path(), base.join("gitserious"));
        Ok(())
    }

    #[test]
    fn system_adapter_uses_xdg_on_unix() -> Result<(), Box<dyn Error>> {
        let paths = resolve_global_paths(&SystemGlobalPathResolver)?;

        assert!(paths.config().as_path().is_absolute());
        assert!(paths.data().as_path().is_absolute());
        assert!(paths.state().as_path().is_absolute());
        assert!(paths.cache().as_path().is_absolute());
        Ok(())
    }
}
