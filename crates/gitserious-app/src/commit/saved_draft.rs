use crate::RepositoryRoot;

/// A message being written, kept so leaving Commit does not lose it. Text is stored as typed,
/// valid or not, so a half-written draft comes back as it was left.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedDraft {
    /// The taxonomy the draft was written in, and its fingerprint then, which tells whether
    /// the policy changed since.
    pub taxonomy: String,
    pub fingerprint: String,
    pub commit_type: String,
    /// The subject line after the type: `(scope): description` or `: description`.
    pub subject: String,
    /// Each property the type asked for, in the type's order.
    pub properties: Vec<SavedProperty>,
    pub breaking_change: String,
}

/// One property's text and, for a conditional property, whether the writer said it applies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedProperty {
    pub key: String,
    pub text: String,
    pub applies: Option<bool>,
}

/// Keeps one draft per work tree, outside anything Git commits.
pub trait DraftStore {
    /// The saved draft, or `None` when there is none.
    ///
    /// # Errors
    ///
    /// Returns why a saved draft could not be read.
    fn load(&self, root: &RepositoryRoot) -> Result<Option<SavedDraft>, String>;

    /// Replaces the saved draft with `draft`.
    ///
    /// # Errors
    ///
    /// Returns why the draft could not be written.
    fn save(&self, root: &RepositoryRoot, draft: &SavedDraft) -> Result<(), String>;

    /// Deletes the saved draft, if any.
    ///
    /// # Errors
    ///
    /// Returns why the draft could not be deleted.
    fn discard(&self, root: &RepositoryRoot) -> Result<(), String>;
}
