use std::convert::Infallible;
use std::fmt::Display;
use std::path::Path;

use crate::taxonomy_configuration_editor::Workspace;
use crate::{
    AuthoredCommit, CommitAuthoringContext, CommitOutput, CommitWriter, ConfigurationWorkspace,
    GlobalConfigurationStore, ProjectStateStore, RepositoryLocator, RepositoryStatus,
    RepositoryStatusReader, prepare_commit_with_taxonomy, validate_reviewed_commit,
};

/// Starting view for the unified terminal interface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteractiveEntry {
    Home,
    Config,
}

/// Presentation content supplied by the CLI without a TUI-to-CLI dependency.
#[derive(Clone, Debug)]
pub struct HelpPage {
    pub title: String,
    pub text: String,
}

/// Application services available to an interactive session.
pub trait InteractiveWorkspace {
    fn configuration(&self) -> &dyn ConfigurationWorkspace;
    fn repository_status(&self) -> Result<RepositoryStatus, String>;
    fn prepare_commit(&self) -> Result<CommitAuthoringContext, String>;
    fn submit_commit(&self, authored: &AuthoredCommit) -> Result<CommitOutput, String>;
}

/// Terminal adapter for the unified application.
pub trait InteractiveUi {
    fn run(
        &self,
        entry: InteractiveEntry,
        workspace: &dyn InteractiveWorkspace,
        help: &[HelpPage],
    ) -> Result<(), String>;
}

/// Opens an interactive session with application-owned persistence and Git services.
#[allow(clippy::too_many_arguments)]
pub fn run_interactive<L, S, G, W>(
    locator: &L,
    store: &S,
    global: &G,
    status: &dyn RepositoryStatusReader,
    writer: &W,
    start: &Path,
    ui: &dyn InteractiveUi,
    entry: InteractiveEntry,
    help: &[HelpPage],
) -> Result<(), String>
where
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectStateStore + ?Sized,
    S::Error: Display,
    G: GlobalConfigurationStore + ?Sized,
    G::Error: Display,
    W: CommitWriter + ?Sized,
    W::Error: Display,
{
    let workspace = InteractiveServices {
        config: Workspace {
            locator,
            project: store,
            global,
            start,
        },
        status,
        writer,
    };
    ui.run(entry, &workspace, help)
}

struct InteractiveServices<'a, L: ?Sized, S: ?Sized, G: ?Sized, W: ?Sized> {
    config: Workspace<'a, L, S, G>,
    status: &'a dyn RepositoryStatusReader,
    writer: &'a W,
}

impl<L, S, G, W> InteractiveWorkspace for InteractiveServices<'_, L, S, G, W>
where
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectStateStore + ?Sized,
    S::Error: Display,
    G: GlobalConfigurationStore + ?Sized,
    G::Error: Display,
    W: CommitWriter + ?Sized,
    W::Error: Display,
{
    fn configuration(&self) -> &dyn ConfigurationWorkspace {
        &self.config
    }

    fn repository_status(&self) -> Result<RepositoryStatus, String> {
        let root = self
            .config
            .locator
            .locate(self.config.start)
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
        prepare_commit_with_taxonomy::<L, S, Infallible, Infallible>(
            self.config.locator,
            self.config.project,
            self.config.start,
            None,
            None,
        )
        .map(|(_, context)| context)
        .map_err(|error| error.to_string())
    }

    fn submit_commit(&self, authored: &AuthoredCommit) -> Result<CommitOutput, String> {
        // Reload persisted policy on submission; unapplied Config drafts never influence authoring.
        let context = self.prepare_commit()?;
        let message = validate_reviewed_commit::<Infallible, Infallible, Infallible, Infallible>(
            &context, authored,
        )
        .map_err(|error| error.to_string())?;
        let root = self
            .config
            .locator
            .locate(self.config.start)
            .map_err(|error| error.to_string())?;
        self.writer
            .commit(&root, &message)
            .map_err(|error| error.to_string())
    }
}
