use std::fmt::{self, Display};
use std::path::{Path, PathBuf};

use super::operation_refusal;
use crate::{
    AuthoredCommit, CommitAuthoringContext, CommitOutput, CommitWriter, GitOperation, IndexWriter,
    ProjectConfigurationStore, RepositoryLocator, RepositoryStatus, RepositoryStatusReader,
    WorktreePaths, prepare_commit_with_taxonomy, validate_reviewed_commit,
};

/// Application operations exposed to the Commit page.
pub trait CommitWorkspace {
    fn repository_status(&self) -> Result<RepositoryStatus, String>;
    /// The authoring context under persisted project policy, whatever the index holds,
    /// unless a Git operation in progress would make `git commit` write another kind of commit.
    fn prepare_commit(&self) -> Result<CommitAuthoringContext, String>;
    /// Writes the reviewed bytes once the index and persisted policy still accept them.
    fn submit_commit(&self, authored: &AuthoredCommit) -> Result<CommitOutput, SubmitError>;
    /// Files and directories the message can cite. Empty when this page has no reader.
    fn cite_paths(&self) -> Vec<String> {
        Vec::new()
    }
    /// Recent commits the message can cite. Empty when this page has no reader.
    fn cite_commits(&self) -> Vec<crate::CitedCommit> {
        Vec::new()
    }
    /// Whether this page can write the index, which its staging keys need.
    fn can_stage(&self) -> bool {
        false
    }
    /// Stages every change to tracked files, deletions included.
    fn stage_tracked(&self) -> Result<(), String> {
        Err(STAGING_UNAVAILABLE.into())
    }
    /// Stages `paths`, untracked files included.
    fn stage(&self, _paths: &[PathBuf]) -> Result<(), String> {
        Err(STAGING_UNAVAILABLE.into())
    }
    /// Unstages `paths`; a rename names both of its paths.
    fn unstage(&self, _paths: &[PathBuf]) -> Result<(), String> {
        Err(STAGING_UNAVAILABLE.into())
    }
}

const STAGING_UNAVAILABLE: &str = "This Commit page cannot write the index.";

/// Why a reviewed commit was not written.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SubmitError {
    /// An operation is in progress whose commit gitserious does not author.
    Operation(GitOperation),
    /// The index holds unmerged paths, in Git's order.
    Conflicts(Vec<PathBuf>),
    /// The index matches `HEAD`, so Git would record no change.
    NothingStaged,
    /// The repository, the persisted policy, the reviewed bytes, or Git refused the commit.
    Failed(String),
}

impl Display for SubmitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Operation(operation) => formatter.write_str(&operation_refusal(*operation)),
            Self::Conflicts(_) => formatter.write_str("Resolve Git conflicts before committing."),
            Self::NothingStaged => formatter.write_str("Nothing is staged to commit."),
            Self::Failed(reason) => formatter.write_str(reason),
        }
    }
}

impl std::error::Error for SubmitError {}

/// Commit operations over the repository that contains `start`.
pub struct CommitServices<'a, L: ?Sized, S: ?Sized, W: ?Sized> {
    locator: &'a L,
    store: &'a S,
    status: &'a dyn RepositoryStatusReader,
    writer: &'a W,
    start: &'a Path,
    paths: Option<&'a dyn WorktreePaths>,
    index: Option<&'a dyn IndexWriter>,
}

