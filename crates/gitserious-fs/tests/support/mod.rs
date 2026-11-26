#![allow(dead_code)]

use std::env;
use std::error::Error;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

/// Git variables that would redirect a fixture's git calls into another repository.
const REPOSITORY_OVERRIDES: [&str; 3] = ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"];

/// Repository settings outrank the developer's global and system configuration, so they also
/// reach the adapters under test, which run Git with this process's environment.
const REPOSITORY_CONFIGURATION: [(&str, &str); 4] = [
    ("user.name", "Git Serious Test"),
    ("user.email", "test@example.invalid"),
    ("commit.gpgsign", "false"),
    ("core.hooksPath", ".git/hooks"),
];

/// A `git -C directory` command that reads no global or system configuration and ignores any
/// repository the environment names.
pub fn git_command(directory: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(directory)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null");
    for variable in REPOSITORY_OVERRIDES {
        command.env_remove(variable);
    }
    command
}

/// Runs [`git_command`] and fails with Git's diagnostics unless it succeeds.
pub fn git(directory: &Path, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
    let output = git_command(directory).args(arguments).output()?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!(
            "git {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

/// Creates a repository on `main` in `directory` with a fixed identity, no signing, and Git's
/// default hooks. It refuses while the environment names a repository, because the adapters
/// under test would run Git against that repository instead.
pub fn init(directory: &Path) -> Result<(), Box<dyn Error>> {
    if let Some(variable) = REPOSITORY_OVERRIDES
        .into_iter()
        .find(|variable| env::var_os(variable).is_some())
    {
        return Err(format!("unset {variable}: the adapters under test inherit it").into());
    }
    git(directory, &["init", "-q", "-b", "main"])?;
    for (key, value) in REPOSITORY_CONFIGURATION {
        git(directory, &["config", key, value])?;
    }
    Ok(())
}

/// An [`init`] repository in a new temporary directory.
pub fn repository() -> Result<TempDir, Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    init(directory.path())?;
    Ok(directory)
}
