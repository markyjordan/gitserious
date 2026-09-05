#![cfg(unix)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_gitserious")
}

fn new_repository() -> Result<TempDir, Box<dyn Error>> {
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
    let repository = new_repository()?;
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
    assert!(authored.contains("[[taxonomies]]"));
    assert!(authored.contains("[[typesets]]"));
    assert!(authored.contains("[[templates]]"));
    let lock = fs::read_to_string(repository.path().join("gitserious.lock"))?;
    assert!(lock.contains("id = \"platform\""));
    assert_eq!(
        lock.matches("[[resolved-template.commit-types]]").count(),
        11
    );

    let repeated = run(repository.path(), &isolation, &["init"])?;
    assert!(repeated.status.success(), "{}", stderr(&repeated));
    assert!(stdout(&repeated).contains("(platform -> platform@1)."));

    for arguments in [
        &["config", "delete", "template", "platform"][..],
        &[
            "config",
            "delete",
            "typeset",
            "platform-taxonomy/platform-typeset",
        ][..],
        &["config", "delete", "taxonomy", "platform-taxonomy"][..],
    ] {
        let deleted = run(repository.path(), &isolation, arguments)?;
        assert!(deleted.status.success(), "{}", stderr(&deleted));
    }

    fs::write(isolation.configuration_path(), "not toml")?;
    let independent = run(repository.path(), &isolation, &["init"])?;
    assert!(independent.status.success(), "{}", stderr(&independent));
    assert!(stdout(&independent).contains("(platform -> platform@1)."));

    let fresh_repository = new_repository()?;
    let default_init = run(fresh_repository.path(), &isolation, &["init"])?;
    assert!(default_init.status.success(), "{}", stderr(&default_init));
    assert!(stdout(&default_init).contains("(default -> conventional@1)."));

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
    let repository = new_repository()?;
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

fn assert_domain_initialization(domain: &str, representative: &str) -> Result<(), Box<dyn Error>> {
    let repository = new_repository()?;
    let isolation = Isolation::new()?;
    // Built-ins must remain usable even when unrelated global TOML is invalid.
    fs::create_dir_all(
        isolation
            .configuration_path()
            .parent()
            .ok_or("missing parent")?,
    )?;
    fs::write(isolation.configuration_path(), "not valid TOML [")?;
    let initialized = run(
        repository.path(),
        &isolation,
        &["init", "--template", domain],
    )?;
    assert!(initialized.status.success(), "{}", stderr(&initialized));
    assert!(stdout(&initialized).contains(&format!("({domain} -> {domain}@1).")));
    let config_path = repository.path().join("gitserious.toml");
    let lock_path = repository.path().join("gitserious.lock");
    let config = fs::read_to_string(&config_path)?;
    let lock = fs::read_to_string(&lock_path)?;
    assert!(config.contains(&format!("active-template = \"{domain}\"")));
    assert!(!config.contains("[[taxonomies]]"));
    assert_eq!(
        lock.matches("[[resolved-template.commit-types]]").count(),
        10
    );
    assert!(lock.contains(&format!("id = \"{representative}\"")));
    assert!(lock.contains("[[resolved-templates]]\nid = \"default\""));
    let unavailable = run(repository.path(), &isolation, &["commit", "--type", "feat"])?;
    assert!(!unavailable.status.success());
    assert!(
        stderr(&unavailable).contains("not available"),
        "{}",
        stderr(&unavailable)
    );
    let selected = run(
        repository.path(),
        &isolation,
        &["commit", "--type", representative],
    )?;
    assert!(!selected.status.success());
    assert!(
        stderr(&selected).contains("interactive"),
        "{}",
        stderr(&selected)
    );
    let repeated = run(repository.path(), &isolation, &["init"])?;
    assert!(repeated.status.success(), "{}", stderr(&repeated));
    assert_eq!(fs::read_to_string(config_path)?, config);
    assert_eq!(fs::read_to_string(lock_path)?, lock);
    Ok(())
}

#[test]
fn ml_research_initializes_without_global_configuration_and_uses_its_types()
-> Result<(), Box<dyn Error>> {
    assert_domain_initialization("ml-research", "hypothesis")
}

#[test]
fn infra_ops_initializes_without_global_configuration_and_uses_its_types()
-> Result<(), Box<dyn Error>> {
    assert_domain_initialization("infra-ops", "provision")
}

#[test]
fn bare_config_requires_a_terminal_without_loading_or_creating_policy() -> Result<(), Box<dyn Error>>
{
    let repository = new_repository()?;
    let isolation = Isolation::new()?;
    let result = run(repository.path(), &isolation, &["config"])?;
    assert!(!result.status.success());
    assert!(stderr(&result).contains("configuration editing requires an interactive terminal"));
    assert!(!isolation.configuration_path().exists());
    assert!(!repository.path().join("gitserious.toml").exists());
    assert!(!repository.path().join("gitserious.lock").exists());
    Ok(())
}
