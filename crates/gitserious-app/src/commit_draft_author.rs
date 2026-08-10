use gitserious_core::{CommitDraft, CommitTypeDefinition};

/// The user's result from one complete commit-draft authoring interaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitDraftAuthorOutcome {
    /// Continue the commit workflow with the authored structured draft.
    Authored(CommitDraft),
    /// Stop the commit workflow without creating a commit.
    Cancelled,
}

/// Authors a structured commit draft from the effective project policy.
pub trait CommitDraftAuthor {
    /// The adapter-specific interaction failure.
    type Error;

    /// Authors a draft from the effective definitions and optional CLI preselection.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when authoring cannot be presented
    /// or user input cannot be read safely.
    fn author(
        &self,
        definitions: &[CommitTypeDefinition],
        preselected: Option<&CommitTypeDefinition>,
    ) -> Result<CommitDraftAuthorOutcome, Self::Error>;
}
