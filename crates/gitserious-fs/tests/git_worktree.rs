use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use gitserious_app::{RepositoryRoot, WorktreePaths};
use gitserious_fs::GitWorktreePaths;
use tempfile::TempDir;

fn git(directory: &Path, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(output)
}

fn repository() -> Result<TempDir, Box<dyn Error>> {
    let temp = TempDir::new()?;
    git(temp.path(), &["init", "-b", "main"])?;
    git(temp.path(), &["config", "user.name", "Cite Test"])?;
    git(
        temp.path(),
        &["config", "user.email", "cite@example.invalid"],
    )?;
    git(temp.path(), &["config", "commit.gpgsign", "false"])?;
    Ok(temp)
}

#[test]
fn lists_tracked_and_untracked_paths_and_skips_ignored_files() -> Result<(), Box<dyn Error>> {
    let temp = repository()?;
    fs::create_dir_all(temp.path().join("src/net"))?;
    fs::write(temp.path().join("src/net/retry.rs"), "retry\n")?;
    fs::write(temp.path().join(".gitignore"), "secret.log\n")?;
    git(temp.path(), &["add", "src/net/retry.rs", ".gitignore"])?;
    git(temp.path(), &["commit", "-m", "initial"])?;
    fs::create_dir(temp.path().join("tests"))?;
    fs::write(temp.path().join("tests/retry.rs"), "test\n")?;
    fs::write(temp.path().join("secret.log"), "hidden\n")?;

    let root = RepositoryRoot::new(temp.path().to_path_buf())?;
    let paths = GitWorktreePaths.paths(&root)?;

    assert_eq!(
        paths,
        [
            ".gitignore",
            "src/",
            "src/net/",
            "src/net/retry.rs",
            "tests/",
            "tests/retry.rs",
        ]
    );
    Ok(())
}

#[test]
fn lists_the_newest_commits_and_stops_after_two_hundred() -> Result<(), Box<dyn Error>> {
    let temp = repository()?;
    for number in 0..=200 {
        let message = format!("commit {number}");
        git(temp.path(), &["commit", "--allow-empty", "-m", &message])?;
    }
    let root = RepositoryRoot::new(temp.path().to_path_buf())?;
    let commits = GitWorktreePaths.commits(&root)?;
    let head = String::from_utf8(git(temp.path(), &["rev-parse", "--short", "HEAD"])?.stdout)?;
    assert_eq!(commits.len(), 200);
    assert_eq!(
        commits.first().map(|commit| commit.sha.as_str()),
        Some(head.trim())
    );
    assert_eq!(
        commits.first().map(|commit| commit.subject.as_str()),
        Some("commit 200")
    );
    assert_eq!(
        commits.last().map(|commit| commit.subject.as_str()),
        Some("commit 1")
    );
    assert!(commits.iter().all(|commit| commit.subject != "commit 0"));
    Ok(())
}

#[test]
fn an_unborn_repository_does_not_list_commits() -> Result<(), Box<dyn Error>> {
    let temp = repository()?;
    let root = RepositoryRoot::new(temp.path().to_path_buf())?;
    assert!(GitWorktreePaths.commits(&root).is_err());
    Ok(())
}
