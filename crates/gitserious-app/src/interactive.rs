use std::fmt::{self, Debug, Display, Formatter};
use std::path::Path;
use std::sync::Arc;

use crate::{
    CommitServices, CommitWorkspace, CommitWriter, ConfigurationServices, ConfigurationWorkspace,
    GlobalConfigurationStore, ProjectConfigurationStore, ProjectServices, ProjectWorkspace,
    RepositoryInitializer, RepositoryLocator, RepositoryStatusReader,
};

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
    fn configuration(&self) -> &dyn ConfigurationWorkspace;
    fn commit(&self) -> &dyn CommitWorkspace;
    fn project(&self) -> &dyn ProjectWorkspace;
}

/// Terminal adapter for the unified application.
pub trait InteractiveUi {
    fn run(&self, workspace: &dyn InteractiveWorkspace) -> Result<(), String>;
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
        project: ProjectServices::new(locator, store, status, initializer, start),
        commit: CommitServices::new(locator, store, status, writer, start),
    };
    ui.run(&workspace)
}

struct InteractiveServices<'a, L: ?Sized, S: ?Sized, G: ?Sized, W: ?Sized> {
    config: ConfigurationServices<'a, L, S, G>,
    project: ProjectServices<'a, L, S>,
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
    fn configuration(&self) -> &dyn ConfigurationWorkspace {
        &self.config
    }

    fn commit(&self) -> &dyn CommitWorkspace {
        &self.commit
    }

    fn project(&self) -> &dyn ProjectWorkspace {
        &self.project
    }
}
