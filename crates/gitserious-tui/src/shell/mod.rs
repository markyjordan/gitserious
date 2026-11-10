mod navigation;

use std::io;

use gitserious_app::{CommitWorkspace, ConfigurationWorkspace, ProjectWorkspace};
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph};
use ratatui::{DefaultTerminal, Frame};

use crate::commit::CommitView;
use crate::config::{ConfigRequest, ConfigView, ShellKeys};
use crate::confirmation::{self, Answer, ConfirmDialog, Confirmation, DISCARD_AND_QUIT};
use crate::help::{HelpPage, HelpView};
use crate::home::{HomeAction, HomeOutcome, HomePage};
use crate::theme::{
    FOOTER_HEIGHT, JET_BLACK, MINIMUM_HEIGHT, MINIMUM_WIDTH, SHELL_HINTS, block_title,
    centered_rect, command_hints, frame_style, navigation_key_style, normalize_background,
    render_footer, render_footer_block, theme,
};
use navigation::{Section, Tabs};

fn quit_question() -> Confirmation<'static> {
    Confirmation {
        title: "Quit",
        message: "Discard unapplied changes and quit?".into(),
        choices: DISCARD_AND_QUIT,
    }
}

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
    Refresh,
    Result,
    Author,
    Resume,
    Initialize,
    Quit,
}

