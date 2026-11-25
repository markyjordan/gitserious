use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use gitserious_app::{BranchState, GitOperation, RepositoryRoot, RepositoryStatusReader};
use gitserious_fs::GitRepositoryStatusReader;
use tempfile::TempDir;

// Windows file names cannot contain line breaks; the parser's unit tests cover them there.
#[cfg(unix)]
const RENAMED: &str = "new\nname";
#[cfg(not(unix))]
const RENAMED: &str = "new name";
#[cfg(unix)]
const UNTRACKED: &str = "untracked/a b\nc";
#[cfg(not(unix))]
const UNTRACKED: &str = "untracked/a b c";

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
    git(temp.path(), &["config", "user.name", "Status Test"])?;
    git(
        temp.path(),
        &["config", "user.email", "status@example.invalid"],
    )?;
    git(temp.path(), &["config", "commit.gpgsign", "false"])?;
    Ok(temp)
}

#[test]
fn real_status_preserves_index_and_reports_partial_staging_rename_untracked_and_detached()
-> Result<(), Box<dyn Error>> {
    let temp = repository()?;
    let root = RepositoryRoot::new(temp.path().to_path_buf())?;
    let reader = GitRepositoryStatusReader;
    assert!(matches!(reader.read(&root)?.branch, BranchState::Unborn(_)));
    assert!(!temp.path().join(".git/index").exists());
    fs::create_dir(temp.path().join("src"))?;
    fs::write(temp.path().join("src/file.rs"), "initial\n")?;
    fs::write(temp.path().join("old name"), "rename me\n")?;
    git(temp.path(), &["add", "."])?;
    git(temp.path(), &["commit", "-m", "initial"])?;
    fs::write(temp.path().join("src/file.rs"), "staged\n")?;
    git(temp.path(), &["add", "src/file.rs"])?;
    fs::write(temp.path().join("src/file.rs"), "unstaged\n")?;
    git(temp.path(), &["mv", "old name", RENAMED])?;
    fs::create_dir(temp.path().join("untracked"))?;
    fs::write(temp.path().join(UNTRACKED), "new\n")?;
    let before = fs::read(temp.path().join(".git/index"))?;
    let status = reader.read(&root)?;
    assert_eq!(fs::read(temp.path().join(".git/index"))?, before);
    let partial = status
        .entries
        .iter()
        .find(|entry| entry.path == Path::new("src/file.rs"))
        .ok_or("missing partial staging")?;
    assert_eq!((partial.index, partial.worktree), ('M', 'M'));
    let rename = status
        .entries
        .iter()
        .find(|entry| entry.original_path.is_some())
        .ok_or("missing rename")?;
    assert_eq!(rename.path, Path::new(RENAMED));
    assert_eq!(rename.original_path.as_deref(), Some(Path::new("old name")));
    assert!(
        status
            .entries
            .iter()
            .any(|entry| entry.untracked && entry.path == Path::new(UNTRACKED))
    );
    assert!(status.has_staged_changes());
    git(temp.path(), &["checkout", "--detach"])?;
    assert!(matches!(
        reader.read(&root)?.branch,
        BranchState::Detached(_)
    ));
    Ok(())
}

#[test]
fn real_conflict_is_reported_without_refreshing_the_index() -> Result<(), Box<dyn Error>> {
    let temp = repository()?;
    fs::write(temp.path().join("file"), "base\n")?;
    git(temp.path(), &["add", "file"])?;
    git(temp.path(), &["commit", "-m", "base"])?;
    git(temp.path(), &["checkout", "-b", "other"])?;
    fs::write(temp.path().join("file"), "other\n")?;
    git(temp.path(), &["commit", "-am", "other"])?;
    git(temp.path(), &["checkout", "main"])?;
    fs::write(temp.path().join("file"), "main\n")?;
    git(temp.path(), &["commit", "-am", "main"])?;
    let output = Command::new("git")
        .arg("-C")
        .arg(temp.path())
        .args(["merge", "other"])
        .output()?;
    assert!(!output.status.success());
    let before = fs::read(temp.path().join(".git/index"))?;
    let status =
        GitRepositoryStatusReader.read(&RepositoryRoot::new(temp.path().to_path_buf())?)?;
    assert_eq!(status.conflicted_paths(), [PathBuf::from("file")]);
    assert_eq!(status.operation, Some(GitOperation::Merge));
    assert!(!status.has_staged_changes());
    assert_eq!(fs::read(temp.path().join(".git/index"))?, before);
    Ok(())
}

/// Runs git in `directory` expecting it to stop, as a conflicted or interrupted operation does.
fn git_stops(directory: &Path, arguments: &[&str]) -> Result<(), Box<dyn Error>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_EDITOR", "true")
        .output()?;
    assert!(!output.status.success(), "git {arguments:?} did not stop");
    Ok(())
}

/// `main` and `other` each change `file` from a shared base, so combining them conflicts.
fn diverged(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::write(directory.join("file"), "base\n")?;
    git(directory, &["add", "file"])?;
    git(directory, &["commit", "-m", "base"])?;
    git(directory, &["checkout", "-b", "other"])?;
    fs::write(directory.join("file"), "other\n")?;
    git(directory, &["commit", "-am", "other"])?;
    git(directory, &["checkout", "main"])?;
    fs::write(directory.join("file"), "main\n")?;
    git(directory, &["commit", "-am", "main"])?;
    Ok(())
}

fn operation(directory: &Path) -> Result<Option<GitOperation>, Box<dyn Error>> {
    Ok(GitRepositoryStatusReader
        .read(&RepositoryRoot::new(directory.to_path_buf())?)?
        .operation)
}

#[test]
fn each_stopped_operation_is_named_and_a_clean_repository_has_none() -> Result<(), Box<dyn Error>> {
    let stops: [(&[&str], GitOperation); 4] = [
        (&["merge", "other"], GitOperation::Merge),
        (&["cherry-pick", "other"], GitOperation::CherryPick),
        // Reverting the base that created the file conflicts with main's change to it.
        (&["revert", "--no-edit", "HEAD~1"], GitOperation::Revert),
        (&["rebase", "other"], GitOperation::Rebase),
    ];
    for (arguments, expected) in stops {
        let temp = repository()?;
        diverged(temp.path())?;
        assert_eq!(operation(temp.path())?, None, "{arguments:?}");
        git_stops(temp.path(), arguments)?;
        assert_eq!(operation(temp.path())?, Some(expected), "{arguments:?}");
    }
    Ok(())
}

#[test]
fn a_rebase_stopped_without_conflicts_is_a_rebase_with_a_clean_index() -> Result<(), Box<dyn Error>>
{
    let temp = repository()?;
    diverged(temp.path())?;
    git_stops(temp.path(), &["rebase", "--exec", "exit 1", "HEAD~1"])?;

    let status =
        GitRepositoryStatusReader.read(&RepositoryRoot::new(temp.path().to_path_buf())?)?;
    assert_eq!(status.operation, Some(GitOperation::Rebase));
    assert!(status.conflicted_paths().is_empty());
    Ok(())
}

#[test]
fn a_linked_worktree_reads_its_own_operation() -> Result<(), Box<dyn Error>> {
    let temp = repository()?;
    diverged(temp.path())?;
    let linked = TempDir::new()?;
    let linked_path = linked.path().join("linked");
    git(
        temp.path(),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "linked",
            &linked_path.to_string_lossy(),
            "main",
        ],
    )?;

    git_stops(&linked_path, &["merge", "other"])?;

    assert_eq!(operation(&linked_path)?, Some(GitOperation::Merge));
    assert_eq!(operation(temp.path())?, None);
    Ok(())
}
