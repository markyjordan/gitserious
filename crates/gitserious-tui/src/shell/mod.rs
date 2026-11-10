mod commands;
mod input;
mod navigation;
mod overlay;

use std::io;

use gitserious_app::{CommitWorkspace, ConfigurationWorkspace, ProjectWorkspace};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Paragraph};
use ratatui::{DefaultTerminal, Frame};

use crate::commit::{CommitRequest, CommitView};
use crate::config::ConfigView;
use crate::confirmation::ConfirmDialog;
use crate::help::{HelpPage, HelpView};
use crate::home::{HomePage, HomeRequest};
use crate::theme::{
    FOOTER_HEIGHT, MINIMUM_HEIGHT, MINIMUM_WIDTH, SHELL_HINTS, centered_rect, normalize_background,
    render_footer, render_footer_block, theme,
};
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

pub(crate) struct Shell {
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

impl Shell {
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
        let mut shell = Self {
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
        shell.enter(section, capabilities);
        shell
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

    pub(crate) fn render(&mut self, frame: &mut Frame<'_>, help: &[HelpPage]) {
        let area = frame.area();
        self.too_small = area.width < MINIMUM_WIDTH || area.height < MINIMUM_HEIGHT;
        self.tabs = Tabs::default();
        frame.render_widget(
            Block::default().style(Style::default().fg(theme().text).bg(theme().ground)),
            area,
        );
        if self.too_small {
            let message = if self.section == Section::Config && self.config.modal() {
                "Terminal too small\nResize to continue"
            } else {
                "Terminal too small\nResize or press esc/q to quit"
            };
            frame.render_widget(
                Paragraph::new(message).centered(),
                centered_rect(54, 3, area),
            );
        } else {
            let [header, body] =
                Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
            self.render_header(frame, header);
            if self.section == Section::Config {
                self.config.render(frame, body);
                if self.config.modal() {
                    crate::theme::dim_region(frame, header);
                }
            } else {
                let [content, footer] =
                    Layout::vertical([Constraint::Min(1), Constraint::Length(FOOTER_HEIGHT)])
                        .areas(body);
                let footer = render_footer_block(frame, footer);
                match self.section {
                    Section::Home => self.home.render(frame, content),
                    Section::Commit => self.commit.render(frame, content),
                    Section::Help => self.help.render(frame, content, help),
                    Section::Config => {}
                }
                let commit_hints = self.commit.hints();
                let home_hints = self.home.hints();
                let help_hints = self.help.hints(help.len());
                let hints: &[(&str, &str)] = match self.section {
                    Section::Commit => &commit_hints,
                    Section::Help => &help_hints,
                    Section::Home => &home_hints,
                    Section::Config => &[],
                };
                render_footer(frame, footer, hints, &SHELL_HINTS);
            }
        }
        self.render_overlay(frame, area);
        normalize_background(frame, area);
    }

    fn render_header(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let band = Style::new().fg(theme().dim).bg(theme().band);
        frame.render_widget(Block::new().style(band), area);
        let version = format!("gitserious v{} ", env!("CARGO_PKG_VERSION"));
        frame.render_widget(Paragraph::new(version).style(band).right_aligned(), area);
        self.tabs.render(frame, area, self.section);
    }
}

#[cfg(test)]
mod tests;
