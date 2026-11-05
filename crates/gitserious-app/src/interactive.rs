use std::convert::Infallible;
use std::fmt::{self, Debug, Display, Formatter};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitserious_core::{Taxonomy, TaxonomyId, TaxonomyVersion};

use crate::taxonomy_configuration_editor::Workspace;
use crate::{
    AuthoredCommit, CommitAuthoringContext, CommitOutput, CommitWriter, ConfigurationWorkspace,
    GlobalConfigurationStore, ProjectConfiguration, ProjectConfigurationStore,
    RepositoryInitializer, RepositoryLocator, RepositoryRoot, RepositoryStatus,
    RepositoryStatusReader, prepare_commit_with_taxonomy, validate_reviewed_commit,
};

/// The project file every gitserious repository commits.
const POLICY_FILE: &str = "gitserious.toml";

/// Starting view for the unified terminal interface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteractiveEntry {
    Home,
    Config,
}

/// A run of help text; `strong` marks a heading or a command or flag name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelpSpan {
    pub text: String,
    pub strong: bool,
}

/// One line of help, already wrapped to the width it was rendered at.
pub type HelpLine = Vec<HelpSpan>;

/// Presentation content supplied by the CLI without a TUI-to-CLI dependency.
#[derive(Clone)]
pub struct HelpPage {
    pub title: String,
    /// The command whose help the page shows; the overview documents none.
    pub command: Option<String>,
    render: Arc<dyn Fn(u16) -> Vec<HelpLine> + Send + Sync>,
}

impl HelpPage {
    pub fn new(
        title: impl Into<String>,
        command: Option<&str>,
        render: impl Fn(u16) -> Vec<HelpLine> + Send + Sync + 'static,
    ) -> Self {
        Self {
            title: title.into(),
            command: command.map(str::to_owned),
            render: Arc::new(render),
        }
    }

    /// The page wrapped to `width` columns.
    #[must_use]
    pub fn lines(&self, width: u16) -> Vec<HelpLine> {
        (self.render)(width)
    }
}

impl Debug for HelpPage {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HelpPage")
            .field("title", &self.title)
            .field("command", &self.command)
            .finish_non_exhaustive()
    }
}

/// What Home shows about the directory gitserious started in.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HomeState {
    /// No Git worktree contains the directory.
    NoRepository { directory: PathBuf },
    /// A Git worktree without a gitserious project.
    Uninitialized { root: RepositoryRoot },
    /// A gitserious project and the taxonomy commits start from.
    Initialized {
        root: RepositoryRoot,
        default: TaxonomyId,
        version: TaxonomyVersion,
        /// Git does not track `gitserious.toml` yet, so the policy stays local.
        policy_untracked: bool,
    },
    /// Git or the project file could not be read.
    Unavailable { directory: PathBuf, error: String },
}

/// Application services available to an interactive session.
pub trait InteractiveWorkspace {
    fn home(&self) -> HomeState;
    /// Creates the project from `configuration`, and the Git repository first when there is
    /// none; an existing project is never rewritten.
    fn initialize(&self, configuration: &ProjectConfiguration) -> Result<(), String>;
    /// The Library's custom taxonomies a project can include, those with at least one type.
    fn library_taxonomies(&self) -> Result<Vec<Taxonomy>, String>;
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
    initializer: &dyn RepositoryInitializer,
    writer: &W,
    start: &Path,
    ui: &dyn InteractiveUi,
    entry: InteractiveEntry,
    help: &[HelpPage],
) -> Result<(), String>
where
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectConfigurationStore + ?Sized,
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
        initializer,
        writer,
    };
    ui.run(entry, &workspace, help)
}

struct InteractiveServices<'a, L: ?Sized, S: ?Sized, G: ?Sized, W: ?Sized> {
    config: Workspace<'a, L, S, G>,
    status: &'a dyn RepositoryStatusReader,
    initializer: &'a dyn RepositoryInitializer,
    writer: &'a W,
}

impl<L, S, G, W> InteractiveWorkspace for InteractiveServices<'_, L, S, G, W>
where
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectConfigurationStore + ?Sized,
    S::Error: Display,
    G: GlobalConfigurationStore + ?Sized,
    G::Error: Display,
    W: CommitWriter + ?Sized,
    W::Error: Display,
{
    fn home(&self) -> HomeState {
        let directory = self.config.start.to_path_buf();
        let root = match self.config.locator.locate(self.config.start) {
            Ok(root) => root,
            Err(error) if L::is_not_repository(&error) => {
                return HomeState::NoRepository { directory };
            }
            Err(error) => {
                return HomeState::Unavailable {
                    directory,
                    error: error.to_string(),
                };
            }
        };
        let stored = match self.config.project.load(&root) {
            Ok(Some(stored)) => stored,
            Ok(None) => return HomeState::Uninitialized { root },
            Err(error) => {
                return HomeState::Unavailable {
                    directory,
                    error: error.to_string(),
                };
            }
        };
        let default = stored.configuration.default_taxonomy().clone();
        let Some(version) = stored
            .configuration
            .find(&default)
            .map(gitserious_core::Taxonomy::version)
        else {
            return HomeState::Unavailable {
                directory,
                error: format!("{POLICY_FILE} names a missing default taxonomy {default}."),
            };
        };
        let policy_untracked = self.status.read(&root).is_ok_and(|status| {
            status
                .entries
                .iter()
                .any(|entry| entry.untracked && entry.path == Path::new(POLICY_FILE))
        });
        HomeState::Initialized {
            root,
            default,
            version,
            policy_untracked,
        }
    }

    fn initialize(&self, configuration: &ProjectConfiguration) -> Result<(), String> {
        let locate = || self.config.locator.locate(self.config.start);
        let root = match locate() {
            Ok(root) => root,
            Err(error) if L::is_not_repository(&error) => {
                self.initializer.initialize(self.config.start)?;
                locate().map_err(|error| error.to_string())?
            }
            Err(error) => return Err(error.to_string()),
        };
        self.config
            .project
            .ensure_local_state(&root)
            .map_err(|error| error.to_string())?;
        self.config
            .project
            .create(&root, configuration)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn library_taxonomies(&self) -> Result<Vec<Taxonomy>, String> {
        let library = self
            .config
            .global
            .load()
            .map_err(|error| error.to_string())?;
        Ok(library
            .custom_taxonomies()
            .iter()
            .filter_map(crate::LibraryTaxonomy::usable)
            .collect())
    }

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
