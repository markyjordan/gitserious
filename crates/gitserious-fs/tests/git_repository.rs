mod support;

use std::error::Error;
use std::fs;
use std::path::Path;

#[cfg(target_os = "linux")]
use std::ffi::OsString;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

use gitserious_app::{MissingWorkTree, RepositoryLocator};
use gitserious_fs::{GitRepositoryError, GitRepositoryLocator};
use support::{git, git_command, init, repository};

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
    init(&inner)?;

    assert_same_existing_path(GitRepositoryLocator.locate(&inner)?.as_path(), &inner)?;
    Ok(())
}

#[test]
fn linked_worktree_resolves_its_own_root() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    git(
        repository.path(),
        &["commit", "-q", "--allow-empty", "-m", "initial"],
    )?;
    let linked_parent = tempfile::tempdir()?;
    let linked = linked_parent.path().join("linked worktree");
    let status = git_command(repository.path())
        .args(["worktree", "add", "-q", "-b", "linked-test"])
        .arg(&linked)
        .status()?;
    assert!(status.success());

    assert_same_existing_path(GitRepositoryLocator.locate(&linked)?.as_path(), &linked)?;
    Ok(())
}

fn missing(path: &Path) -> Option<MissingWorkTree> {
    GitRepositoryLocator
        .locate(path)
        .err()
        .and_then(|error| GitRepositoryLocator::missing_work_tree(&error))
}

#[test]
fn each_directory_without_a_work_tree_says_why() -> Result<(), Box<dyn Error>> {
    let plain = tempfile::tempdir()?;
    assert_eq!(missing(plain.path()), Some(MissingWorkTree::NotRepository));
    assert!(matches!(
        GitRepositoryLocator.locate(plain.path()),
        Err(GitRepositoryError::NotWorkTree { .. })
    ));

    let bare = tempfile::tempdir()?;
    git(bare.path(), &["init", "-q", "--bare"])?;
    assert_eq!(missing(bare.path()), Some(MissingWorkTree::Bare));
    assert_eq!(
        missing(&bare.path().join("objects")),
        Some(MissingWorkTree::Bare)
    );

    let repository = repository()?;
    let Some(MissingWorkTree::InsideGitDir {
        worktree: Some(worktree),
    }) = missing(&repository.path().join(".git"))
    else {
        return Err("a .git directory names its work tree".into());
    };
    assert_same_existing_path(&worktree, repository.path())?;

    let broken = tempfile::tempdir()?;
    fs::write(broken.path().join(".git"), "not a gitdir pointer\n")?;
    assert_eq!(
        missing(broken.path()),
        None,
        "a broken .git file stays unclassified"
    );
    Ok(())
}

#[test]
fn a_filesystem_root_without_a_repository_is_never_offered_git_init() {
    let root = std::env::temp_dir()
        .ancestors()
        .last()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    // A machine whose root is itself a repository has nothing to check here.
    if let Some(missing) = missing(&root) {
        assert_eq!(missing, MissingWorkTree::HomeOrRoot);
    }
}

#[test]
fn preserves_worktree_paths_with_spaces() -> Result<(), Box<dyn Error>> {
    let parent = tempfile::tempdir()?;
    let repository = parent.path().join("repository with spaces");
    fs::create_dir(&repository)?;
    init(&repository)?;

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
    let status = git_command(&repository)
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