impl<'a, L, S, W> CommitServices<'a, L, S, W>
where
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectConfigurationStore + ?Sized,
    S::Error: Display,
    W: CommitWriter + ?Sized,
    W::Error: Display,
{
    pub fn new(
        locator: &'a L,
        store: &'a S,
        status: &'a dyn RepositoryStatusReader,
        writer: &'a W,
        start: &'a Path,
    ) -> Self {
        Self {
            locator,
            store,
            status,
            writer,
            start,
            paths: None,
            index: None,
        }
    }

    fn persisted_context(&self) -> Result<CommitAuthoringContext, String> {
        prepare_commit_with_taxonomy(self.locator, self.store, self.start)
            .map(|(_, context)| context)
            .map_err(|error| error.to_string())
    }

    /// Writes the index through `index` when Commit stages or unstages.
    #[must_use]
    pub fn staging(mut self, index: &'a dyn IndexWriter) -> Self {
        self.index = Some(index);
        self
    }

    /// Runs `write` on the index of the repository that contains `start`.
    fn write_index(
        &self,
        write: impl FnOnce(&dyn IndexWriter, &crate::RepositoryRoot) -> Result<(), String>,
    ) -> Result<(), String> {
        let index = self.index.ok_or(STAGING_UNAVAILABLE)?;
        let root = self
            .locator
            .locate(self.start)
            .map_err(|error| error.to_string())?;
        write(index, &root)
    }

    /// Uses `paths` when the editor asks what the message can cite.
    #[must_use]
    pub fn citing(mut self, paths: &'a dyn WorktreePaths) -> Self {
        self.paths = Some(paths);
        self
    }
}

impl<L, S, W> CommitWorkspace for CommitServices<'_, L, S, W>
where
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectConfigurationStore + ?Sized,
    S::Error: Display,
    W: CommitWriter + ?Sized,
    W::Error: Display,
{
    fn repository_status(&self) -> Result<RepositoryStatus, String> {
        let root = self
            .locator
            .locate(self.start)
            .map_err(|error| error.to_string())?;
        self.status.read(&root)
    }

    fn prepare_commit(&self) -> Result<CommitAuthoringContext, String> {
        if let Some(refusal) = self.repository_status()?.authoring_refusal() {
            return Err(refusal);
        }
        self.persisted_context()
    }

    fn submit_commit(&self, authored: &AuthoredCommit) -> Result<CommitOutput, SubmitError> {
        let snapshot = self.repository_status().map_err(SubmitError::Failed)?;
        if let Some(operation) = snapshot.foreign_commit_operation() {
            return Err(SubmitError::Operation(operation));
        }
        let conflicts = snapshot.conflicted_paths();
        if !conflicts.is_empty() {
            return Err(SubmitError::Conflicts(conflicts));
        }
        if !snapshot.has_staged_changes() {
            return Err(SubmitError::NothingStaged);
        }
        // Reload persisted policy on submission; unapplied Config drafts never influence authoring.
        let context = self.persisted_context().map_err(SubmitError::Failed)?;
        let message = validate_reviewed_commit::<L::Error, S::Error>(&context, authored)
            .map_err(|error| SubmitError::Failed(error.to_string()))?;
        self.writer
            .commit(&snapshot.root, &message)
            .map_err(|error| SubmitError::Failed(error.to_string()))
    }

    fn cite_paths(&self) -> Vec<String> {
        let Some(paths) = self.paths else {
            return Vec::new();
        };
        // A failed listing leaves the editor usable. The cite list is then empty.
        self.locator
            .locate(self.start)
            .ok()
            .and_then(|root| paths.paths(&root).ok())
            .unwrap_or_default()
    }

    fn can_stage(&self) -> bool {
        self.index.is_some()
    }

    fn stage_tracked(&self) -> Result<(), String> {
        self.write_index(|index, root| index.stage_tracked(root))
    }

    fn stage(&self, paths: &[PathBuf]) -> Result<(), String> {
        self.write_index(|index, root| index.stage(root, paths))
    }

    fn unstage(&self, paths: &[PathBuf]) -> Result<(), String> {
        self.write_index(|index, root| index.unstage(root, paths))
    }

    fn cite_commits(&self) -> Vec<crate::CitedCommit> {
        let Some(paths) = self.paths else {
            return Vec::new();
        };
        self.locator
            .locate(self.start)
            .ok()
            .and_then(|root| paths.commits(&root).ok())
            .unwrap_or_default()
    }
}
