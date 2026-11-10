mod commands;
mod navigation;
mod overlay;

use std::io;

use gitserious_app::{CommitWorkspace, ConfigurationWorkspace, ProjectWorkspace};
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Paragraph};
use ratatui::{DefaultTerminal, Frame};

use crate::commit::{CommitRequest, CommitView};
use crate::config::{ConfigRequest, ConfigView, ShellKeys};
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

    /// Handles `event` and returns what only the session loop can do.
    pub(crate) fn session_event(
        &mut self,
        event: Event,
        capabilities: Capabilities<'_>,
        help: &[HelpPage],
    ) -> Option<SessionRequest> {
        match self.handle_event(event, capabilities, help)? {
            Action::Quit => Some(SessionRequest::Quit),
            Action::Commit(CommitRequest::Author) => Some(SessionRequest::Author { resume: false }),
            Action::Commit(CommitRequest::Resume) => Some(SessionRequest::Author { resume: true }),
            Action::Select(_) | Action::Home(_) | Action::Commit(_) | Action::Initialize => None,
        }
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

    fn handle_event(
        &mut self,
        event: Event,
        capabilities: Capabilities<'_>,
        help: &[HelpPage],
    ) -> Option<Action> {
        if matches!(&event, Event::Key(key) if key.kind == KeyEventKind::Release) {
            return None;
        }
        if self.overlay.is_some() {
            return self.handle_overlay(event, capabilities);
        }
        if self.commit.modal() {
            self.commit.modal_event(&event);
            return None;
        }
        if self.too_small && self.section == Section::Config && self.config.modal() {
            return None;
        }
        if self.section == Section::Config && self.config.modal() {
            return self.config_event(event, capabilities);
        }
        if let Event::Mouse(mouse) = &event
            && mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && !self.too_small
        {
            if let Some(section) = self.tabs.at(mouse.column, mouse.row) {
                return self.execute(Action::Select(section), capabilities);
            }
            match self.section {
                Section::Commit => self.commit.click(mouse.column, mouse.row),
                Section::Help => self.help.click(mouse.column, mouse.row),
                Section::Home | Section::Config => {}
            }
        }
        if let Event::Key(key) = &event {
            let control = key.modifiers.contains(KeyModifiers::CONTROL);
            if control && key.code == KeyCode::Char('c') {
                return self.execute(Action::Quit, capabilities);
            }
            if self.too_small {
                return if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                    self.execute(Action::Quit, capabilities)
                } else {
                    None
                };
            }
            if self.section == Section::Config {
                match (self.config.shell_keys(), key.code) {
                    (ShellKeys::Navigation, KeyCode::Tab | KeyCode::BackTab) => {
                        return self.execute(
                            Action::Select(self.section.next(key.code == KeyCode::BackTab)),
                            capabilities,
                        );
                    }
                    (ShellKeys::Navigation, KeyCode::Esc) => {
                        return self.execute(Action::Select(Section::Home), capabilities);
                    }
                    (ShellKeys::Navigation, KeyCode::Char('q')) => {
                        return self.execute(Action::Quit, capabilities);
                    }
                    _ => {}
                }
            } else {
                if self.section == Section::Home && self.home.captures(key) {
                    let request = self.home.key(*key, capabilities.project)?;
                    return self.execute(Action::Home(request), capabilities);
                }
                match key.code {
                    KeyCode::Tab | KeyCode::BackTab => {
                        return self.execute(
                            Action::Select(self.section.next(key.code == KeyCode::BackTab)),
                            capabilities,
                        );
                    }
                    KeyCode::Esc if self.section != Section::Home => {
                        return self.execute(Action::Select(Section::Home), capabilities);
                    }
                    KeyCode::Char('q') | KeyCode::Esc => {
                        return self.execute(Action::Quit, capabilities);
                    }
                    KeyCode::Char('/') => {
                        self.open_palette();
                        return None;
                    }
                    _ => {}
                }
                match self.section {
                    Section::Home => {
                        let request = self.home.key(*key, capabilities.project)?;
                        return self.execute(Action::Home(request), capabilities);
                    }
                    Section::Commit => {
                        let request = self.commit.key(key.code)?;
                        return self.execute(Action::Commit(request), capabilities);
                    }
                    Section::Help => self.help.key(key.code, help.len()),
                    Section::Config => {}
                }
            }
        }
        if self.section == Section::Config {
            return self.config_event(event, capabilities);
        }
        None
    }

    fn config_event(&mut self, event: Event, capabilities: Capabilities<'_>) -> Option<Action> {
        match self.config.handle_event(event, capabilities.config) {
            Some(ConfigRequest::Quit) => return self.execute(Action::Quit, capabilities),
            Some(ConfigRequest::Initialize) => {
                self.home.refresh(capabilities.project);
                if self.home.can_initialize() {
                    return self.execute(Action::Initialize, capabilities);
                }
                self.config = ConfigView::new(capabilities.config);
            }
            None => {}
        }
        None
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
