use gitserious_core::{CommitTypeDefinition, CommitTypeId};

/// Read access to the effective catalog of commit-type definitions.
///
/// Adapters own storage and configuration concerns. Application use cases see
/// only owned domain definitions and the adapter's error type.
pub trait CommitTypeCatalog {
    /// The adapter-specific failure returned by catalog operations.
    type Error;

    /// Finds one commit type by its open identifier.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when the catalog cannot complete
    /// the lookup. A successful lookup can still return `None`.
    fn find(&self, id: &CommitTypeId) -> Result<Option<CommitTypeDefinition>, Self::Error>;

    /// Lists commit types in the catalog's deterministic order.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when the catalog cannot provide
    /// its definitions.
    fn list(&self) -> Result<Vec<CommitTypeDefinition>, Self::Error>;
}
