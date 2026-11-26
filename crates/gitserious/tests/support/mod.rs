#![allow(dead_code)]

use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

/// Git variables that would redirect a fixture's git calls into another repository.
const REPOSITORY_OVERRIDES: [&str; 3] = ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"];

/// Repository settings outrank the developer's global and system configuration, so they also
/// reach the adapters that tests call in process, which run Git with this process's environment.
const REPOSITORY_CONFIGURATION: [(&str, &str); 4] = [
    ("user.name", "Git Serious"),
    ("user.email", "gitserious@example.com"),
    ("commit.gpgsign", "false"),
    ("core.hooksPath", ".git/hooks"),
];

/// Makes `command` read no global or system Git configuration and ignore any repository the
/// environment names.
fn isolate_git(command: &mut Command) -> &mut Command {
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null");
    for variable in REPOSITORY_OVERRIDES {
        command.env_remove(variable);
    }
    command
}

pub fn git_command(directory: &Path) -> Command {
    let mut command = Command::new("git");
    isolate_git(command.current_dir(directory));
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

/// A repository in a new temporary directory with a fixed identity, no signing, and Git's
/// default hooks. It refuses while the environment names a repository, because the adapters
/// that tests call in process would run Git against that repository instead.
pub fn repository() -> Result<TempDir, Box<dyn Error>> {
    if let Some(variable) = REPOSITORY_OVERRIDES
        .into_iter()
        .find(|variable| env::var_os(variable).is_some())
    {
        return Err(
            format!("unset {variable}: the adapters tests call in process inherit it").into(),
        );
    }
    let directory = tempfile::Builder::new()
        .prefix("gitserious process ")
        .tempdir()?;
    git(directory.path(), &["init", "-q"])?;
    for (key, value) in REPOSITORY_CONFIGURATION {
        git(directory.path(), &["config", key, value])?;
    }
    Ok(directory)
}

pub fn head_exists(directory: &Path) -> Result<bool, Box<dyn Error>> {
    Ok(git_command(directory)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()?
        .status
        .success())
}

/// A private home and Git configuration, so the binary never reads the developer's Library
/// or Git settings.
pub struct Home {
    root: TempDir,
}

impl Home {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            root: tempfile::tempdir()?,
        })
    }

    /// The directory every command sees as `HOME`.
    pub fn path(&self) -> &Path {
        self.root.path()
    }

    pub fn config_home(&self) -> PathBuf {
        self.root.path().join("config")
    }

    pub fn library_path(&self) -> PathBuf {
        self.config_home().join("gitserious/config.toml")
    }

    pub fn command(&self, directory: &Path) -> Command {
        self.command_for(Path::new(env!("CARGO_BIN_EXE_gitserious")), directory)
    }

    /// Runs `program`, a copy of the binary, the way [`Home::command`] runs the binary.
    pub fn command_for(&self, program: &Path, directory: &Path) -> Command {
        let mut command = Command::new(program);
        command
            .current_dir(directory)
            .env("HOME", self.root.path())
            .env("XDG_CONFIG_HOME", self.config_home())
            .env("XDG_DATA_HOME", self.root.path().join("data"))
            .env("XDG_STATE_HOME", self.root.path().join("state"))
            .env("XDG_CACHE_HOME", self.root.path().join("cache"));
        isolate_git(&mut command);
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

/// A fresh project adds only its policy and ignored local state beside Git and user files.
pub fn assert_project_shape(directory: &Path, existing: &[&str]) -> Result<(), Box<dyn Error>> {
    fn entries(
        directory: &Path,
    ) -> Result<std::collections::BTreeSet<std::ffi::OsString>, std::io::Error> {
        std::fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect()
    }
    let expected = [".git", ".gitserious", "gitserious.toml"]
        .into_iter()
        .chain(existing.iter().copied())
        .map(std::ffi::OsString::from)
        .collect();
    assert_eq!(
        entries(directory)?,
        expected,
        "initialization must not generate a lock file or another root artifact"
    );
    assert!(directory.join("gitserious.toml").is_file());
    assert_eq!(
        entries(&directory.join(".gitserious"))?,
        [std::ffi::OsString::from(".gitignore")]
            .into_iter()
            .collect(),
        "fresh local state contains only its ignore rule"
    );
    assert_eq!(
        std::fs::read_to_string(directory.join(".gitserious/.gitignore"))?,
        "*\n"
    );
    Ok(())
}
