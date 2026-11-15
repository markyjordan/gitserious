use std::fmt::Display;
use std::path::Path;

use crate::{
    AuthoredCommit, CommitAuthoringContext, CommitOutput, CommitWriter, ProjectConfigurationStore,
    RepositoryLocator, RepositoryStatus, RepositoryStatusReader, WorktreePaths,
    prepare_commit_with_taxonomy, validate_reviewed_commit,
};

/// Application operations exposed to the Commit page.
pub trait CommitWorkspace {
    fn repository_status(&self) -> Result<RepositoryStatus, String>;
    /// The authoring context for a staged, conflict-free index under persisted project policy.
    fn prepare_commit(&self) -> Result<CommitAuthoringContext, String>;
    /// Writes the reviewed bytes once the index and persisted policy still accept them.
    fn submit_commit(&self, authored: &AuthoredCommit) -> Result<CommitOutput, String>;
    /// Files and directories the message can cite. Empty when this page has no reader.
    fn cite_paths(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Commit operations over the repository that contains `start`.
pub struct CommitServices<'a, L: ?Sized, S: ?Sized, W: ?Sized> {
    locator: &'a L,
    store: &'a S,
    status: &'a dyn RepositoryStatusReader,
    writer: &'a W,
    start: &'a Path,
    paths: Option<&'a dyn WorktreePaths>,
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
        }
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
        let snapshot = self.repository_status()?;
        if snapshot.has_conflicts() {
            return Err("Resolve Git conflicts before starting a commit.".into());
        }
        if !snapshot.has_staged_changes() {
            return Err("Stage changes with Git, then refresh to start a commit.".into());
        }
        prepare_commit_with_taxonomy(self.locator, self.store, self.start)
            .map(|(_, context)| context)
            .map_err(|error| error.to_string())
    }

    fn submit_commit(&self, authored: &AuthoredCommit) -> Result<CommitOutput, String> {
        // Reload persisted policy on submission; unapplied Config drafts never influence authoring.
        let context = self.prepare_commit()?;
        let message = validate_reviewed_commit::<L::Error, S::Error>(&context, authored)
            .map_err(|error| error.to_string())?;
        let root = self
            .locator
            .locate(self.start)
            .map_err(|error| error.to_string())?;
        self.writer
            .commit(&root, &message)
            .map_err(|error| error.to_string())
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
}
