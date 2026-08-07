use crate::StorageDirectory;

/// Creates application-selected storage directories.
pub trait DirectoryCreator {
    /// The adapter-specific failure returned by directory creation.
    type Error;

    /// Creates one selected directory and any missing parents.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when the directory cannot be
    /// created or accepted as existing.
    fn ensure(&self, directory: &StorageDirectory) -> Result<(), Self::Error>;
}
