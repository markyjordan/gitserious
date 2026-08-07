use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;

#[cfg(target_os = "linux")]
use std::ffi::OsString;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

use gitserious_app::RepositoryLocator;
use gitserious_fs::{GitRepositoryError, GitRepositoryLocator};
use tempfile::TempDir;

fn git(arguments: &[&str], directory: &Path) -> Result<(), Box<dyn Error>> {
    let status = Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .status()?;
    if !status.success() {
        return Err(format!("git {arguments:?} failed with {status}").into());
    }
    Ok(())
}

fn repository() -> Result<TempDir, Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    git(&["init", "-q"], directory.path())?;
    Ok(directory)
}

fn assert_same_existing_path(actual: &Path, expected: &Path) -> Result<(), Box<dyn Error>> {
    assert_eq!(fs::canonicalize(actual)?, fs::canonicalize(expected)?);
    Ok(())
}

#[test]
fn locates_root_from_root_and_nested_directories() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let nested = repository.path().join("one/two");
    fs::create_dir_all(&nested)?;
    let expected = fs::canonicalize(repository.path())?;

    assert_same_existing_path(
        GitRepositoryLocator.locate(repository.path())?.as_path(),
        &expected,
    )?;
    assert_same_existing_path(GitRepositoryLocator.locate(&nested)?.as_path(), &expected)?;
    Ok(())
}

#[test]
fn nested_repository_selects_the_innermost_worktree() -> Result<(), Box<dyn Error>> {
    let outer = repository()?;
    let inner = outer.path().join("nested");
    fs::create_dir(&inner)?;
    git(&["init", "-q"], &inner)?;

    assert_same_existing_path(GitRepositoryLocator.locate(&inner)?.as_path(), &inner)?;
    Ok(())
}

#[test]
fn linked_worktree_resolves_its_own_root() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    git(
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "initial",
        ],
        repository.path(),
    )?;
    let linked_parent = tempfile::tempdir()?;
    let linked = linked_parent.path().join("linked worktree");
    let status = Command::new("git")
        .args(["worktree", "add", "-q", "-b", "linked-test"])
        .arg(&linked)
        .current_dir(repository.path())
        .status()?;
    assert!(status.success());

    assert_same_existing_path(GitRepositoryLocator.locate(&linked)?.as_path(), &linked)?;
    Ok(())
}

#[test]
fn rejects_non_repositories_and_bare_repositories() -> Result<(), Box<dyn Error>> {
    let plain = tempfile::tempdir()?;
    assert!(matches!(
        GitRepositoryLocator.locate(plain.path()),
        Err(GitRepositoryError::NotWorkTree { .. })
    ));

    let bare = tempfile::tempdir()?;
    git(&["init", "-q", "--bare"], bare.path())?;
    assert!(matches!(
        GitRepositoryLocator.locate(bare.path()),
        Err(GitRepositoryError::NotWorkTree { .. })
    ));
    Ok(())
}

#[test]
fn preserves_worktree_paths_with_spaces() -> Result<(), Box<dyn Error>> {
    let parent = tempfile::tempdir()?;
    let repository = parent.path().join("repository with spaces");
    fs::create_dir(&repository)?;
    git(&["init", "-q"], &repository)?;

    assert_same_existing_path(
        GitRepositoryLocator.locate(&repository)?.as_path(),
        &repository,
    )?;
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn preserves_non_utf8_unix_worktree_paths() -> Result<(), Box<dyn Error>> {
    use std::os::unix::ffi::OsStringExt;

    let parent = tempfile::tempdir()?;
    let name = OsString::from_vec(b"repository-\xff".to_vec());
    let repository = parent.path().join(PathBuf::from(name.clone()));
    fs::create_dir(&repository)?;
    let status = Command::new("git")
        .arg("-C")
        .arg(&repository)
        .args(["init", "-q"])
        .current_dir(parent.path())
        .status()?;
    assert!(status.success());

    assert_eq!(
        GitRepositoryLocator.locate(&repository)?.as_path(),
        fs::canonicalize(parent.path())?.join(PathBuf::from(name))
    );
    Ok(())
}
