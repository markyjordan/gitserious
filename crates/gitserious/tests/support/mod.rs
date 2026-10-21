#![allow(dead_code)]

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

/// Git variables that would redirect a fixture's git calls into another repository.
const REPOSITORY_OVERRIDES: [&str; 3] = ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"];

pub fn git_command(directory: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(directory);
    for variable in REPOSITORY_OVERRIDES {
        command.env_remove(variable);
    }
    command
}

pub fn git(directory: &Path, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
    let output = git_command(directory).args(arguments).output()?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!(
            "git {} failed: {}{}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

pub fn repository() -> Result<TempDir, Box<dyn Error>> {
    let directory = tempfile::Builder::new()
        .prefix("gitserious process ")
        .tempdir()?;
    git(directory.path(), &["init", "-q"])?;
    git(directory.path(), &["config", "user.name", "Git Serious"])?;
    git(
        directory.path(),
        &["config", "user.email", "gitserious@example.com"],
    )?;
    git(directory.path(), &["config", "commit.gpgsign", "false"])?;
    Ok(directory)
}

pub fn head_exists(directory: &Path) -> Result<bool, Box<dyn Error>> {
    Ok(git_command(directory)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()?
        .status
        .success())
}

/// A private home so the binary never reads the developer's global library.
pub struct Home {
    root: TempDir,
}

impl Home {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            root: tempfile::tempdir()?,
        })
    }

    pub fn config_home(&self) -> PathBuf {
        self.root.path().join("config")
    }

    pub fn library_path(&self) -> PathBuf {
        self.config_home().join("gitserious/config.toml")
    }

    pub fn command(&self, directory: &Path) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_gitserious"));
        command
            .current_dir(directory)
            .env("HOME", self.root.path())
            .env("XDG_CONFIG_HOME", self.config_home())
            .env("XDG_DATA_HOME", self.root.path().join("data"))
            .env("XDG_STATE_HOME", self.root.path().join("state"))
            .env("XDG_CACHE_HOME", self.root.path().join("cache"));
        for variable in REPOSITORY_OVERRIDES {
            command.env_remove(variable);
        }
        command
    }

    pub fn run(&self, directory: &Path, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
        Ok(self.command(directory).args(arguments).output()?)
    }

    pub fn init(&self, directory: &Path) -> Result<(), Box<dyn Error>> {
        let output = self.run(directory, &["init"])?;
        if output.status.success() {
            Ok(())
        } else {
            Err(stderr(&output).into())
        }
    }
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

pub fn with_stale_fingerprint(lock: &str) -> Result<String, Box<dyn Error>> {
    let marker = "config-fingerprint = \"sha256:";
    let start = lock.find(marker).ok_or("lock has no fingerprint")? + marker.len();
    let replacement = if lock[start..].starts_with('0') {
        "1"
    } else {
        "0"
    };
    Ok(format!(
        "{}{replacement}{}",
        &lock[..start],
        &lock[start + 1..]
    ))
}
