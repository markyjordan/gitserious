use gitserious_core::{CommitTypeDefinition, CommitTypeId};

/// The user's result from an interactive commit-type selection adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitTypeSelection {
    /// Continue authoring with the selected open type identifier.
    Selected(CommitTypeId),
    /// Stop the commit workflow without creating a commit.
    Cancelled,
}

/// Selects one commit type when the delivery request does not provide one.
pub trait CommitTypeSelector {
    /// The adapter-specific interaction failure.
    type Error;

    /// Selects from effective definitions in project-policy order.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when selection cannot be presented
    /// or terminal input cannot be read safely.
    fn select(
        &self,
        definitions: &[CommitTypeDefinition],
    ) -> Result<CommitTypeSelection, Self::Error>;
}
