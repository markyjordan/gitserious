use std::error::Error;
use std::path::{Path, PathBuf};

use gitserious_app::resolve_global_paths;

use crate::platform::windows;
use crate::{GlobalPathError, SystemGlobalPathResolver};

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

    let paths = windows::resolve_from_root(Some(&roaming))?;

    assert_eq!(
        paths.config().as_path(),
        roaming.join("gitserious").join("config")
    );
    Ok(())
}

#[test]
fn windows_resolution_requires_the_roaming_root() {
    assert_eq!(
        windows::resolve_from_root(None),
        Err(GlobalPathError::NativeDirectoriesUnavailable)
    );
}

#[test]
fn windows_resolution_rejects_a_relative_roaming_root() {
    let relative = PathBuf::from("relative-app-data");

    assert_eq!(
        windows::resolve_from_root(Some(&relative)),
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
    fn an_absolute_xdg_config_home_does_not_require_home() -> Result<(), Box<dyn Error>> {
        let environment = FakeEnvironment::default().with("XDG_CONFIG_HOME", "/xdg/config");

        let paths = xdg::resolve_from(&environment)?;

        assert_eq!(
            paths.config().as_path(),
            Path::new("/xdg/config/gitserious")
        );
        Ok(())
    }

    #[test]
    fn an_unset_xdg_config_home_falls_back_beneath_home() -> Result<(), Box<dyn Error>> {
        let paths = xdg::resolve_from(&environment_with_home())?;

        assert_eq!(
            paths.config().as_path(),
            Path::new("/home/example/.config/gitserious")
        );
        Ok(())
    }

    #[test]
    fn empty_and_relative_xdg_config_homes_are_ignored() -> Result<(), Box<dyn Error>> {
        for value in ["", "relative/config"] {
            let environment = environment_with_home().with("XDG_CONFIG_HOME", value);

            let paths = xdg::resolve_from(&environment)?;

            assert_eq!(
                paths.config().as_path(),
                Path::new("/home/example/.config/gitserious")
            );
        }
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
            .with("XDG_CONFIG_HOME", base.join("config").into_os_string());

        let paths = xdg::resolve_from(&environment)?;

        assert!(!base.exists());
        assert!(!paths.config().as_path().exists());
        Ok(())
    }

    #[test]
    fn non_unicode_xdg_paths_are_preserved() -> Result<(), Box<dyn Error>> {
        use std::os::unix::ffi::OsStringExt;

        let base = PathBuf::from(OsString::from_vec(vec![b'/', b'x', 0xff]));
        let environment =
            FakeEnvironment::default().with("XDG_CONFIG_HOME", base.clone().into_os_string());

        let paths = xdg::resolve_from(&environment)?;

        assert_eq!(paths.config().as_path(), base.join("gitserious"));
        Ok(())
    }

    #[test]
    fn system_adapter_uses_xdg_on_unix() -> Result<(), Box<dyn Error>> {
        let paths = resolve_global_paths(&SystemGlobalPathResolver)?;

        assert!(paths.config().as_path().is_absolute());
        Ok(())
    }
}
