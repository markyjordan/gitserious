#![cfg(unix)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_gitserious")
}

fn repository() -> Result<TempDir, Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(directory.path())
        .status()?;
    if !status.success() {
        return Err(format!("git init failed with {status}").into());
    }
    Ok(directory)
}

struct Isolation {
    config_home: TempDir,
}

impl Isolation {
    fn new() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            config_home: tempfile::tempdir()?,
        })
    }

    fn configuration_path(&self) -> PathBuf {
        self.config_home.path().join("gitserious/config.toml")
    }
}

fn run(
    directory: &Path,
    isolation: &Isolation,
    arguments: &[&str],
) -> Result<Output, Box<dyn Error>> {
    Ok(Command::new(binary())
        .args(arguments)
        .current_dir(directory)
        .env("XDG_CONFIG_HOME", isolation.config_home.path())
        .env_remove("XDG_DATA_HOME")
        .env_remove("XDG_STATE_HOME")
        .env_remove("XDG_CACHE_HOME")
        .output()?)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

const FEAT_ENTRY_MARKER: &str =
    "id = \"feat\"\nschema-version = 1\ndefinition-fingerprint = \"sha256:";

#[test]
fn fork_init_and_policy_verification_end_to_end() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let isolation = Isolation::new()?;

    let fork = run(
        repository.path(),
        &isolation,
        &["config", "fork", "--template", "platform"],
    )?;
    assert!(fork.status.success(), "{}", stderr(&fork));
    assert_eq!(
        stdout(&fork),
        "Forked conventional into template platform \
         (taxonomy platform-taxonomy, typeset platform-typeset).\n"
    );
    assert!(isolation.configuration_path().exists());

    fs::write(repository.path().join("seed.txt"), "seed")?;
    Command::new("git")
        .args(["add", "."])
        .current_dir(repository.path())
        .status()?;
    Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "seed",
        ])
        .current_dir(repository.path())
        .status()?;

    let init = run(
        repository.path(),
        &isolation,
        &["init", "--template", "platform"],
    )?;
    assert!(init.status.success(), "{}", stderr(&init));
    assert!(stdout(&init).contains("(platform -> platform@1)."));
    let authored = fs::read_to_string(repository.path().join("gitserious.toml"))?;
    assert!(authored.contains("active-template = \"platform\""));
    let lock = fs::read_to_string(repository.path().join("gitserious.lock"))?;
    assert!(lock.contains("id = \"platform\""));
    assert_eq!(
        lock.matches("[[resolved-template.commit-types]]").count(),
        11
    );

    let repeated = run(repository.path(), &isolation, &["init"])?;
    assert!(repeated.status.success(), "{}", stderr(&repeated));
    assert!(stdout(&repeated).contains("(platform -> platform@1)."));

    let unknown = run(
        repository.path(),
        &isolation,
        &["commit", "--type", "deploy"],
    )?;
    assert!(!unknown.status.success());
    let failure = stderr(&unknown);
    assert!(
        failure.contains("commit type ChangeTypeId(\"deploy\") is not available"),
        "unexpected stderr: {failure}"
    );
    assert!(failure.contains("feat"), "unexpected stderr: {failure}");

    let interactive = run(repository.path(), &isolation, &["commit", "--type", "feat"])?;
    assert!(!interactive.status.success());
    assert!(
        stderr(&interactive).contains("requires an interactive terminal"),
        "unexpected stderr: {}",
        stderr(&interactive)
    );
    Ok(())
}

#[test]
fn tampered_locks_are_rejected_before_authoring() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let isolation = Isolation::new()?;

    let forked = run(
        repository.path(),
        &isolation,
        &["config", "fork", "--template", "platform"],
    )?;
    assert!(forked.status.success(), "{}", stderr(&forked));
    fs::write(repository.path().join("seed.txt"), "seed")?;
    Command::new("git")
        .args(["add", "."])
        .current_dir(repository.path())
        .status()?;
    Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "seed",
        ])
        .current_dir(repository.path())
        .status()?;
    let initialized = run(
        repository.path(),
        &isolation,
        &["init", "--template", "platform"],
    )?;
    assert!(initialized.status.success(), "{}", stderr(&initialized));
    let lock_path = repository.path().join("gitserious.lock");
    let original = fs::read_to_string(&lock_path)?;
    let hash_start = original
        .find(FEAT_ENTRY_MARKER)
        .ok_or("locked feat entry not found in generated lock")?
        + FEAT_ENTRY_MARKER.len();
    let tampered = format!(
        "{}{}{}",
        &original[..hash_start],
        "0".repeat(64),
        &original[hash_start + 64..]
    );
    assert_ne!(original, tampered);
    fs::write(&lock_path, tampered)?;

    let rejected = run(repository.path(), &isolation, &["commit", "--type", "feat"])?;
    assert!(!rejected.status.success());
    assert!(
        stderr(&rejected).contains("policy is stale"),
        "unexpected stderr: {}",
        stderr(&rejected)
    );
    Ok(())
}
