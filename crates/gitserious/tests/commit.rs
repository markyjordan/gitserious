use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_gitserious")
}

fn repository() -> Result<TempDir, Box<dyn Error>> {
    let directory = tempfile::Builder::new()
        .prefix("gitserious commit ")
        .tempdir()?;
    git_success(directory.path(), &["init", "-q"])?;
    git_success(directory.path(), &["config", "user.name", "Git Serious"])?;
    git_success(
        directory.path(),
        &["config", "user.email", "gitserious@example.com"],
    )?;
    Ok(directory)
}

fn run(directory: &Path, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
    Ok(Command::new(binary())
        .args(arguments)
        .current_dir(directory)
        .output()?)
}

fn git(directory: &Path, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
    Ok(Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .output()?)
}

fn git_success(directory: &Path, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
    let output = git(directory, arguments)?;
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

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn initialize(directory: &Path) -> Result<(), Box<dyn Error>> {
    let output = run(directory, &["init"])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(stderr(&output).into())
    }
}

fn head_exists(directory: &Path) -> Result<bool, Box<dyn Error>> {
    Ok(git(directory, &["rev-parse", "--verify", "HEAD"])?
        .status
        .success())
}

#[test]
fn project_policy_is_resolved_before_terminal_authoring() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;

    let absent = run(repository.path(), &["commit", "--type", "feat"])?;
    assert_eq!(absent.status.code(), Some(1));
    assert!(stdout(&absent).is_empty());
    assert!(stderr(&absent).contains("not initialized"));

    initialize(repository.path())?;
    let lock = repository.path().join("gitserious.lock");
    let stale = fs::read_to_string(&lock)?.replace(
        "config-fingerprint = \"sha256:b",
        "config-fingerprint = \"sha256:a",
    );
    fs::write(lock, stale)?;
    let stale = run(repository.path(), &["commit", "--type", "feat"])?;
    assert_eq!(stale.status.code(), Some(1));
    assert!(stdout(&stale).is_empty());
    assert!(stderr(&stale).contains("stale"));
    assert!(!head_exists(repository.path())?);
    Ok(())
}

#[test]
fn unavailable_type_is_rejected_before_terminal_authoring() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    initialize(repository.path())?;

    let output = run(repository.path(), &["commit", "--type", "custom"])?;

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
    let error = stderr(&output);
    assert!(error.contains("custom"));
    assert!(error.contains("choose one of: feat, fix"));
    assert!(error.contains("revert"));
    assert!(!error.contains("interactive terminal"));
    assert!(!head_exists(repository.path())?);
    Ok(())
}

#[test]
fn bare_and_typed_commits_reject_non_terminal_execution_without_calling_git()
-> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    initialize(repository.path())?;
    git_success(repository.path(), &["config", "core.editor", "false"])?;
    fs::write(repository.path().join("staged.txt"), "staged\n")?;
    git_success(repository.path(), &["add", "staged.txt"])?;

    for arguments in [&["commit"][..], &["commit", "--type", "feat"][..]] {
        let output = run(repository.path(), arguments)?;
        assert_eq!(output.status.code(), Some(1));
        assert!(stdout(&output).is_empty());
        assert_eq!(
            stderr(&output),
            "error: commit authoring requires an interactive terminal\n"
        );
        assert!(!head_exists(repository.path())?);
    }
    Ok(())
}
