use std::fmt::{self, Debug, Display, Formatter};
use std::path::Path;
use std::sync::Arc;

use crate::{
    CommitServices, CommitWorkspace, CommitWriter, ConfigurationServices, ConfigurationWorkspace,
    GlobalConfigurationStore, ProjectConfiguration, ProjectConfigurationStore, ProjectOverview,
    RepositoryInitializer, RepositoryLocator, RepositoryStatusReader,
};

/// The project file every gitserious repository commits.
const POLICY_FILE: &str = "gitserious.toml";

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

/// Application services available to an interactive session.
pub trait InteractiveWorkspace {
    fn home(&self) -> ProjectOverview;
    /// Creates the project from `configuration`, and the Git repository first when there is
    /// none; an existing project is never rewritten.
    fn initialize(&self, configuration: &ProjectConfiguration) -> Result<(), String>;
    fn configuration(&self) -> &dyn ConfigurationWorkspace;
    fn commit(&self) -> &dyn CommitWorkspace;
}

/// Terminal adapter for the unified application.
pub trait InteractiveUi {
    fn run(&self, workspace: &dyn InteractiveWorkspace, help: &[HelpPage]) -> Result<(), String>;
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
        config: ConfigurationServices::new(locator, store, global, start),
        status,
        initializer,
        commit: CommitServices::new(locator, store, status, writer, start),
    };
    ui.run(&workspace, help)
}

struct InteractiveServices<'a, L: ?Sized, S: ?Sized, G: ?Sized, W: ?Sized> {
    config: ConfigurationServices<'a, L, S, G>,
    status: &'a dyn RepositoryStatusReader,
    initializer: &'a dyn RepositoryInitializer,
    commit: CommitServices<'a, L, S, W>,
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
    fn home(&self) -> ProjectOverview {
        let directory = self.config.start.to_path_buf();
        let root = match self.config.locator.locate(self.config.start) {
            Ok(root) => root,
            Err(error) if L::is_not_repository(&error) => {
                return ProjectOverview::NoRepository { directory };
            }
            Err(error) => {
                return ProjectOverview::Unavailable {
                    directory,
                    error: error.to_string(),
                };
            }
        };
        let stored = match self.config.project.load(&root) {
            Ok(Some(stored)) => stored,
            Ok(None) => return ProjectOverview::Uninitialized { root },
            Err(error) => {
                return ProjectOverview::Unavailable {
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
            return ProjectOverview::Unavailable {
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
        ProjectOverview::Initialized {
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

    fn configuration(&self) -> &dyn ConfigurationWorkspace {
        &self.config
    }

    fn commit(&self) -> &dyn CommitWorkspace {
        &self.commit
    }
}
