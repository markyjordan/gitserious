use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use gitserious_app::GlobalPaths;

use crate::GlobalPathError;

const APPLICATION_DIRECTORY: &str = "gitserious";

pub(crate) trait Environment {
    fn variable(&self, name: &str) -> Option<OsString>;
}

struct ProcessEnvironment;

impl Environment for ProcessEnvironment {
    fn variable(&self, name: &str) -> Option<OsString> {
        env::var_os(name)
    }
}

pub(crate) fn resolve() -> Result<GlobalPaths, GlobalPathError> {
    resolve_from(&ProcessEnvironment)
}

pub(crate) fn resolve_from(environment: &impl Environment) -> Result<GlobalPaths, GlobalPathError> {
    let home = environment.variable("HOME").map(PathBuf::from);

    Ok(GlobalPaths::new(resolve_directory(
        environment,
        "XDG_CONFIG_HOME",
        home.as_deref(),
        ".config",
    )?))
}

fn resolve_directory(
    environment: &impl Environment,
    variable: &str,
    home: Option<&Path>,
    fallback: &str,
) -> Result<PathBuf, GlobalPathError> {
    let base = environment
        .variable(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute());

    match base {
        Some(path) => Ok(path.join(APPLICATION_DIRECTORY)),
        None => Ok(resolve_home(home)?
            .join(fallback)
            .join(APPLICATION_DIRECTORY)),
    }
}

fn resolve_home(home: Option<&Path>) -> Result<&Path, GlobalPathError> {
    match home {
        Some(path) if path.as_os_str().is_empty() => Err(GlobalPathError::HomeUnavailable),
        Some(path) if path.is_absolute() => Ok(path),
        Some(path) => Err(GlobalPathError::RelativeHome(path.to_path_buf())),
        None => Err(GlobalPathError::HomeUnavailable),
    }
}

trait OsStringExt {
    fn is_empty(&self) -> bool;
}

impl OsStringExt for OsString {
    fn is_empty(&self) -> bool {
        self.as_os_str() == OsStr::new("")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::error::Error;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use gitserious_app::resolve_global_paths;

    use super::{Environment, resolve_from};
    use crate::{GlobalPathError, SystemGlobalPathResolver};

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

        let paths = resolve_from(&environment)?;

        assert_eq!(
            paths.config().as_path(),
            Path::new("/xdg/config/gitserious")
        );
        Ok(())
    }

    #[test]
    fn an_unset_xdg_config_home_falls_back_beneath_home() -> Result<(), Box<dyn Error>> {
        let paths = resolve_from(&environment_with_home())?;

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

            let paths = resolve_from(&environment)?;

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
            resolve_from(&missing),
            Err(GlobalPathError::HomeUnavailable)
        );
        assert_eq!(resolve_from(&empty), Err(GlobalPathError::HomeUnavailable));
    }

    #[test]
    fn relative_home_is_reported_with_its_value() {
        let environment = FakeEnvironment::default().with("HOME", "relative/home");

        assert_eq!(
            resolve_from(&environment),
            Err(GlobalPathError::RelativeHome(PathBuf::from(
                "relative/home"
            )))
        );
    }

    #[test]
    fn resolved_paths_are_owned_environment_snapshots() -> Result<(), Box<dyn Error>> {
        let mut environment = environment_with_home();
        let paths = resolve_from(&environment)?;

        environment.set("HOME", "/home/changed");

        assert_eq!(
            paths.config().as_path(),
            Path::new("/home/example/.config/gitserious")
        );
        Ok(())
    }

    #[test]
    fn path_resolution_performs_no_filesystem_creation() -> Result<(), Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let base = root.path().join("not-created");
        let environment = FakeEnvironment::default()
            .with("XDG_CONFIG_HOME", base.join("config").into_os_string());

        let paths = resolve_from(&environment)?;

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

        let paths = resolve_from(&environment)?;

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
