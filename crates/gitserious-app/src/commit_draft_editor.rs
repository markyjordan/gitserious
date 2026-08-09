use crate::RepositoryRoot;

/// Opens and reads an editor document for one repository commit draft.
pub trait CommitDraftEditor {
    /// The adapter-specific editor or temporary-document failure.
    type Error;

    /// Opens `document` in the configured editor and returns its saved text.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when the document cannot be
    /// prepared, the editor cannot be launched, or the editor exits unsuccessfully.
    fn edit(&self, root: &RepositoryRoot, document: &str) -> Result<String, Self::Error>;
}
