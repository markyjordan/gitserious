//! Each test target compiles its own copy of this module, so an item that some target
//! leaves unused allows `dead_code`.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

/// Git variables that would redirect a fixture's git calls into another repository.
const REPOSITORY_OVERRIDES: [&str; 3] = ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"];

/// The identity, signing, and hooks of every Git command that a test or the binary runs in a
/// fixture repository.
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

fn git_command(directory: &Path) -> Command {
    let mut command = Command::new("git");
    isolate_git(command.current_dir(directory));
    command
}

#[allow(dead_code)]
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
/// default hooks.
#[allow(dead_code)]
pub fn repository() -> Result<TempDir, Box<dyn Error>> {
    let directory = tempfile::Builder::new()
        .prefix("gitserious process ")
        .tempdir()?;
    git(directory.path(), &["init", "-q"])?;
    for (key, value) in REPOSITORY_CONFIGURATION {
        git(directory.path(), &["config", key, value])?;
    }
    Ok(directory)
}

#[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn path(&self) -> &Path {
        self.root.path()
    }

    pub fn config_home(&self) -> PathBuf {
        self.root.path().join("config")
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub fn run(&self, directory: &Path, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
        Ok(self.command(directory).args(arguments).output()?)
    }

    #[allow(dead_code)]
    pub fn init(&self, directory: &Path) -> Result<(), Box<dyn Error>> {
        let output = self.run(directory, &["init"])?;
        if output.status.success() {
            Ok(())
        } else {
            Err(stderr(&output).into())
        }
    }
}

#[allow(dead_code)]
pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[allow(dead_code)]
pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
