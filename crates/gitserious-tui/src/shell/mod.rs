mod commit_view;
mod home_logo;

use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use gitserious_app::{
    CommitAuthoringContext, HelpPage, InteractiveEntry, InteractiveUi, InteractiveWorkspace,
};
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use crate::author::{AuthoringCheckpoint, embedded_author};
use crate::confirmation::{self, Answer, ConfirmDialog, Confirmation, DISCARD_AND_QUIT};
use crate::taxonomy_config::ConfigView;
use crate::theme::{
    JET_BLACK, MINIMUM_HEIGHT, MINIMUM_WIDTH, centered_rect, command_hints, frame_style,
    navigation_key_style, normalize_background, render_navigation_row,
};
use commit_view::CommitView;
use home_logo::{HomeLogo, render_home_logo, resolve_dev_theme};

fn quit_question() -> Confirmation<'static> {
    Confirmation {
        title: "Quit",
        message: "Discard unapplied changes and quit?".into(),
        choices: DISCARD_AND_QUIT,
    }
}

/// Unified Home, Commit, Config, and Help terminal adapter.
#[derive(Clone, Debug, Default)]
pub struct RatatuiInteractiveUi {
    library_path: Option<PathBuf>,
}

impl RatatuiInteractiveUi {
    #[must_use]
    pub fn with_library_path(path: PathBuf) -> Self {
        Self {
            library_path: Some(path),
        }
    }
}

impl InteractiveUi for RatatuiInteractiveUi {
    fn run(
        &self,
        entry: InteractiveEntry,
        workspace: &dyn InteractiveWorkspace,
        help: &[HelpPage],
    ) -> Result<(), String> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(if entry == InteractiveEntry::Config {
                "configuration editing requires an interactive terminal"
            } else {
                "interactive shell requires an interactive terminal"
            }
            .into());
        }
        crate::runtime::run(|terminal| {
            let mut shell = Shell::new(entry, workspace, self.library_path.clone());
            let mut previous = Instant::now();
            loop {
                let now = Instant::now();
                shell.advance(now.duration_since(previous));
                previous = now;
                terminal.draw(|frame| shell.render(frame, help))?;
                if shell.animating() && !event::poll(Duration::from_millis(16))? {
                    continue;
                }
                let event = event::read()?;
                // Idle time is not animation time for transitions triggered by this event.
                previous = Instant::now();
                if let Some(action) = shell.handle_event(event, workspace, help) {
                    if matches!(action, Action::Quit) {
                        return Ok(());
                    }
                    if matches!(action, Action::Author | Action::Resume) {
                        shell.author(terminal, workspace, matches!(action, Action::Resume))?;
                        previous = Instant::now();
                    }
                }
            }
        })
        .map_err(|error| format!("interactive shell failed: {error}"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Section {
    Home,
    Commit,
    Config,
    Help,
}
impl Section {
    const ALL: [Self; 4] = [Self::Home, Self::Commit, Self::Config, Self::Help];
    fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Config => "Config",
            Self::Commit => "Commit",
            Self::Help => "Help",
        }
    }
    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|section| *section == self)
            .unwrap_or(0)
    }
    fn next(self, reverse: bool) -> Self {
        let count = Self::ALL.len();
        Self::ALL[(self.index() + if reverse { count - 1 } else { 1 }) % count]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    Select(Section),
    Refresh,
    Result,
    Author,
    Resume,
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
    Result(u16),
}
struct PendingAuthor {
    context: CommitAuthoringContext,
    checkpoint: AuthoringCheckpoint,
}

struct Shell {
    section: Section,
    config: ConfigView,
    commit: CommitView,
    home_logo: HomeLogo,
    pending_author: Option<PendingAuthor>,
    pending_navigation: Option<Action>,
    overlay: Option<Overlay>,
    quit_dialog: ConfirmDialog,
    help_page: usize,
    help_scroll: u16,
    root_areas: [Rect; Section::ALL.len()],
    too_small: bool,
}

impl Shell {
    fn new(
        entry: InteractiveEntry,
        workspace: &dyn InteractiveWorkspace,
        library_path: Option<PathBuf>,
    ) -> Self {
        Self {
            section: if entry == InteractiveEntry::Config {
                Section::Config
            } else {
                Section::Home
            },
            config: ConfigView::new(workspace.configuration(), library_path),
            commit: CommitView::default(),
            home_logo: HomeLogo::with_theme(resolve_dev_theme()),
            pending_author: None,
            pending_navigation: None,
            overlay: None,
            quit_dialog: ConfirmDialog::default(),
            help_page: 0,
            help_scroll: 0,
            root_areas: [Rect::default(); Section::ALL.len()],
            too_small: false,
        }
    }

