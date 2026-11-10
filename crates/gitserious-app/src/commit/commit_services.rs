use std::convert::Infallible;
use std::fmt::Display;
use std::path::Path;

use crate::{
    AuthoredCommit, CommitAuthoringContext, CommitOutput, CommitWriter, ProjectConfigurationStore,
    RepositoryLocator, RepositoryStatus, RepositoryStatusReader, prepare_commit_with_taxonomy,
    validate_reviewed_commit,
};

/// Commit operations over the repository that contains `start`.
pub struct CommitServices<'a, L: ?Sized, S: ?Sized, W: ?Sized> {
    locator: &'a L,
    store: &'a S,
    status: &'a dyn RepositoryStatusReader,
    writer: &'a W,
    start: &'a Path,
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
        }
    }

    pub fn repository_status(&self) -> Result<RepositoryStatus, String> {
        let root = self
            .locator
            .locate(self.start)
            .map_err(|error| error.to_string())?;
        self.status.read(&root)
    }

    pub fn prepare_commit(&self) -> Result<CommitAuthoringContext, String> {
        let snapshot = self.repository_status()?;
        if snapshot.has_conflicts() {
            return Err("Resolve Git conflicts before starting a commit.".into());
        }
        if !snapshot.has_staged_changes() {
            return Err("Stage changes with Git, then refresh to start a commit.".into());
        }
        prepare_commit_with_taxonomy::<L, S, Infallible, Infallible>(
            self.locator,
            self.store,
            self.start,
        )
        .map(|(_, context)| context)
        .map_err(|error| error.to_string())
    }

    pub fn submit_commit(&self, authored: &AuthoredCommit) -> Result<CommitOutput, String> {
        // Reload persisted policy on submission; unapplied Config drafts never influence authoring.
        let context = self.prepare_commit()?;
        let message = validate_reviewed_commit::<Infallible, Infallible, Infallible, Infallible>(
            &context, authored,
        )
        .map_err(|error| error.to_string())?;
        let root = self
            .locator
            .locate(self.start)
            .map_err(|error| error.to_string())?;
        self.writer
            .commit(&root, &message)
            .map_err(|error| error.to_string())
    }
}
