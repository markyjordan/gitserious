mod commands;
mod input;
mod navigation;
mod overlay;
mod render;

use std::io;

use gitserious_app::{CommitWorkspace, ConfigurationWorkspace, ProjectWorkspace};
use ratatui::DefaultTerminal;

use crate::commit::{CommitRequest, CommitView};
use crate::config::ConfigView;
use crate::confirmation::ConfirmDialog;
use crate::help::{HelpPage, HelpView};
use crate::home::{HomePage, HomeRequest};
use navigation::{Section, Tabs};
use overlay::Overlay;

/// The page a unified session shows first.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InitialPage {
    Home,
    Commit,
    Config,
    /// Help, on the page of the named command, or the overview for none.
    Help {
        command: Option<String>,
    },
}

/// The application capabilities the unified TUI uses.
#[derive(Clone, Copy)]
pub struct Capabilities<'a> {
    pub commit: &'a dyn CommitWorkspace,
    pub config: &'a dyn ConfigurationWorkspace,
    pub project: &'a dyn ProjectWorkspace,
}

/// What only the session loop can do after an event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SessionRequest {
    /// Author a commit in the terminal, or resume the failed one's review.
    Author {
        resume: bool,
    },
    Quit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    Select(Section),
    Home(HomeRequest),
    Commit(CommitRequest),
    Initialize,
    Quit,
}

pub(crate) struct Root {
    section: Section,
    config: ConfigView,
    commit: CommitView,
    home: HomePage,
    overlay: Option<Overlay>,
    quit_dialog: ConfirmDialog,
    help: HelpView,
    tabs: Tabs,
    too_small: bool,
}

impl Root {
    pub(crate) fn new(
        entry: InitialPage,
        capabilities: Capabilities<'_>,
        help: &[HelpPage],
    ) -> Self {
        let (section, help) = match entry {
            InitialPage::Home => (Section::Home, HelpView::default()),
            InitialPage::Commit => (Section::Commit, HelpView::default()),
            InitialPage::Config => (Section::Config, HelpView::default()),
            InitialPage::Help { command } => (
                Section::Help,
                HelpView::at(
                    help.iter()
                        .position(|page| page.command == command)
                        .unwrap_or_default(),
                ),
            ),
        };
        let mut root = Self {
            section,
            config: ConfigView::new(capabilities.config),
            commit: CommitView::default(),
            home: HomePage::new(capabilities.project),
            overlay: None,
            quit_dialog: ConfirmDialog::default(),
            help,
            tabs: Tabs::default(),
            too_small: false,
        };
        root.enter(section, capabilities);
        root
    }

    /// Arrives at `section` the same way a tab, a click, or the launch does.
    fn enter(&mut self, section: Section, capabilities: Capabilities<'_>) {
        self.section = section;
        match section {
            Section::Commit => self.commit.refresh(capabilities.commit),
            Section::Home => self.home.enter(capabilities.project),
            Section::Help => self.help.enter(),
            Section::Config => {}
        }
    }

    fn execute(&mut self, action: Action, capabilities: Capabilities<'_>) -> Option<Action> {
        match action {
            Action::Select(section) => self.enter(section, capabilities),
            Action::Home(HomeRequest::GoToCommit) => self.enter(Section::Commit, capabilities),
            Action::Home(HomeRequest::Initialized) => {
                // Config loaded the project before it existed.
                self.config = ConfigView::new(capabilities.config);
            }
            Action::Home(HomeRequest::Initialize) | Action::Initialize => {
                self.section = Section::Home;
                self.home.offer_initialization(capabilities.config);
            }
            Action::Commit(request) => {
                return self
                    .commit
                    .run(request, capabilities.commit)
                    .map(Action::Commit);
            }
            Action::Quit => {
                if self.commit.has_pending_review() {
                    self.overlay = Some(Overlay::QuitConfirm);
                } else {
                    return Some(Action::Quit);
                }
            }
        }
        None
    }

    /// Authors a commit in `terminal`, or resumes the failed one's review.
    pub(crate) fn author(
        &mut self,
        terminal: &mut DefaultTerminal,
        capabilities: Capabilities<'_>,
        resume: bool,
    ) -> io::Result<()> {
        self.commit.author(terminal, capabilities.commit, resume)
    }
}

#[cfg(test)]
mod tests;
