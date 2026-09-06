use crate::{AuthoredCommit, CommitAuthoringContext, CommitAuthoringOutcome};
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

    /// Authors within a captured set of project template choices.
    ///
    /// The compatibility implementation delegates to the initial template's
    /// definitions. Interactive template switching can override this method.
    ///
    /// # Errors
    /// Returns the adapter's interaction error unchanged.
    fn author_with_context(
        &self,
        context: &CommitAuthoringContext,
    ) -> Result<CommitAuthoringOutcome, Self::Error> {
        let template = context.initial_template();
        self.author(template.definitions(), context.preselected_type())
            .map(|outcome| match outcome {
                CommitDraftAuthorOutcome::Authored(draft) => CommitAuthoringOutcome::Authored(
                    AuthoredCommit::new(template.id().clone(), draft),
                ),
                CommitDraftAuthorOutcome::Cancelled => CommitAuthoringOutcome::Cancelled,
            })
    }
}
