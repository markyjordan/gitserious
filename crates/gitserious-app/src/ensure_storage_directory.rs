use crate::{DirectoryCreator, StorageDirectory};

/// Ensures one selected storage directory exists.
///
/// # Errors
///
/// Returns the creator adapter's error unchanged when creation fails.
pub fn ensure_storage_directory<C>(
    creator: &C,
    directory: &StorageDirectory,
) -> Result<(), C::Error>
where
    C: DirectoryCreator + ?Sized,
{
    creator.ensure(directory)
}