struct Palette {
    query: String,
    selected: usize,
    list: ListState,
}
enum Overlay {
    Palette(Palette),
    QuitConfirm,
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
            Action::Initialize => {
                self.section = Section::Home;
                self.home.offer_initialization(capabilities.config);
            }
            Action::Refresh => {
                self.commit.notice.clear();
                self.commit.refresh(capabilities.commit);
            }
            Action::Result => self.commit.show_result(),
            Action::Author | Action::Resume => return Some(action),
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
            Action::Author => Some(SessionRequest::Author { resume: false }),
            Action::Resume => Some(SessionRequest::Author { resume: true }),
            Action::Select(_) | Action::Refresh | Action::Result | Action::Initialize => None,
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
                if self.section == Section::Home
                    && self.home.editing()
                    && !matches!(
                        key.code,
                        KeyCode::Tab | KeyCode::BackTab | KeyCode::Char('q' | '/')
                    )
                {
                    if self.home.key(*key, capabilities.project) == HomeOutcome::Initialized {
                        // Config loaded the project before it existed.
                        self.config = ConfigView::new(capabilities.config);
                    }
                    return None;
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
                    Section::Home if key.code == KeyCode::Enter => match self.home.action() {
                        Some(HomeAction::GoToCommit) => {
                            return self.execute(Action::Select(Section::Commit), capabilities);
                        }
                        Some(_) => return self.execute(Action::Initialize, capabilities),
                        None => {}
                    },
                    Section::Commit => match key.code {
                        KeyCode::Char('r') => return self.execute(Action::Refresh, capabilities),
                        KeyCode::Char('o') if !self.commit.notice.is_empty() => {
                            return self.execute(Action::Result, capabilities);
                        }
                        KeyCode::Enter => {
                            return self.execute(
                                if self.commit.has_pending_review() {
                                    Action::Resume
                                } else {
                                    Action::Author
                                },
                                capabilities,
                            );
                        }
                        KeyCode::Char('c') => return self.execute(Action::Author, capabilities),
                        KeyCode::Up | KeyCode::Down => {
                            self.commit.move_selection(key.code == KeyCode::Down)
                        }
                        KeyCode::Left | KeyCode::Right => {
                            self.commit.directory(key.code == KeyCode::Right)
                        }
                        _ => {}
                    },
                    Section::Help => self.help.key(key.code, help.len()),
                    _ => {}
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
                if self.home.action().is_some_and(HomeAction::initializes) {
                    return self.execute(Action::Initialize, capabilities);
                }
                self.config = ConfigView::new(capabilities.config);
            }
            None => {}
        }
        None
    }

    fn open_palette(&mut self) {
        self.overlay = Some(Overlay::Palette(Palette {
            query: String::new(),
            selected: 0,
            list: ListState::default(),
        }));
    }

    fn commands(&self) -> Vec<(String, Action)> {
        let mut commands: Vec<_> = Section::ALL
            .into_iter()
            .map(|section| (format!("Open {}", section.label()), Action::Select(section)))
            .collect();
        if self.section == Section::Home && self.home.action().is_some_and(HomeAction::initializes)
        {
            commands.push(("Initialize gitserious".into(), Action::Initialize));
        }
        if self.section == Section::Commit {
            commands.push(("Refresh repository".into(), Action::Refresh));
            if !self.commit.notice.is_empty() {
                commands.push(("View last Git result".into(), Action::Result));
            }
            if self.commit.blocker.is_none() {
                commands.push(("Start commit".into(), Action::Author));
            }
            if self.commit.has_pending_review() {
                commands.push(("Resume review".into(), Action::Resume));
            }
        }
        commands.push(("Quit".into(), Action::Quit));
        commands
    }

    fn handle_overlay(&mut self, event: Event, capabilities: Capabilities<'_>) -> Option<Action> {
        let overlay = self.overlay.take()?;
        match overlay {
            Overlay::QuitConfirm => match self.quit_dialog.handle_event(DISCARD_AND_QUIT, &event) {
                Some(Answer::Confirm) => return Some(Action::Quit),
                Some(Answer::Keep | Answer::Alternate) => self.quit_dialog.invalidate(),
                None => self.overlay = Some(Overlay::QuitConfirm),
            },
            Overlay::Palette(mut palette) => {
                if let Event::Key(key) = event {
                    let commands: Vec<_> = self
                        .commands()
                        .into_iter()
                        .filter(|(label, _)| {
                            label.to_lowercase().contains(&palette.query.to_lowercase())
                        })
                        .collect();
                    match key.code {
                        KeyCode::Esc => return None,
                        KeyCode::Up => palette.selected = palette.selected.saturating_sub(1),
                        KeyCode::Down => {
                            palette.selected =
                                (palette.selected + 1).min(commands.len().saturating_sub(1))
                        }
                        KeyCode::Enter => {
                            if let Some((_, action)) = commands.get(palette.selected) {
                                return self.execute(*action, capabilities);
                            }
                        }
                        KeyCode::Backspace => {
                            palette.query.pop();
                            palette.selected = 0;
                        }
                        KeyCode::Char(character)
                            if !key.modifiers.contains(KeyModifiers::CONTROL) =>
                        {
                            palette.query.push(character);
                            palette.selected = 0;
                        }
                        _ => {}
                    }
                } else if let Event::Paste(text) = event {
                    palette.query.push_str(&text);
                    palette.selected = 0;
                }
                self.overlay = Some(Overlay::Palette(palette));
            }
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
                let home_hints = self.home.hints();
                let help_hints = self.help.hints(help.len());
                let hints: &[(&str, &str)] = match self.section {
                    Section::Commit => &[("↑/↓", "move"), ("←/→", "tree"), ("r", "refresh")],
                    Section::Help => &help_hints,
                    Section::Home => &home_hints,
                    Section::Config => &[],
                };
                render_footer(frame, footer, hints, &SHELL_HINTS);
            }
        }
        self.render_overlay(frame, area);
        if matches!(self.overlay, Some(Overlay::QuitConfirm)) {
            let footer = Rect::new(
                area.x,
                area.bottom().saturating_sub(FOOTER_HEIGHT),
                area.width,
                FOOTER_HEIGHT,
            );
            let footer = render_footer_block(frame, footer);
            render_footer(frame, footer, &confirmation::hints(DISCARD_AND_QUIT), &[]);
        }
        normalize_background(frame, area);
    }

    fn render_header(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let band = Style::new().fg(theme().dim).bg(theme().band);
        frame.render_widget(Block::new().style(band), area);
        let version = format!("gitserious v{} ", env!("CARGO_PKG_VERSION"));
        frame.render_widget(Paragraph::new(version).style(band).right_aligned(), area);
        self.tabs.render(frame, area, self.section);
    }

    fn render_overlay(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.quit_dialog.invalidate();
        if matches!(self.overlay, Some(Overlay::QuitConfirm)) {
            self.quit_dialog.render(frame, &quit_question());
            return;
        }
        let commands = self.commands();
        if self.overlay.is_none() && !self.commit.modal() {
            return;
        }
        let popup = centered_rect(54, 13.min(area.height), area);
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Block::bordered()
                .border_style(frame_style())
                .style(Style::default().fg(Color::White).bg(JET_BLACK)),
            popup,
        );
        let inner = Block::bordered().inner(popup);
        match &mut self.overlay {
            None => self.commit.render_modal(frame, inner),
            Some(Overlay::QuitConfirm) => {}
            Some(Overlay::Palette(palette)) => {
                let [search, list, footer] = Layout::vertical([
                    Constraint::Length(3),
                    Constraint::Min(1),
                    Constraint::Length(1),
                ])
                .areas(inner);
                frame.render_widget(
                    Paragraph::new(format!("/ {}", palette.query))
                        .block(Block::bordered().title(block_title("Search"))),
                    search,
                );
                let rows: Vec<_> = commands
                    .into_iter()
                    .filter(|(label, _)| {
                        label.to_lowercase().contains(&palette.query.to_lowercase())
                    })
                    .map(|(label, _)| ListItem::new(label))
                    .collect();
                palette.selected = palette.selected.min(rows.len().saturating_sub(1));
                palette
                    .list
                    .select((!rows.is_empty()).then_some(palette.selected));
                frame.render_stateful_widget(
                    List::new(rows).highlight_style(navigation_key_style()),
                    list,
                    &mut palette.list,
                );
                frame.render_widget(
                    Paragraph::new(command_hints(&[
                        ("↑/↓", "select"),
                        ("enter", "run"),
                        ("esc", "close"),
                    ])),
                    footer,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
