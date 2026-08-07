use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::{GlobalPathError, GlobalPaths};

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

    Ok(GlobalPaths::new(
        resolve_directory(environment, "XDG_CONFIG_HOME", home.as_deref(), ".config")?,
        resolve_directory(
            environment,
            "XDG_DATA_HOME",
            home.as_deref(),
            ".local/share",
        )?,
        resolve_directory(
            environment,
            "XDG_STATE_HOME",
            home.as_deref(),
            ".local/state",
        )?,
        resolve_directory(environment, "XDG_CACHE_HOME", home.as_deref(), ".cache")?,
    ))
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