    fn advance(&mut self, elapsed: Duration) {
        self.home_logo.advance(elapsed);
    }

    fn animating(&self) -> bool {
        self.section == Section::Home
    }

    fn request(&mut self, action: Action, workspace: &dyn InteractiveWorkspace) -> Option<Action> {
        if self.section == Section::Config
            && self.config.editing_taxonomy()
            && !self.config.request_leave_editor()
        {
            self.pending_navigation = Some(action);
            return None;
        }
        self.execute(action, workspace)
    }

    fn execute(&mut self, action: Action, workspace: &dyn InteractiveWorkspace) -> Option<Action> {
        match action {
            Action::Select(section) => {
                self.section = section;
                if section == Section::Commit {
                    self.commit.refresh(workspace);
                }
            }
            Action::Refresh => {
                self.commit.notice.clear();
                self.commit.refresh(workspace);
            }
            Action::Result => self.overlay = Some(Overlay::Result(0)),
            Action::Author | Action::Resume => return Some(action),
            Action::Quit => {
                if self.config.dirty() || self.pending_author.is_some() {
                    self.overlay = Some(Overlay::QuitConfirm);
                } else {
                    return Some(Action::Quit);
                }
            }
        }
        None
    }

    fn handle_event(
        &mut self,
        event: Event,
        workspace: &dyn InteractiveWorkspace,
        help: &[HelpPage],
    ) -> Option<Action> {
        if matches!(&event, Event::Key(key) if key.kind == KeyEventKind::Release) {
            return None;
        }
        if self.overlay.is_some() {
            return self.handle_overlay(event, workspace);
        }
        if self.section == Section::Config && self.config.modal() {
            let was_confirming = self.config.confirming_discard();
            let quit = self.config.handle_event(event, workspace.configuration());
            if quit {
                return self.execute(Action::Quit, workspace);
            }
            if was_confirming && !self.config.confirming_discard() {
                let pending = self.pending_navigation.take();
                if !self.config.editing_taxonomy()
                    && let Some(action) = pending
                {
                    return self.execute(action, workspace);
                }
            }
            return None;
        }
        if let Event::Mouse(mouse) = &event
            && mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && !self.too_small
        {
            if let Some(section) = Section::ALL.into_iter().find(|section| {
                self.root_areas[section.index()].contains((mouse.column, mouse.row).into())
            }) {
                return self.request(Action::Select(section), workspace);
            }
            if self.section == Section::Commit {
                self.commit.click(mouse.column, mouse.row);
            }
        }
        if let Event::Key(key) = &event {
            let control = key.modifiers.contains(KeyModifiers::CONTROL);
            if control && key.code == KeyCode::Char('c') {
                return self.request(Action::Quit, workspace);
            }
            if self.too_small {
                return if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                    self.request(Action::Quit, workspace)
                } else {
                    None
                };
            }
            if self.section == Section::Config {
                if self.config.overview() {
                    match key.code {
                        KeyCode::Tab | KeyCode::BackTab => {
                            return self.execute(
                                Action::Select(self.section.next(key.code == KeyCode::BackTab)),
                                workspace,
                            );
                        }
                        KeyCode::Esc => {
                            return self.execute(Action::Select(Section::Home), workspace);
                        }
                        KeyCode::Char('q') => return self.execute(Action::Quit, workspace),
                        _ => {}
                    }
                }
            } else {
                match key.code {
                    KeyCode::Tab | KeyCode::BackTab => {
                        return self.execute(
                            Action::Select(self.section.next(key.code == KeyCode::BackTab)),
                            workspace,
                        );
                    }
                    KeyCode::Esc if self.section != Section::Home => {
                        return self.execute(Action::Select(Section::Home), workspace);
                    }
                    KeyCode::Char('q') | KeyCode::Esc => {
                        return self.execute(Action::Quit, workspace);
                    }
                    KeyCode::Char('/') => {
                        self.overlay = Some(Overlay::Palette(Palette {
                            query: String::new(),
                            selected: 0,
                            list: ListState::default(),
                        }));
                        return None;
                    }
                    KeyCode::Char('?') => {
                        return self.execute(Action::Select(Section::Help), workspace);
                    }
                    _ => {}
                }
                match self.section {
                    Section::Commit => match key.code {
                        KeyCode::Char('r') => return self.execute(Action::Refresh, workspace),
                        KeyCode::Char('o') if !self.commit.notice.is_empty() => {
                            return self.execute(Action::Result, workspace);
                        }
                        KeyCode::Enter => {
                            return self.execute(
                                if self.pending_author.is_some() {
                                    Action::Resume
                                } else {
                                    Action::Author
                                },
                                workspace,
                            );
                        }
                        KeyCode::Char('c') => return self.execute(Action::Author, workspace),
                        KeyCode::Up | KeyCode::Down => {
                            self.commit.move_selection(key.code == KeyCode::Down)
                        }
                        KeyCode::Left | KeyCode::Right => {
                            self.commit.directory(key.code == KeyCode::Right)
                        }
                        _ => {}
                    },
                    Section::Help => match key.code {
                        KeyCode::Left | KeyCode::Right if !help.is_empty() => {
                            self.help_page = (self.help_page
                                + if key.code == KeyCode::Right {
                                    1
                                } else {
                                    help.len() - 1
                                })
                                % help.len();
                            self.help_scroll = 0;
                        }
                        KeyCode::Up => self.help_scroll = self.help_scroll.saturating_sub(1),
                        KeyCode::Down => self.help_scroll = self.help_scroll.saturating_add(1),
                        KeyCode::PageUp => self.help_scroll = self.help_scroll.saturating_sub(10),
                        KeyCode::PageDown => self.help_scroll = self.help_scroll.saturating_add(10),
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
        if self.section == Section::Config
            && self.config.handle_event(event, workspace.configuration())
        {
            return self.execute(Action::Quit, workspace);
        }
        None
    }

    fn commands(&self) -> Vec<(String, Action)> {
        let mut commands: Vec<_> = Section::ALL
            .into_iter()
            .map(|section| (format!("Open {}", section.label()), Action::Select(section)))
            .collect();
        if self.section == Section::Commit {
            commands.push(("Refresh repository".into(), Action::Refresh));
            if !self.commit.notice.is_empty() {
                commands.push(("View last Git result".into(), Action::Result));
            }
            if self.commit.blocker.is_none() {
                commands.push(("Start commit".into(), Action::Author));
            }
            if self.pending_author.is_some() {
                commands.push(("Resume review".into(), Action::Resume));
            }
        }
        commands.push(("Quit".into(), Action::Quit));
        commands
    }

    fn handle_overlay(
        &mut self,
        event: Event,
        workspace: &dyn InteractiveWorkspace,
    ) -> Option<Action> {
        let overlay = self.overlay.take()?;
        match overlay {
            Overlay::Result(mut scroll) => {
                if let Event::Key(key) = event {
                    match key.code {
                        KeyCode::Esc | KeyCode::Enter => return None,
                        KeyCode::Up => scroll = scroll.saturating_sub(1),
                        KeyCode::Down => scroll = scroll.saturating_add(1),
                        KeyCode::PageUp => scroll = scroll.saturating_sub(10),
                        KeyCode::PageDown => scroll = scroll.saturating_add(10),
                        _ => {}
                    }
                }
                self.overlay = Some(Overlay::Result(scroll));
            }
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
                                return self.request(*action, workspace);
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

    fn author(
        &mut self,
        terminal: &mut DefaultTerminal,
        workspace: &dyn InteractiveWorkspace,
        resume: bool,
    ) -> io::Result<()> {
        self.commit.refresh(workspace);
        let pending = if resume {
            self.pending_author.take()
        } else {
            None
        };
        let (context, checkpoint) = if let Some(pending) = pending {
            (pending.context, Some(pending.checkpoint))
        } else {
            match workspace.prepare_commit() {
                Ok(context) => (context, None),
                Err(error) => {
                    self.commit.notice = error;
                    return Ok(());
                }
            }
        };
        let Some((authored, checkpoint)) = embedded_author(terminal, &context, checkpoint)? else {
            return Ok(());
        };
        let result = crate::runtime::suspended(terminal, || workspace.submit_commit(&authored))?;
        match result {
            Ok(output) => {
                self.pending_author = None;
                format!(
                    "Commit created.\n{}{}",
                    String::from_utf8_lossy(output.stdout()),
                    String::from_utf8_lossy(output.stderr())
                )
                .trim()
                .clone_into(&mut self.commit.notice);
            }
            Err(error) => {
                self.commit.notice =
                    format!("Commit failed: {error}\nThe reviewed message is retained.");
                self.pending_author = Some(PendingAuthor {
                    context,
                    checkpoint,
                });
            }
        }
        self.commit.refresh(workspace);
        Ok(())
    }

    fn render(&mut self, frame: &mut Frame<'_>, help: &[HelpPage]) {
        let area = frame.area();
        self.too_small = area.width < MINIMUM_WIDTH || area.height < MINIMUM_HEIGHT;
        self.root_areas = [Rect::default(); Section::ALL.len()];
        frame.render_widget(
            Block::default().style(Style::default().fg(Color::White).bg(JET_BLACK)),
            area,
        );
        if self.too_small {
            frame.render_widget(
                Paragraph::new("Terminal too small\nResize or press esc/q to quit").centered(),
                centered_rect(54, 3, area),
            );
        } else {
            let [header, body] =
                Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(area);
            self.render_header(frame, header);
            if self.section == Section::Config {
                self.config.render(frame, body);
            } else {
                let [content, footer] =
                    Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(body);
                match self.section {
                    Section::Home => {
                        let block = Block::bordered().border_style(frame_style());
                        let inner = block.inner(content);
                        frame.render_widget(block, content);
                        let version = format!("gitserious v{}", env!("CARGO_PKG_VERSION"));
                        render_home_logo(frame, inner, &self.home_logo, &version);
                    }
                    Section::Commit => {
                        self.commit
                            .render(frame, content, self.pending_author.is_some())
                    }
                    Section::Help => self.render_help(frame, content, help),
                    Section::Config => {}
                }
                let hints: &[(&str, &str)] = match self.section {
                    Section::Commit => &[
                        ("tab/shift+tab", "switch"),
                        ("↑/↓", "move"),
                        ("←/→", "tree"),
                        ("r", "refresh"),
                        ("/", "commands"),
                    ],
                    Section::Help => &[
                        ("tab/shift+tab", "switch"),
                        ("←/→", "page"),
                        ("↑/↓", "scroll"),
                        ("/", "commands"),
                        ("q", "quit"),
                    ],
                    _ => &[
                        ("tab/shift+tab", "switch"),
                        ("/", "commands"),
                        ("?", "help"),
                        ("q", "quit"),
                    ],
                };
                render_navigation_row(frame, footer, hints);
            }
        }
        if self.too_small && self.section == Section::Config && self.config.confirming_discard() {
            self.config.render(frame, area);
        }
        self.render_overlay(frame, area);
        if matches!(self.overlay, Some(Overlay::QuitConfirm)) {
            let footer = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
            frame.render_widget(Clear, footer);
            render_navigation_row(frame, footer, &confirmation::hints(DISCARD_AND_QUIT));
        }
        normalize_background(frame, area);
    }

    fn render_header(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let block = Block::bordered().border_style(frame_style());
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let width = Section::ALL
            .into_iter()
            .map(|section| {
                u16::try_from(Line::from(section.label()).width())
                    .unwrap_or(0)
                    .saturating_add(2)
            })
            .sum::<u16>()
            .saturating_add(u16::try_from(Section::ALL.len() - 1).unwrap_or(0));
        let mut x = inner
            .x
            .saturating_add(inner.width.saturating_sub(width) / 2);
        for section in Section::ALL {
            let text = format!(" {} ", section.label());
            let width = u16::try_from(Line::from(text.as_str()).width()).unwrap_or(0);
            let rect = Rect::new(x, inner.y, width.min(inner.right().saturating_sub(x)), 1);
            let style = if section == self.section {
                navigation_key_style()
            } else {
                Style::default()
            };
            frame.render_widget(Paragraph::new(text).style(style), rect);
            self.root_areas[section.index()] = rect;
            x = x.saturating_add(width + 1);
        }
    }

    fn render_help(&mut self, frame: &mut Frame<'_>, area: Rect, help: &[HelpPage]) {
        if let Some(page) = help.get(self.help_page) {
            let block = Block::bordered().border_style(frame_style()).title(format!(
                "Help — {} ({}/{})",
                page.title,
                self.help_page + 1,
                help.len()
            ));
            let inner = block.inner(area);
            let paragraph = Paragraph::new(page.text.as_str()).wrap(Wrap { trim: false });
            let maximum = paragraph
                .line_count(inner.width)
                .saturating_sub(usize::from(inner.height));
            self.help_scroll = self
                .help_scroll
                .min(u16::try_from(maximum).unwrap_or(u16::MAX));
            frame.render_widget(paragraph.scroll((self.help_scroll, 0)).block(block), area);
        }
    }

    fn render_overlay(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.quit_dialog.invalidate();
        if matches!(self.overlay, Some(Overlay::QuitConfirm)) {
            self.quit_dialog.render(frame, area, &quit_question());
            return;
        }
        let commands = self.commands();
        let Some(overlay) = &mut self.overlay else {
            return;
        };
        let popup = centered_rect(54, 13.min(area.height), area);
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Block::bordered()
                .border_style(frame_style())
                .style(Style::default().fg(Color::White).bg(JET_BLACK)),
            popup,
        );
        let inner = Block::bordered().inner(popup);
        match overlay {
            Overlay::Result(scroll) => {
                let [text, footer] =
                    Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
                let paragraph =
                    Paragraph::new(self.commit.notice.as_str()).wrap(Wrap { trim: false });
                let maximum = paragraph
                    .line_count(text.width)
                    .saturating_sub(usize::from(text.height));
                *scroll = (*scroll).min(u16::try_from(maximum).unwrap_or(u16::MAX));
                frame.render_widget(paragraph.scroll((*scroll, 0)), text);
                frame.render_widget(
                    Paragraph::new(command_hints(&[
                        ("↑/↓", "scroll"),
                        ("pgup/pgdn", "page"),
                        ("esc", "close"),
                    ])),
                    footer,
                );
            }
            Overlay::QuitConfirm => {}
            Overlay::Palette(palette) => {
                let [search, list, footer] = Layout::vertical([
                    Constraint::Length(3),
                    Constraint::Min(1),
                    Constraint::Length(1),
                ])
                .areas(inner);
                frame.render_widget(
                    Paragraph::new(format!("/ {}", palette.query))
                        .block(Block::bordered().title("Search")),
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
mod tests {
    use ratatui::crossterm::event::KeyEvent;

    use super::*;
    use gitserious_app::{
        BranchState, CommitOutput, ConfigurationDestination, ConfigurationSession,
        ConfigurationWorkspace, GlobalConfiguration, ProjectState, RepositoryRoot,
        RepositoryStatus,
    };
    use ratatui::{Terminal, backend::TestBackend};

    struct Services;
    impl ConfigurationWorkspace for Services {
        fn load(
            &self,
            destination: ConfigurationDestination,
        ) -> Result<ConfigurationSession, String> {
            let global = GlobalConfiguration::default();
            match destination {
                ConfigurationDestination::Global => Ok(ConfigurationSession::open_global(global)),
                ConfigurationDestination::Project => ConfigurationSession::open_project(
                    RepositoryRoot::new(std::env::temp_dir()).map_err(|error| error.to_string())?,
                    global,
                    ProjectState::Absent,
                ),
            }
        }
        fn save(&self, _: &ConfigurationSession) -> Result<ConfigurationSession, String> {
            Err("test save unavailable".into())
        }
    }
    impl InteractiveWorkspace for Services {
        fn configuration(&self) -> &dyn ConfigurationWorkspace {
            self
        }
        fn repository_status(&self) -> Result<RepositoryStatus, String> {
            Ok(RepositoryStatus {
                root: RepositoryRoot::new(std::env::temp_dir())
                    .map_err(|error| error.to_string())?,
                branch: BranchState::Unborn("main".into()),
                entries: Vec::new(),
            })
        }
        fn prepare_commit(&self) -> Result<CommitAuthoringContext, String> {
            Err("Stage changes with Git".into())
        }
        fn submit_commit(
            &self,
            _: &gitserious_app::AuthoredCommit,
        ) -> Result<CommitOutput, String> {
            Err("test submit unavailable".into())
        }
    }

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }
    fn rendered(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .chunks(usize::from(terminal.size().map_or(60, |size| size.width)))
            .map(|row| {
                row.iter()
                    .map(ratatui::buffer::Cell::symbol)
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn click(area: Rect) -> Event {
        Event::Mouse(event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.x + 1,
            row: area.y,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn config_overview_tab_switches_sections_and_keeps_menu_selection()
    -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Home, &services, None);
        shell.handle_event(key(KeyCode::Tab), &services, &[]);
        assert_eq!(shell.section, Section::Commit);
        assert!(shell.commit.blocker.is_some());
        shell.handle_event(key(KeyCode::Tab), &services, &[]);
        assert_eq!(shell.section, Section::Config);
        assert!(!shell.animating());
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        let text = rendered(&terminal);
        assert!(text.contains("Menu"));
        assert!(text.contains("tab/shift+tab: switch"));
        shell.handle_event(key(KeyCode::Down), &services, &[]);
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert!(rendered(&terminal).contains("Taxonomies"));
        shell.handle_event(key(KeyCode::BackTab), &services, &[]);
        assert_eq!(shell.section, Section::Commit);
        shell.handle_event(key(KeyCode::Tab), &services, &[]);
        shell.handle_event(key(KeyCode::Tab), &services, &[]);
        assert_eq!(shell.section, Section::Help);
        shell.handle_event(key(KeyCode::BackTab), &services, &[]);
        assert_eq!(shell.section, Section::Config);
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert!(rendered(&terminal).contains("Taxonomies"));
        shell.handle_event(key(KeyCode::Esc), &services, &[]);
        assert_eq!(shell.section, Section::Home);
        Ok(())
    }

    #[test]
    fn taxonomy_editor_guards_root_navigation_with_the_config_discard_dialog()
    -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Config, &services, None);
        shell.handle_event(key(KeyCode::Down), &services, &[]);
        shell.handle_event(key(KeyCode::Char('n')), &services, &[]);
        shell.handle_event(Event::Paste("team".into()), &services, &[]);
        assert!(shell.config.editing_taxonomy());
        shell.handle_event(key(KeyCode::Tab), &services, &[]);
        assert_eq!(shell.section, Section::Config);
        assert!(!shell.config.confirming_discard());
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert!(rendered(&terminal).contains("Library / create taxonomy"));
        shell.handle_event(
            click(shell.root_areas[Section::Commit.index()]),
            &services,
            &[],
        );
        assert!(shell.config.confirming_discard());
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert!(rendered(&terminal).contains("Discard new taxonomy"));
        shell.handle_event(key(KeyCode::Esc), &services, &[]);
        assert!(shell.config.editing_taxonomy());
        assert!(shell.pending_navigation.is_none());
        assert_eq!(shell.section, Section::Config);
        shell.handle_event(
            Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            &services,
            &[],
        );
        assert!(shell.config.confirming_discard());
        shell.handle_event(key(KeyCode::Char('n')), &services, &[]);
        shell.handle_event(
            click(shell.root_areas[Section::Commit.index()]),
            &services,
            &[],
        );
        shell.handle_event(key(KeyCode::Char('y')), &services, &[]);
        assert!(!shell.config.editing_taxonomy());
        assert_eq!(shell.section, Section::Commit);
        Ok(())
    }

    #[test]
    fn unchanged_taxonomy_editor_leaves_without_confirmation() {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Config, &services, None);
        shell.handle_event(key(KeyCode::Down), &services, &[]);
        shell.handle_event(key(KeyCode::Char('n')), &services, &[]);
        assert!(shell.config.editing_taxonomy());
        shell.execute(Action::Select(Section::Help), &services);
        assert_eq!(shell.section, Section::Help);
        shell.request(Action::Select(Section::Help), &services);
        shell.execute(Action::Select(Section::Config), &services);
        assert!(shell.config.editing_taxonomy());
        shell.request(Action::Select(Section::Commit), &services);
        assert_eq!(shell.section, Section::Commit);
        assert!(!shell.config.editing_taxonomy());
    }

    #[test]
    fn root_wrap_palette_is_modal_and_help_scrolls_with_resize()
    -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Home, &services, None);
        let help = vec![
            HelpPage {
                title: "Overview".into(),
                text: "line\n".repeat(100),
            },
            HelpPage {
                title: "Config".into(),
                text: "config help".into(),
            },
        ];
        shell.handle_event(key(KeyCode::BackTab), &services, &help);
        assert_eq!(shell.section, Section::Help);
        shell.handle_event(key(KeyCode::PageDown), &services, &help);
        assert_eq!(shell.help_scroll, 10);
        shell.handle_event(key(KeyCode::Right), &services, &help);
        assert_eq!(shell.help_page, 1);
        assert_eq!(shell.help_scroll, 0);
        shell.handle_event(key(KeyCode::Tab), &services, &help);
        assert_eq!(shell.section, Section::Home);
        shell.handle_event(key(KeyCode::Char('/')), &services, &help);
        shell.handle_event(Event::Paste("open commit".into()), &services, &help);
        shell.handle_event(key(KeyCode::Tab), &services, &help);
        assert_eq!(shell.section, Section::Home);
        shell.handle_event(key(KeyCode::Enter), &services, &help);
        assert_eq!(shell.section, Section::Commit);
        let mut terminal = Terminal::new(TestBackend::new(66, 18))?;
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert!(rendered(&terminal).contains("Staged (0)"));
        terminal.backend_mut().resize(40, 10);
        terminal.resize(Rect::new(0, 0, 40, 10))?;
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert!(rendered(&terminal).contains("Terminal too small"));
        Ok(())
    }

    #[test]
    fn config_drafts_survive_section_visits_and_quit_requires_confirmation() {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Config, &services, None);
        shell.handle_event(key(KeyCode::Char('i')), &services, &[]);
        assert!(shell.config.dirty());
        shell.handle_event(key(KeyCode::Esc), &services, &[]);
        assert!(shell.config.overview());
        shell.handle_event(key(KeyCode::Esc), &services, &[]);
        assert_eq!(shell.section, Section::Home);
        shell.handle_event(key(KeyCode::Tab), &services, &[]);
        assert_eq!(shell.section, Section::Commit);
        assert!(shell.config.dirty());
        assert!(
            shell
                .handle_event(key(KeyCode::Char('q')), &services, &[])
                .is_none()
        );
        assert!(matches!(shell.overlay, Some(Overlay::QuitConfirm)));
        shell.handle_event(key(KeyCode::Char('n')), &services, &[]);
        assert!(shell.config.dirty());
        shell.handle_event(key(KeyCode::Char('q')), &services, &[]);
        assert_eq!(
            shell.handle_event(key(KeyCode::Char('y')), &services, &[]),
            Some(Action::Quit)
        );
    }
    #[test]
    fn full_git_result_scrolls_and_keeps_navigation_modal() -> Result<(), Box<dyn std::error::Error>>
    {
        use std::fmt::Write as _;

        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Home, &services, None);
        shell.execute(Action::Select(Section::Commit), &services);
        for index in 0..100 {
            writeln!(&mut shell.commit.notice, "Git diagnostic {index}")?;
        }
        shell.handle_event(key(KeyCode::Char('o')), &services, &[]);
        shell.handle_event(key(KeyCode::PageDown), &services, &[]);
        shell.handle_event(key(KeyCode::Tab), &services, &[]);
        assert_eq!(shell.section, Section::Commit);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert!(rendered(&terminal).contains("Git diagnostic 10"));
        shell.handle_event(key(KeyCode::Esc), &services, &[]);
        assert!(shell.overlay.is_none());
        Ok(())
    }
    #[test]
    fn config_palette_quit_uses_one_shell_discard_confirmation() {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Config, &services, None);
        shell.handle_event(key(KeyCode::Char('i')), &services, &[]);
        shell.handle_event(key(KeyCode::Esc), &services, &[]);
        shell.handle_event(key(KeyCode::Char('/')), &services, &[]);
        shell.handle_event(Event::Paste("quit".into()), &services, &[]);
        shell.handle_event(key(KeyCode::Enter), &services, &[]);
        assert!(matches!(shell.overlay, Some(Overlay::QuitConfirm)));
        assert!(shell.config.overview());
        assert_eq!(
            shell.handle_event(key(KeyCode::Char('y')), &services, &[]),
            Some(Action::Quit)
        );
    }

    #[test]
    fn centered_root_header_keeps_positions_and_mouse_targets_after_resize()
    -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Home, &services, None);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        let original = shell.root_areas;
        let left = original[0].x;
        let right = original[Section::Help.index()].right();
        assert!(i32::from(left) - i32::from(100 - right) <= 1);
        assert!(i32::from(100 - right) - i32::from(left) <= 1);
        shell.handle_event(key(KeyCode::Tab), &services, &[]);
        shell.handle_event(key(KeyCode::Tab), &services, &[]);
        assert_eq!(shell.section, Section::Config);
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert_eq!(shell.root_areas, original);
        assert!(!shell.animating());
        let header: String = (0..100)
            .map(|x| terminal.backend().buffer()[(x, 1)].symbol())
            .collect();
        assert!(header.contains("Home   Commit   Config   Help"));
        assert!(!header.contains('['));
        assert!(rendered(&terminal).contains("Menu"));
        let config = shell.root_areas[Section::Config.index()];
        assert_eq!(
            terminal.backend().buffer()[(config.x + 1, config.y)].bg,
            Color::Yellow
        );
        terminal.backend_mut().resize(66, 18);
        terminal.resize(Rect::new(0, 0, 66, 18))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert_ne!(shell.root_areas, original);
        let help = shell.root_areas[Section::Help.index()];
        shell.handle_event(
            Event::Mouse(event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: help.x + 1,
                row: help.y,
                modifiers: KeyModifiers::NONE,
            }),
            &services,
            &[],
        );
        assert_eq!(shell.section, Section::Help);
        Ok(())
    }

