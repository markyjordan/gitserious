use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

const MESSAGE: &str = "feat(cli): create commits\n\nintent:\n  author a durable message\n\nbehavior:\n  commit the staged index\n";

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

fn configure_copy_editor(directory: &Path, message: &str) -> Result<(), Box<dyn Error>> {
    let document = directory.join("authored message.txt");
    let script = directory.join("editor script.sh");
    fs::write(&document, message)?;
    fs::write(&script, "#!/bin/sh\ncp \"$1\" \"$2\"\n")?;
    let editor = format!("sh {} {}", shell_quote(&script), shell_quote(&document));
    git_success(directory, &["config", "core.editor", &editor])?;
    Ok(())
}

fn shell_quote(path: &Path) -> String {
    let value = path.to_string_lossy();
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn commit_message(directory: &Path) -> Result<String, Box<dyn Error>> {
    let output = git_success(directory, &["cat-file", "commit", "HEAD"])?;
    let object = stdout(&output);
    object
        .split_once("\n\n")
        .map(|(_, message)| message.to_owned())
        .ok_or_else(|| "commit object has no message separator".into())
}

#[test]
fn type_option_opens_editor_and_commits_only_the_staged_index() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    initialize(repository.path())?;
    configure_copy_editor(repository.path(), MESSAGE)?;

    fs::write(repository.path().join("staged.txt"), "staged\n")?;
    fs::write(repository.path().join("unstaged.txt"), "unstaged\n")?;
    git_success(repository.path(), &["add", "staged.txt"])?;

    let output = run(repository.path(), &["commit", "--type", "feat"])?;

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("create commits"));
    assert!(stderr(&output).is_empty());
    assert_eq!(commit_message(repository.path())?, MESSAGE);
    let tree = stdout(&git_success(
        repository.path(),
        &["ls-tree", "--name-only", "HEAD"],
    )?);
    assert_eq!(tree, "staged.txt\n");
    assert!(repository.path().join("unstaged.txt").exists());
    Ok(())
}

#[test]
fn commit_requires_initialized_current_policy_before_opening_editor() -> Result<(), Box<dyn Error>>
{
    let repository = repository()?;
    configure_copy_editor(repository.path(), MESSAGE)?;

    let absent = run(repository.path(), &["commit", "--type", "feat"])?;
    assert_eq!(absent.status.code(), Some(1));
    assert!(stdout(&absent).is_empty());
    assert!(stderr(&absent).contains("not initialized"));

    initialize(repository.path())?;
    let lock = repository.path().join(".gitserious/gitserious.lock");
    let stale = fs::read_to_string(&lock)?.replace(
        "config-fingerprint = \"sha256:b",
        "config-fingerprint = \"sha256:a",
    );
    fs::write(lock, stale)?;
    let stale = run(repository.path(), &["commit", "--type", "feat"])?;
    assert_eq!(stale.status.code(), Some(1));
    assert!(stdout(&stale).is_empty());
    assert!(stderr(&stale).contains("stale"));
    assert!(
        git(repository.path(), &["rev-parse", "--verify", "HEAD"])
            .map(|output| !output.status.success())?
    );
    Ok(())
}

#[test]
fn unavailable_type_reports_policy_order_without_opening_editor() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    initialize(repository.path())?;
    git_success(repository.path(), &["config", "core.editor", "false"])?;

    let output = run(repository.path(), &["commit", "--type", "custom"])?;

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
    let error = stderr(&output);
    assert!(error.contains("custom"));
    assert!(error.contains("choose one of: feat, fix"));
    assert!(error.contains("revert"));
    assert!(!error.contains("configured Git editor exited"));
    Ok(())
}

#[test]
fn editor_cancellation_and_git_rejection_never_create_a_commit() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    initialize(repository.path())?;
    git_success(repository.path(), &["config", "core.editor", "true"])?;

    let cancelled = run(repository.path(), &["commit", "--type", "feat"])?;
    assert_eq!(cancelled.status.code(), Some(1));
    assert_eq!(stderr(&cancelled), "Commit cancelled.\n");

    configure_copy_editor(repository.path(), MESSAGE)?;
    fs::write(repository.path().join("staged.txt"), "staged\n")?;
    git_success(repository.path(), &["add", "staged.txt"])?;
    let hooks = repository.path().join(".git/hooks");
    let hook = hooks.join("pre-commit");
    fs::write(
        &hook,
        "#!/bin/sh\necho 'hook rejected commit' >&2\nexit 1\n",
    )?;
    make_executable(&hook)?;

    let rejected = run(repository.path(), &["commit", "--type", "feat"])?;
    assert_eq!(rejected.status.code(), Some(1));
    assert!(stdout(&rejected).is_empty());
    assert!(stderr(&rejected).contains("hook rejected commit"));
    assert!(
        git(repository.path(), &["rev-parse", "--verify", "HEAD"])
            .map(|output| !output.status.success())?
    );
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::PermissionsExt as _;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), Box<dyn Error>> {
    Ok(())
}
