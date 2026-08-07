use std::error::Error;
use std::fs;
use std::io;

use gitserious_app::{GlobalPaths, ensure_storage_directory};

use crate::LocalDirectoryCreator;
use crate::tests::support::TestDirectory;

fn paths_beneath(root: &TestDirectory) -> GlobalPaths {
    GlobalPaths::new(
        root.path().join("config"),
        root.path().join("data"),
        root.path().join("state"),
        root.path().join("cache"),
    )
}

#[test]
fn creating_one_directory_does_not_create_its_siblings() -> Result<(), Box<dyn Error>> {
    let root = TestDirectory::new("selected-directory")?;
    let paths = paths_beneath(&root);

    ensure_storage_directory(&LocalDirectoryCreator, paths.state())?;

    assert!(paths.state().as_path().is_dir());
    assert!(!paths.config().as_path().exists());
    assert!(!paths.data().as_path().exists());
    assert!(!paths.cache().as_path().exists());
    Ok(())
}

#[test]
fn creation_is_recursive_and_idempotent() -> Result<(), Box<dyn Error>> {
    let root = TestDirectory::new("recursive-directory")?;
    let paths = GlobalPaths::new(
        root.path().join("parent/config"),
        root.path().join("parent/data"),
        root.path().join("parent/state/child"),
        root.path().join("parent/cache"),
    );

    ensure_storage_directory(&LocalDirectoryCreator, paths.state())?;
    ensure_storage_directory(&LocalDirectoryCreator, paths.state())?;

    assert!(paths.state().as_path().is_dir());
    Ok(())
}

#[test]
fn a_file_collision_is_reported_by_the_adapter() -> Result<(), Box<dyn Error>> {
    let root = TestDirectory::new("file-collision")?;
    let paths = paths_beneath(&root);
    fs::write(paths.cache().as_path(), b"not a directory")?;

    let error = match ensure_storage_directory(&LocalDirectoryCreator, paths.cache()) {
        Ok(()) => {
            return Err(io::Error::other("a file was accepted as a directory").into());
        }
        Err(error) => error,
    };

    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    Ok(())
}

#[cfg(unix)]
#[test]
fn new_directories_request_private_permissions() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::PermissionsExt;

    let root = TestDirectory::new("private-permissions")?;
    let paths = GlobalPaths::new(
        root.path().join("parent/config"),
        root.path().join("parent/data"),
        root.path().join("parent/state/child"),
        root.path().join("parent/cache"),
    );

    ensure_storage_directory(&LocalDirectoryCreator, paths.state())?;

    let parent_mode = fs::metadata(root.path().join("parent"))?
        .permissions()
        .mode()
        & 0o777;
    let state_mode = fs::metadata(paths.state().as_path())?.permissions().mode() & 0o777;
    assert_eq!(parent_mode, 0o700);
    assert_eq!(state_mode, 0o700);
    Ok(())
}

#[cfg(unix)]
#[test]
fn existing_directory_permissions_are_unchanged() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::PermissionsExt;

    let root = TestDirectory::new("existing-permissions")?;
    let paths = paths_beneath(&root);
    fs::create_dir(paths.config().as_path())?;
    fs::set_permissions(paths.config().as_path(), fs::Permissions::from_mode(0o750))?;

    ensure_storage_directory(&LocalDirectoryCreator, paths.config())?;

    let mode = fs::metadata(paths.config().as_path())?.permissions().mode() & 0o777;
    assert_eq!(mode, 0o750);
    Ok(())
}