    #[test]
    fn home_frame_abuts_nav_and_shimmers_wordmark() -> Result<(), Box<dyn std::error::Error>> {
        use super::home_logo::{HomeLogo, YELLOW_EPILEPTIC_DREAMS};
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Home, &services, None);
        // Pin to the default theme so `GITSERIOUS_LOGO_THEME` dev overrides
        // cannot make this timing-sensitive test flaky.
        shell.home_logo = HomeLogo::with_theme(&YELLOW_EPILEPTIC_DREAMS);
        assert!(shell.animating(), "home should tick continuously");
        for (width, height) in [(100, 30), (66, 18)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| shell.render(frame, &[]))?;
            let buffer = terminal.backend().buffer();
            // No blank gap: the Home frame starts directly under the nav box.
            assert_eq!(buffer[(0, 3)].symbol(), "┌");
            assert_eq!(buffer[(width - 1, 3)].symbol(), "┐");
            assert_eq!(buffer[(0, height - 2)].symbol(), "└");
            assert_eq!(buffer[(width - 1, height - 2)].symbol(), "┘");
            // Wordmark graphic is present (block/dither cells, not plain text).
            let body: String = (0..width)
                .flat_map(|x| (4..height - 2).map(move |y| (x, y)))
                .map(|(x, y)| buffer[(x, y)].symbol())
                .collect();
            assert!(body.contains('█'), "home should render the block wordmark");
            // Version stays centered below the graphic.
            let version = format!("gitserious v{}", env!("CARGO_PKG_VERSION"));
            let text = rendered(&terminal);
            assert!(text.contains(&version), "version must stay visible");
            let version_row = text
                .lines()
                .position(|row| row.contains(&version))
                .ok_or("missing version row")?;
            let row = text.lines().nth(version_row).ok_or("missing row")?;
            let byte_start = row.find(&version).ok_or("missing version")?;
            // `find` is byte-based but the leading border is 3 bytes / 1 cell.
            let display_start = row[..byte_start].chars().count();
            let expected = (usize::from(width) - version.len()) / 2;
            assert!(
                display_start.abs_diff(expected) <= 1,
                "version not centered: {display_start} vs {expected}"
            );
            // Logo rows sit above the version row.
            let logo_row = text
                .lines()
                .position(|row| row.contains('█'))
                .ok_or("missing logo row")?;
            assert!(logo_row < version_row, "logo must sit above version");
            let footer: String = (0..width)
                .map(|x| buffer[(x, height - 1)].symbol())
                .collect();
            assert!(footer.starts_with("tab/shift+tab: switch  /: commands  ?: help  q: quit"));
            assert!(
                buffer[(0, height - 1)]
                    .modifier
                    .contains(ratatui::style::Modifier::BOLD)
            );
            assert!(
                !buffer[(16, height - 1)]
                    .modifier
                    .contains(ratatui::style::Modifier::BOLD)
            );
        }
        // Continuous shimmer: advancing the clock visibly changes the logo.
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        let before: Vec<(String, Color)> = {
            let buffer = terminal.backend().buffer();
            (0..100)
                .flat_map(|x| (0..30).map(move |y| (x, y)))
                .map(|(x, y)| (buffer[(x, y)].symbol().to_owned(), buffer[(x, y)].fg))
                .collect()
        };
        shell.advance(Duration::from_millis(95));
        terminal.draw(|frame| shell.render(frame, &[]))?;
        let after: Vec<(String, Color)> = {
            let buffer = terminal.backend().buffer();
            (0..100)
                .flat_map(|x| (0..30).map(move |y| (x, y)))
                .map(|(x, y)| (buffer[(x, y)].symbol().to_owned(), buffer[(x, y)].fg))
                .collect()
        };
        assert_ne!(before, after, "shimmer must animate between frames");
        assert!(
            rendered(&terminal).contains(&format!("gitserious v{}", env!("CARGO_PKG_VERSION")))
        );
        // Leaving Home stops the idle animation (unless config animates).
        shell.execute(Action::Select(Section::Commit), &services);
        assert!(!shell.animating());
        Ok(())
    }

    #[test]
    fn config_review_survives_root_clicks() -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Config, &services, None);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        let original_header = shell.root_areas;
        shell.handle_event(key(KeyCode::Char('i')), &services, &[]);
        assert!(!shell.config.overview());
        assert!(!shell.config.editing_taxonomy());
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert_eq!(shell.root_areas, original_header);
        let review = rendered(&terminal);
        assert!(review.contains("Review changes"));
        shell.handle_event(
            click(shell.root_areas[Section::Help.index()]),
            &services,
            &[],
        );
        assert_eq!(shell.section, Section::Help);
        shell.handle_event(
            click(shell.root_areas[Section::Config.index()]),
            &services,
            &[],
        );
        assert_eq!(shell.section, Section::Config);
        assert!(!shell.config.overview());
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert_eq!(rendered(&terminal), review);
        Ok(())
    }

    #[test]
    fn small_create_view_preserves_draft_and_discard_confirmation()
    -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Config, &services, None);
        shell.handle_event(key(KeyCode::Down), &services, &[]);
        shell.handle_event(key(KeyCode::Char('n')), &services, &[]);
        shell.handle_event(Event::Paste("team".into()), &services, &[]);
        let mut terminal = Terminal::new(TestBackend::new(66, 21))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert!(rendered(&terminal).contains("Library / create taxonomy"));
        assert!(rendered(&terminal).contains("+ add type"));
        terminal.backend_mut().resize(66, 18);
        terminal.resize(Rect::new(0, 0, 66, 18))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert!(!shell.too_small);
        assert!(rendered(&terminal).contains("Terminal too small"));
        shell.handle_event(key(KeyCode::Esc), &services, &[]);
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert!(rendered(&terminal).contains("Discard new taxonomy"));
        terminal.backend_mut().resize(60, 18);
        terminal.resize(Rect::new(0, 0, 60, 18))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert!(shell.too_small);
        assert!(rendered(&terminal).contains("Discard new taxonomy"));
        shell.handle_event(key(KeyCode::Char('n')), &services, &[]);
        assert!(shell.config.editing_taxonomy());
        terminal.backend_mut().resize(66, 21);
        terminal.resize(Rect::new(0, 0, 66, 21))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert!(rendered(&terminal).contains("team"));
        Ok(())
    }
}
