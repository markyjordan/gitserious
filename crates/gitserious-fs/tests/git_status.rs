use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use gitserious_app::{BranchState, RepositoryRoot, RepositoryStatusReader};
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
    assert!(status.has_conflicts());
    assert!(!status.has_staged_changes());
    assert_eq!(fs::read(temp.path().join(".git/index"))?, before);
    Ok(())
}
