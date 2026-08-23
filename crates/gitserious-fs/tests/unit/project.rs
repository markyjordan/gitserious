use std::error::Error;
use std::fs;

use crate::ProjectStateError;
use crate::project::rollback_file;

#[test]
fn rollback_failure_preserves_the_original_error_and_failed_path() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let non_file = root.path().join("rollback-target");
    fs::create_dir(&non_file)?;
    let original_path = root.path().join("original-target");

    let error = rollback_file(
        &non_file,
        ProjectStateError::TemporaryFileUnavailable(original_path.clone()),
    );

    match error {
        ProjectStateError::Rollback {
            original,
            path,
            source: _,
        } => {
            assert!(matches!(
                *original,
                ProjectStateError::TemporaryFileUnavailable(ref observed)
                    if observed == &original_path
            ));
            assert_eq!(path, non_file);
        }
        other => return Err(format!("unexpected error: {other}").into()),
    }
    Ok(())
}
