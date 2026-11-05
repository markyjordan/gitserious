mod commit_view;
mod help_view;
mod home;
mod init_editor;

use std::io::{self, IsTerminal};
use std::time::{Duration, Instant};

use gitserious_app::{
    CommitAuthoringContext, HelpPage, HomeState, InteractiveEntry, InteractiveUi,
    InteractiveWorkspace, ProjectConfiguration,
};
use gitserious_core::built_in_taxonomies;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use crate::author::{AuthoringCheckpoint, embedded_author};
use crate::confirmation::{self, Answer, ConfirmDialog, Confirmation, DISCARD_AND_QUIT};
use crate::taxonomy_config::{ConfigView, ShellKeys};
use crate::theme::{
    FOOTER_HEIGHT, JET_BLACK, MINIMUM_HEIGHT, MINIMUM_WIDTH, SHELL_HINTS, block_title,
    centered_rect, command_hints, frame_style, navigation_key_style, normalize_background,
    render_footer, render_footer_block, theme,
};
use commit_view::CommitView;
use help_view::HelpView;
use home::HomeAction;
use init_editor::{EditorOutcome, InitEditor};

fn quit_question() -> Confirmation<'static> {
    Confirmation {
        title: "Quit",
        message: "Discard unapplied changes and quit?".into(),
        choices: DISCARD_AND_QUIT,
    }
}

/// Unified Home, Commit, Config, and Help terminal adapter.
#[derive(Clone, Copy, Debug, Default)]
pub struct RatatuiInteractiveUi;

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
            let mut shell = Shell::new(entry, workspace);
            let mut previous = Instant::now();
            loop {
                let now = Instant::now();
                shell.advance(now.duration_since(previous));
                previous = now;
                terminal.draw(|frame| shell.render(frame, help))?;
                let Some(event) = crate::runtime::next_event(shell.next_frame_in())? else {
                    continue;
                };
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
    home: HomeState,
    /// Open while the user chooses the new project's taxonomies.
    init_editor: Option<InitEditor>,
    /// Why the last initialization from Home failed.
    home_error: Option<String>,
    pending_author: Option<PendingAuthor>,
    overlay: Option<Overlay>,
    quit_dialog: ConfirmDialog,
    help: HelpView,
    root_areas: [Rect; Section::ALL.len()],
    too_small: bool,
}

impl Shell {
    fn new(entry: InteractiveEntry, workspace: &dyn InteractiveWorkspace) -> Self {
        Self {
            section: if entry == InteractiveEntry::Config {
                Section::Config
            } else {
                Section::Home
            },
            config: ConfigView::new(workspace.configuration()),
            commit: CommitView::default(),
            home: workspace.home(),
            init_editor: None,
            home_error: None,
            pending_author: None,
            overlay: None,
            quit_dialog: ConfirmDialog::default(),
            help: HelpView::default(),
            root_areas: [Rect::default(); Section::ALL.len()],
            too_small: false,
        }
    }

    fn advance(&mut self, elapsed: Duration) {
        self.config.advance(elapsed, self.config_visible());
    }

    fn config_visible(&self) -> bool {
        self.section == Section::Config && self.overlay.is_none() && !self.too_small
    }

    fn next_frame_in(&self) -> Option<Duration> {
        if self.config_visible() {
            self.config.next_frame_in()
        } else {
            None
        }
    }

    fn execute(&mut self, action: Action, workspace: &dyn InteractiveWorkspace) -> Option<Action> {
        match action {
            Action::Select(section) => {
                self.section = section;
                match section {
                    Section::Commit => self.commit.refresh(workspace),
                    Section::Home => {
                        self.home = workspace.home();
                        self.home_error = None;
                        if !HomeAction::of(&self.home).is_some_and(HomeAction::initializes) {
                            self.init_editor = None;
                        }
                    }
                    Section::Help => self.help.enter(),
                    Section::Config => {}
                }
            }
            Action::Initialize => {
                self.section = Section::Home;
                let library = workspace.library_taxonomies().unwrap_or_else(|error| {
                    self.home_error = Some(format!("Library taxonomies are unavailable: {error}"));
                    Vec::new()
                });
                let creates_repository = matches!(self.home, HomeState::NoRepository { .. });
                self.init_editor = Some(InitEditor::new(
                    built_in_taxonomies(),
                    &library,
                    creates_repository,
                ));
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
        let action = self.handle_event_inner(event, workspace, help);
        self.config.advance(Duration::ZERO, self.config_visible());
        action
    }

    fn handle_event_inner(
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
            if self.config.handle_event(event, workspace.configuration()) {
                return self.execute(Action::Quit, workspace);
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
                return self.execute(Action::Select(section), workspace);
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
                return self.execute(Action::Quit, workspace);
            }
            if self.too_small {
                return if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                    self.execute(Action::Quit, workspace)
                } else {
                    None
                };
            }
            if self.section == Section::Config {
                match (self.config.shell_keys(), key.code) {
                    (ShellKeys::Navigation, KeyCode::Tab | KeyCode::BackTab) => {
                        return self.execute(
                            Action::Select(self.section.next(key.code == KeyCode::BackTab)),
                            workspace,
                        );
                    }
                    (ShellKeys::Navigation, KeyCode::Esc) => {
                        return self.execute(Action::Select(Section::Home), workspace);
                    }
                    (ShellKeys::Navigation, KeyCode::Char('q')) => {
                        return self.execute(Action::Quit, workspace);
                    }
                    _ => {}
                }
            } else {
                if self.section == Section::Home
                    && let Some(editor) = &mut self.init_editor
                    && !matches!(
                        key.code,
                        KeyCode::Tab | KeyCode::BackTab | KeyCode::Char('q' | '/')
                    )
                {
                    match editor.key(*key) {
                        EditorOutcome::Stay => {}
                        EditorOutcome::Back => {
                            self.init_editor = None;
                            self.home_error = None;
                        }
                        EditorOutcome::Initialize(configuration) => {
                            self.initialize(workspace, &configuration);
                        }
                    }
                    return None;
                }
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
                        self.open_palette();
                        return None;
                    }
                    _ => {}
                }
                match self.section {
                    Section::Home if key.code == KeyCode::Enter => {
                        match HomeAction::of(&self.home) {
                            Some(HomeAction::GoToCommit) => {
                                return self.execute(Action::Select(Section::Commit), workspace);
                            }
                            Some(_) => return self.execute(Action::Initialize, workspace),
                            None => {}
                        }
                    }
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
                    Section::Help => self.help.key(key.code, help.len()),
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

    fn initialize(
        &mut self,
        workspace: &dyn InteractiveWorkspace,
        configuration: &ProjectConfiguration,
    ) {
        match workspace.initialize(configuration) {
            Ok(()) => {
                self.init_editor = None;
                self.home_error = None;
                // Config loaded the project before it existed; reload it unless it holds drafts.
                if !self.config.dirty() {
                    self.config = ConfigView::new(workspace.configuration());
                }
            }
            Err(error) => self.home_error = Some(error),
        }
        self.home = workspace.home();
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
        if self.section == Section::Home
            && HomeAction::of(&self.home).is_some_and(HomeAction::initializes)
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
                                return self.execute(*action, workspace);
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
        self.config.advance(Duration::ZERO, self.config_visible());
        self.root_areas = [Rect::default(); Section::ALL.len()];
        frame.render_widget(
            Block::default().style(Style::default().fg(theme().text).bg(theme().ground)),
            area,
        );
        if self.too_small {
            frame.render_widget(
                Paragraph::new("Terminal too small\nResize or press esc/q to quit").centered(),
                centered_rect(54, 3, area),
            );
        } else {
            let [header, body] =
                Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
            self.render_header(frame, header);
            if self.section == Section::Config {
                self.config.render(frame, body);
            } else {
                let [content, footer] =
                    Layout::vertical([Constraint::Min(1), Constraint::Length(FOOTER_HEIGHT)])
                        .areas(body);
                let footer = render_footer_block(frame, footer);
                match self.section {
                    Section::Home => match (&self.init_editor, &self.home) {
                        (Some(editor), HomeState::Uninitialized { root }) => editor.render(
                            frame,
                            content,
                            root.as_path(),
                            self.home_error.as_deref(),
                        ),
                        (Some(editor), HomeState::NoRepository { directory }) => {
                            editor.render(frame, content, directory, self.home_error.as_deref())
                        }
                        _ => home::render(frame, content, &self.home, self.home_error.as_deref()),
                    },
                    Section::Commit => {
                        self.commit
                            .render(frame, content, self.pending_author.is_some())
                    }
                    Section::Help => self.help.render(frame, content, help),
                    Section::Config => {}
                }
                let home_hints = self.init_editor.as_ref().map_or_else(
                    || {
                        HomeAction::of(&self.home)
                            .map(HomeAction::hint)
                            .into_iter()
                            .collect()
                    },
                    InitEditor::hints,
                );
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
        let mut x = area.x.saturating_add(1);
        for section in Section::ALL {
            let label = section.label();
            let width = u16::try_from(label.len()).unwrap_or(0);
            let rect = Rect::new(x, area.y, width.min(area.right().saturating_sub(x)), 1);
            let style = if section == self.section {
                Style::new()
                    .fg(theme().text)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::new().fg(theme().dim)
            };
            frame.render_widget(Paragraph::new(label).style(style), rect);
            self.root_areas[section.index()] = rect;
            x = x.saturating_add(width + 2);
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
mod tests {
    use ratatui::crossterm::event::{self, KeyEvent};

    use super::*;
    use std::cell::RefCell;

    use gitserious_app::{
        BranchState, CommitOutput, ConfigurationDestination, ConfigurationSession,
        ConfigurationWorkspace, GlobalConfiguration, HelpLine, HelpSpan, ProjectConfiguration,
        RepositoryRoot, RepositoryStatus,
    };
    use gitserious_core::TaxonomyVersion;
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
                ConfigurationDestination::Project => Ok(ConfigurationSession::open_project(
                    RepositoryRoot::new(std::env::temp_dir()).map_err(|error| error.to_string())?,
                    None,
                )),
            }
        }
        fn save(&self, _: &ConfigurationSession) -> Result<ConfigurationSession, String> {
            Err("test save unavailable".into())
        }
    }
    impl InteractiveWorkspace for Services {
        fn home(&self) -> HomeState {
            HomeState::Uninitialized { root: fresh_root() }
        }
        fn initialize(&self, _: &ProjectConfiguration) -> Result<(), String> {
            Err("test initialize unavailable".into())
        }
        fn library_taxonomies(&self) -> Result<Vec<gitserious_core::Taxonomy>, String> {
            Ok(Vec::new())
        }
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

    fn fresh_root() -> RepositoryRoot {
        let filesystem_root = std::env::temp_dir()
            .ancestors()
            .last()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        RepositoryRoot::new(filesystem_root.join("fresh"))
            .unwrap_or_else(|error| unreachable!("the filesystem root is absolute: {error}"))
    }

    /// A workspace whose Home state follows initialization.
    struct HomeWorkspace {
        state: RefCell<HomeState>,
        created: RefCell<Option<ProjectConfiguration>>,
        failure: Option<&'static str>,
        library: Result<Vec<gitserious_core::Taxonomy>, String>,
    }

    impl HomeWorkspace {
        fn new(state: HomeState) -> Self {
            Self {
                state: RefCell::new(state),
                created: RefCell::new(None),
                failure: None,
                library: Ok(Vec::new()),
            }
        }
    }

    impl InteractiveWorkspace for HomeWorkspace {
        fn home(&self) -> HomeState {
            self.state.borrow().clone()
        }
        fn initialize(&self, configuration: &ProjectConfiguration) -> Result<(), String> {
            if let Some(failure) = self.failure {
                return Err(failure.into());
            }
            *self.state.borrow_mut() = HomeState::Initialized {
                root: fresh_root(),
                default: configuration.default_taxonomy().clone(),
                version: TaxonomyVersion::V1,
                policy_untracked: true,
            };
            *self.created.borrow_mut() = Some(configuration.clone());
            Ok(())
        }
        fn library_taxonomies(&self) -> Result<Vec<gitserious_core::Taxonomy>, String> {
            self.library.clone()
        }
        fn configuration(&self) -> &dyn ConfigurationWorkspace {
            &Services
        }
        fn repository_status(&self) -> Result<RepositoryStatus, String> {
            Services.repository_status()
        }
        fn prepare_commit(&self) -> Result<CommitAuthoringContext, String> {
            Services.prepare_commit()
        }
        fn submit_commit(
            &self,
            authored: &gitserious_app::AuthoredCommit,
        ) -> Result<CommitOutput, String> {
            Services.submit_commit(authored)
        }
    }

    fn home_screen(
        workspace: &HomeWorkspace,
        width: u16,
        height: u16,
    ) -> Result<(Shell, Terminal<TestBackend>), Box<dyn std::error::Error>> {
        let mut shell = Shell::new(InteractiveEntry::Home, workspace);
        let mut terminal = Terminal::new(TestBackend::new(width, height))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        Ok((shell, terminal))
    }

    fn row_of(text: &str, needle: &str) -> Option<usize> {
        text.lines().position(|row| row.contains(needle))
    }

    #[test]
    fn home_names_the_directory_and_offers_only_the_step_its_state_allows()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = fresh_root();
        let path = root.as_path().display().to_string();
        let initialized = |policy_untracked| HomeState::Initialized {
            root: fresh_root(),
            default: gitserious_core::built_in_taxonomies()[0].id().clone(),
            version: TaxonomyVersion::V1,
            policy_untracked,
        };
        for (width, height) in [(87, 40), (100, 30)] {
            let workspace = HomeWorkspace::new(HomeState::NoRepository {
                directory: root.as_path().to_path_buf(),
            });
            let (_, terminal) = home_screen(&workspace, width, height)?;
            let text = rendered(&terminal);
            let hint = row_of(&text, "no git repository found").ok_or("missing hint")?;
            let column = text
                .lines()
                .nth(hint)
                .unwrap_or_default()
                .find("no git")
                .unwrap_or(0);
            let buffer = terminal.backend().buffer();
            assert_eq!(
                buffer[(u16::try_from(column)?, u16::try_from(hint)?)].fg,
                theme().pending
            );
            assert!(row_of(&text, &path).is_some(), "{text}");
            let action =
                row_of(&text, " initialize git and gitserious ").ok_or("missing action")?;
            assert_eq!(action, hint + 2, "{text}");
            assert!(
                text.lines()
                    .nth(action)
                    .unwrap_or_default()
                    .trim_end()
                    .ends_with("enter")
            );
            assert!(
                text.lines()
                    .last()
                    .unwrap_or_default()
                    .starts_with(" enter initialize")
            );

            let workspace = HomeWorkspace::new(HomeState::Uninitialized { root: fresh_root() });
            let (_, terminal) = home_screen(&workspace, width, height)?;
            let text = rendered(&terminal);
            let rows: Vec<&str> = text.lines().collect();
            let name = row_of(&text, "fresh").ok_or("missing name")?;
            assert!(rows[name + 1].contains(&path));
            let action = row_of(&text, " initialize gitserious ").ok_or("missing action")?;
            assert_eq!(action, name + 3);
            assert!(rows[action].trim_end().ends_with("enter"));
            let wordmark = row_of(&text, "▄████ ███").ok_or("missing wordmark")?;
            let left = rows[wordmark].chars().take_while(|c| *c == ' ').count();
            assert_eq!(rows[name].chars().take_while(|c| *c == ' ').count(), left);
            let buffer = terminal.backend().buffer();
            assert_eq!(
                buffer[(u16::try_from(left)?, u16::try_from(action)?)].bg,
                theme().cursor
            );
            assert_eq!(
                buffer[(u16::try_from(left + 48)?, u16::try_from(action)?)].bg,
                theme().cursor,
                "the action row spans the wordmark"
            );
            assert!(
                !text.contains("gitserious.toml"),
                "no hint about the missing file"
            );
            assert!(!text.contains("no git repository"));
            let content_rows = usize::from(height) - 2;
            let block = action - wordmark + 1;
            assert!(
                (wordmark - 1).abs_diff((content_rows - block) / 2) <= 1,
                "the block is centered vertically"
            );
            assert!(rows[usize::from(height) - 1].starts_with(" enter initialize"));

            let workspace = HomeWorkspace::new(initialized(true));
            let (_, terminal) = home_screen(&workspace, width, height)?;
            let text = rendered(&terminal);
            let rows: Vec<&str> = text.lines().collect();
            let done = row_of(&text, "✓ fresh is initialized with conventional-v1")
                .ok_or("missing initialized line")?;
            assert_eq!(
                row_of(&text, "◐ gitserious.toml is untracked."),
                Some(done + 2)
            );
            assert!(rows[done + 3].contains("Commit it so the policy travels with the repo:"));
            assert!(rows[done + 4].contains("git add gitserious.toml"));
            assert_eq!(row_of(&text, " Go to Commit "), Some(done + 6));
            assert!(rows[usize::from(height) - 1].starts_with(" enter commit"));

            let workspace = HomeWorkspace::new(initialized(false));
            let (_, terminal) = home_screen(&workspace, width, height)?;
            let text = rendered(&terminal);
            let done = row_of(&text, "is initialized with").ok_or("missing line")?;
            assert!(!text.contains("untracked"));
            assert_eq!(row_of(&text, " Go to Commit "), Some(done + 2));

            let workspace = HomeWorkspace::new(HomeState::Unavailable {
                directory: root.as_path().to_path_buf(),
                error: "git is not installed".into(),
            });
            let (_, terminal) = home_screen(&workspace, width, height)?;
            let text = rendered(&terminal);
            let error = row_of(&text, "✗ git is not installed").ok_or("missing error")?;
            let column = text
                .lines()
                .nth(error)
                .unwrap_or_default()
                .find('✗')
                .unwrap_or(0);
            let column = text.lines().nth(error).unwrap_or_default()[..column]
                .chars()
                .count();
            let buffer = terminal.backend().buffer();
            assert_eq!(
                buffer[(u16::try_from(column)?, u16::try_from(error)?)].fg,
                theme().blocks
            );
            assert!(!text.contains("enter "));
        }
        Ok(())
    }

    #[test]
    fn enter_opens_the_editor_whose_initialize_row_writes_the_chosen_taxonomies()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = HomeWorkspace::new(HomeState::Uninitialized { root: fresh_root() });
        let (mut shell, mut terminal) = home_screen(&workspace, 87, 40)?;
        shell.handle_event(key(KeyCode::Enter), &workspace, &[]);
        terminal.draw(|frame| shell.render(frame, &[]))?;
        let text = rendered(&terminal);
        assert!(text.contains(" initialize fresh"), "{text}");
        assert!(text.contains("esc back  space include  enter default"));
        shell.handle_event(key(KeyCode::Esc), &workspace, &[]);
        assert!(workspace.created.borrow().is_none(), "esc writes nothing");
        assert_eq!(
            shell.section,
            Section::Home,
            "esc closes the editor, not the shell"
        );
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert!(rendered(&terminal).contains(" initialize gitserious "));

        shell.handle_event(key(KeyCode::Enter), &workspace, &[]);
        shell.handle_event(key(KeyCode::Tab), &workspace, &[]);
        assert_eq!(shell.section, Section::Commit, "tab leaves the open editor");
        shell.handle_event(key(KeyCode::BackTab), &workspace, &[]);
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert!(
            rendered(&terminal).contains(" initialize fresh"),
            "the editor survives a visit to another section"
        );
        for _ in 0..3 {
            shell.handle_event(key(KeyCode::Down), &workspace, &[]);
        }
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert!(rendered(&terminal).contains("enter initialize"));
        shell.handle_event(key(KeyCode::Enter), &workspace, &[]);
        let created = workspace
            .created
            .borrow()
            .clone()
            .ok_or("nothing initialized")?;
        assert_eq!(created.default_taxonomy().as_str(), "conventional");
        assert_eq!(
            created
                .taxonomies()
                .iter()
                .map(|taxonomy| taxonomy.versioned().to_string())
                .collect::<Vec<_>>(),
            ["conventional-v1", "research-v1", "infra-ops-v1"]
        );
        assert_eq!(shell.section, Section::Home);
        assert!(matches!(shell.home, HomeState::Initialized { .. }));
        shell.handle_event(key(KeyCode::Enter), &workspace, &[]);
        assert_eq!(shell.section, Section::Commit);
        Ok(())
    }

    #[test]
    fn library_taxonomies_start_excluded_and_an_unreadable_library_is_named()
    -> Result<(), Box<dyn std::error::Error>> {
        let built_in = &gitserious_core::built_in_taxonomies()[0];
        let team = gitserious_core::Taxonomy::new(
            gitserious_core::TaxonomyId::new("team-changes")?,
            TaxonomyVersion::new(3)?,
            built_in.description().clone(),
            None,
            built_in.commit_types()[..3].to_vec(),
        )?;
        let mut workspace = HomeWorkspace::new(HomeState::Uninitialized { root: fresh_root() });
        workspace.library = Ok(vec![team]);
        let (mut shell, mut terminal) = home_screen(&workspace, 87, 40)?;
        shell.handle_event(key(KeyCode::Enter), &workspace, &[]);
        terminal.draw(|frame| shell.render(frame, &[]))?;
        let text = rendered(&terminal);
        let row = text
            .lines()
            .find(|row| row.contains("team-changes-v3"))
            .ok_or("missing Library taxonomy")?;
        assert!(row.starts_with(" ○ –  team-changes-v3  custom"), "{row}");
        assert!(row.trim_end().ends_with("3 types"));
        for code in [
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Char(' '),
        ] {
            shell.handle_event(key(code), &workspace, &[]);
        }
        for _ in 0..4 {
            shell.handle_event(key(KeyCode::Down), &workspace, &[]);
        }
        shell.handle_event(key(KeyCode::Enter), &workspace, &[]);
        let created = workspace
            .created
            .borrow()
            .clone()
            .ok_or("nothing initialized")?;
        assert_eq!(
            created
                .taxonomies()
                .iter()
                .map(|taxonomy| taxonomy.versioned().to_string())
                .collect::<Vec<_>>(),
            [
                "conventional-v1",
                "research-v1",
                "infra-ops-v1",
                "team-changes-v3"
            ]
        );

        let mut workspace = HomeWorkspace::new(HomeState::Uninitialized { root: fresh_root() });
        workspace.library = Err("config.toml is not valid TOML".into());
        let (mut shell, mut terminal) = home_screen(&workspace, 87, 40)?;
        shell.handle_event(key(KeyCode::Enter), &workspace, &[]);
        terminal.draw(|frame| shell.render(frame, &[]))?;
        let text = rendered(&terminal);
        assert!(
            text.contains("✗ Library taxonomies are unavailable: config.toml is not valid"),
            "{text}"
        );
        assert!(
            text.contains("✓ 3  infra-ops-v1"),
            "the built-ins stay available"
        );
        Ok(())
    }

    #[test]
    fn a_failed_initialization_shows_why_and_keeps_the_offer()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut workspace = HomeWorkspace::new(HomeState::Uninitialized { root: fresh_root() });
        workspace.failure = Some("gitserious.toml already exists");
        let (mut shell, mut terminal) = home_screen(&workspace, 100, 30)?;
        shell.handle_event(key(KeyCode::Enter), &workspace, &[]);
        for _ in 0..3 {
            shell.handle_event(key(KeyCode::Down), &workspace, &[]);
        }
        shell.handle_event(key(KeyCode::Enter), &workspace, &[]);
        terminal.draw(|frame| shell.render(frame, &[]))?;
        let text = rendered(&terminal);
        assert!(text.contains("✗ gitserious.toml already exists"), "{text}");
        assert!(text.contains(" initialize fresh"), "the editor stays open");
        shell.handle_event(key(KeyCode::Esc), &workspace, &[]);
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert!(rendered(&terminal).contains(" initialize gitserious "));
        assert!(workspace.created.borrow().is_none());
        shell.execute(Action::Select(Section::Help), &workspace);
        shell.execute(Action::Select(Section::Home), &workspace);
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert!(
            !rendered(&terminal).contains("already exists"),
            "a revisit starts clean"
        );

        let workspace = HomeWorkspace::new(HomeState::Unavailable {
            directory: fresh_root().as_path().to_path_buf(),
            error: "git is not installed".into(),
        });
        let (mut shell, _) = home_screen(&workspace, 87, 40)?;
        shell.handle_event(key(KeyCode::Enter), &workspace, &[]);
        assert_eq!(shell.section, Section::Home);
        assert!(shell.init_editor.is_none(), "enter has no step to take");

        let workspace = HomeWorkspace::new(HomeState::NoRepository {
            directory: fresh_root().as_path().to_path_buf(),
        });
        let (mut shell, mut terminal) = home_screen(&workspace, 87, 40)?;
        shell.handle_event(key(KeyCode::Enter), &workspace, &[]);
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert!(
            rendered(&terminal).contains(" initialize fresh"),
            "a plain directory opens the editor too"
        );
        Ok(())
    }

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }
    fn span(text: &str, strong: bool) -> HelpSpan {
        HelpSpan {
            text: text.into(),
            strong,
        }
    }

    fn numbered(count: usize) -> Vec<HelpLine> {
        (1..=count)
            .map(|number| vec![span(&format!("line {number}"), false)])
            .collect()
    }

    /// The column and row where `needle` first appears on screen.
    fn position(terminal: &Terminal<TestBackend>, needle: &str) -> Option<(u16, u16)> {
        rendered(terminal).lines().enumerate().find_map(|(y, row)| {
            let x = row.find(needle)?;
            Some((
                u16::try_from(row[..x].chars().count()).ok()?,
                u16::try_from(y).ok()?,
            ))
        })
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

    /// The four shell pages, with `overview` numbered lines on the first.
    fn pages(overview: usize) -> Vec<HelpPage> {
        vec![
            HelpPage::new("Overview", None, move |_| numbered(overview)),
            HelpPage::new("Init", Some("init"), |_| numbered(1)),
            HelpPage::new("Config", Some("config"), |_| numbered(1)),
            HelpPage::new("Commit", Some("commit"), |_| numbered(1)),
        ]
    }

    /// The first content row, right of the sidebar's rule.
    fn top(terminal: &Terminal<TestBackend>) -> Option<String> {
        rendered(terminal)
            .lines()
            .nth(2)
            .and_then(|row| row.split('│').nth(1))
            .map(|content| content.trim().to_owned())
    }

    fn footer(terminal: &Terminal<TestBackend>) -> String {
        rendered(terminal)
            .lines()
            .last()
            .unwrap_or_default()
            .to_owned()
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
        let mut shell = Shell::new(InteractiveEntry::Home, &services);
        shell.handle_event(key(KeyCode::Tab), &services, &[]);
        assert_eq!(shell.section, Section::Commit);
        assert!(shell.commit.blocker.is_some());
        shell.handle_event(key(KeyCode::Tab), &services, &[]);
        assert_eq!(shell.section, Section::Config);
        assert!(shell.next_frame_in().is_none());
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        let text = rendered(&terminal);
        assert!(text.contains("1  Project"));
        assert!(text.contains("tab nav bar  / commands  q quit"));
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
    fn embedded_config_splits_the_bordered_panels_equally_after_its_header_and_footer()
    -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Config, &services);
        shell.handle_event(key(KeyCode::Char('2')), &services, &[]);
        let mut terminal = Terminal::new(TestBackend::new(87, 40))?;
        for (width, height) in [
            (87, 40),
            (87, 41),
            (MINIMUM_WIDTH, MINIMUM_HEIGHT),
            (87, 39),
            (87, 40),
        ] {
            terminal.backend_mut().resize(width, height);
            terminal.resize(Rect::new(0, 0, width, height))?;
            terminal.draw(|frame| shell.render(frame, &[]))?;
            let buffer = terminal.backend().buffer();
            let content = height - 2;
            let outline = content.div_ceil(2);
            let inspector_y = 1 + outline;
            let x = (0..width)
                .find(|x| buffer[(*x, 1)].symbol() == "┌" && *x > 0)
                .ok_or("outline border")?;
            assert_eq!(buffer[(x, inspector_y - 1)].symbol(), "└");
            assert_eq!(buffer[(x, inspector_y)].symbol(), "┌");
            assert_eq!(buffer[(x, height - 2)].symbol(), "└");
            assert_eq!(outline - (height - 1 - inspector_y), content % 2);
            if height == 40 {
                assert_eq!(outline, 19);
                assert_eq!(height - 1 - inspector_y, 19);
            }
            assert_eq!(buffer[(1, height - 1)].symbol(), " ");
            assert_eq!(buffer[(2, height - 1)].symbol(), "N");
        }
        Ok(())
    }

    #[test]
    fn every_root_footer_is_one_band_row_with_the_nav_bar_hint()
    -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        for (width, height) in [(87, 40), (MINIMUM_WIDTH, MINIMUM_HEIGHT)] {
            let mut shell = Shell::new(InteractiveEntry::Config, &services);
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            for section in Section::ALL {
                shell.execute(Action::Select(section), &services);
                if section == Section::Config {
                    shell.handle_event(key(KeyCode::Char('2')), &services, &[]);
                }
                terminal.draw(|frame| shell.render(frame, &[]))?;
                let buffer = terminal.backend().buffer();
                for x in [0, width - 1] {
                    assert_eq!(buffer[(x, height - 1)].symbol(), " ", "{section:?}");
                    assert_eq!(buffer[(x, height - 1)].bg, theme().band, "{section:?}");
                }
                let footer = rendered(&terminal)
                    .lines()
                    .last()
                    .unwrap_or_default()
                    .to_owned();
                // The Library status line drops its trailing hints when crowded.
                if section != Section::Config || width == 87 {
                    assert!(footer.contains("tab nav bar"), "{section:?}: {footer}");
                }
            }
        }
        Ok(())
    }

    #[test]
    fn config_summary_deadline_advances_without_keys_and_resets_when_hidden()
    -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Config, &services);
        let mut terminal = Terminal::new(TestBackend::new(80, 30))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        for code in [
            KeyCode::Char('2'),
            KeyCode::Down,
            KeyCode::Enter,
            KeyCode::Down,
        ] {
            shell.handle_event(key(code), &services, &[]);
        }
        terminal.draw(|frame| shell.render(frame, &[]))?;
        let initial = terminal.backend().buffer().clone();
        let hold = Duration::from_secs(3);
        assert_eq!(shell.next_frame_in(), Some(hold));
        shell.advance(hold);
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert_ne!(*terminal.backend().buffer(), initial);
        assert_eq!(shell.next_frame_in(), Some(Duration::from_millis(150)));

        shell.execute(Action::Result, &services);
        terminal.draw(|frame| shell.render(frame, &[]))?;
        let overlay = terminal.backend().buffer().clone();
        shell.advance(Duration::from_secs(20));
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert_eq!(shell.next_frame_in(), None);
        assert_eq!(*terminal.backend().buffer(), overlay);
        shell.handle_event(key(KeyCode::Esc), &services, &[]);
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert_eq!(shell.next_frame_in(), Some(hold));

        for section in [Section::Commit, Section::Help, Section::Home] {
            shell.execute(Action::Select(section), &services);
            terminal.draw(|frame| shell.render(frame, &[]))?;
            assert_eq!(shell.next_frame_in(), None);
            shell.advance(Duration::from_secs(20));
            shell.execute(Action::Select(Section::Config), &services);
            terminal.draw(|frame| shell.render(frame, &[]))?;
            assert_eq!(shell.next_frame_in(), Some(hold));
        }

        terminal.backend_mut().resize(60, 15);
        terminal.resize(Rect::new(0, 0, 60, 15))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert_eq!(shell.next_frame_in(), None);
        terminal.backend_mut().resize(80, 30);
        terminal.resize(Rect::new(0, 0, 80, 30))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert_eq!(shell.next_frame_in(), Some(hold));
        Ok(())
    }

    #[test]
    fn the_config_tab_has_no_library_change_badge() -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Config, &services);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert!(!rendered(&terminal).contains('●'));
        shell.handle_event(key(KeyCode::Char('2')), &services, &[]);
        shell.handle_event(key(KeyCode::Char('n')), &services, &[]);
        terminal.draw(|frame| shell.render(frame, &[]))?;
        let header = rendered(&terminal)
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned();
        assert!(header.contains(" Config  Help "), "{header:?}");
        let config = shell.root_areas[Section::Config.index()];
        let help = shell.root_areas[Section::Help.index()];
        assert_eq!(config.width, 6);
        assert_eq!(help.x, config.right() + 2);
        assert_eq!(
            header
                .chars()
                .skip(usize::from(config.x))
                .take(6)
                .collect::<String>(),
            "Config"
        );
        Ok(())
    }

    #[test]
    fn config_edit_locks_tabs_and_gives_esc_and_q_to_the_field() {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Config, &services);
        shell.handle_event(key(KeyCode::Char('2')), &services, &[]);
        shell.handle_event(key(KeyCode::Char('n')), &services, &[]);
        assert_eq!(shell.config.shell_keys(), ShellKeys::None);
        shell.handle_event(key(KeyCode::Char('q')), &services, &[]);
        assert_eq!(shell.section, Section::Config);
        assert!(shell.overlay.is_none());
        for code in [KeyCode::Tab, KeyCode::BackTab] {
            shell.handle_event(key(code), &services, &[]);
            assert_eq!(shell.section, Section::Config);
            assert_eq!(shell.config.shell_keys(), ShellKeys::None);
        }
        shell.handle_event(key(KeyCode::Esc), &services, &[]);
        assert_eq!(shell.section, Section::Config);
        assert_eq!(shell.config.shell_keys(), ShellKeys::None);
        shell.handle_event(key(KeyCode::Up), &services, &[]);
        shell.handle_event(key(KeyCode::Backspace), &services, &[]);
        shell.handle_event(key(KeyCode::Esc), &services, &[]);
        assert_eq!(shell.config.shell_keys(), ShellKeys::Navigation);
        shell.handle_event(key(KeyCode::Tab), &services, &[]);
        assert_eq!(shell.section, Section::Help);
        shell.handle_event(key(KeyCode::BackTab), &services, &[]);
        assert_eq!(shell.section, Section::Config);
        assert!(!shell.config.dirty());
        assert_eq!(
            shell.handle_event(key(KeyCode::Char('q')), &services, &[]),
            Some(Action::Quit)
        );
    }

    #[test]
    fn question_mark_does_not_open_help_but_the_help_tab_still_works() {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Home, &services);
        shell.handle_event(key(KeyCode::Char('?')), &services, &[]);
        assert_eq!(shell.section, Section::Home);
        assert!(shell.overlay.is_none());
        shell.handle_event(key(KeyCode::BackTab), &services, &[]);
        assert_eq!(shell.section, Section::Help);
    }

    #[test]
    fn root_wrap_palette_is_modal_and_help_scrolls_with_resize()
    -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Home, &services);
        let help = vec![
            HelpPage::new("Overview", None, |_| numbered(100)),
            HelpPage::new("Config", Some("config"), |_| numbered(1)),
        ];
        shell.handle_event(key(KeyCode::BackTab), &services, &help);
        assert_eq!(shell.section, Section::Help);
        shell.handle_event(key(KeyCode::Down), &services, &help);
        let mut terminal = Terminal::new(TestBackend::new(66, 18))?;
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert_eq!(top(&terminal).as_deref(), Some("gitserious help: config"));
        shell.handle_event(key(KeyCode::Tab), &services, &help);
        assert_eq!(shell.section, Section::Home, "tab wraps from Help to Home");
        shell.handle_event(key(KeyCode::Char('/')), &services, &help);
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert!(rendered(&terminal).contains("┌ Search ─"));
        shell.handle_event(Event::Paste("open commit".into()), &services, &help);
        shell.handle_event(key(KeyCode::Tab), &services, &help);
        assert_eq!(shell.section, Section::Home);
        shell.handle_event(key(KeyCode::Enter), &services, &help);
        assert_eq!(shell.section, Section::Commit);
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert!(rendered(&terminal).contains("Staged (0)"));
        assert!(rendered(&terminal).contains("┌ Changes ─"));
        terminal.backend_mut().resize(40, 10);
        terminal.resize(Rect::new(0, 0, 40, 10))?;
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert!(rendered(&terminal).contains("Terminal too small"));
        Ok(())
    }

    #[test]
    fn the_overview_opens_on_its_help_and_bolds_only_headings_and_names()
    -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Home, &services);
        let help = vec![HelpPage::new("Overview", None, |_| {
            vec![
                vec![span("Create durable commit-message policy", false)],
                vec![],
                vec![
                    span("Usage:", true),
                    span(" ", false),
                    span("gitserious", true),
                    span(" [COMMAND]", false),
                ],
                vec![],
                vec![span("Options:", true)],
                vec![
                    span("  ", false),
                    span("-h, --help", true),
                    span("  Display the concise help", false),
                ],
            ]
        })];
        shell.handle_event(key(KeyCode::BackTab), &services, &help);
        let mut terminal = Terminal::new(TestBackend::new(66, 18))?;
        terminal.draw(|frame| shell.render(frame, &help))?;

        let text = rendered(&terminal);
        assert!(!text.contains("┌ Help"), "{text}");
        assert!(!text.contains("gitserious help:"), "{text}");
        let (x, y) = position(&terminal, "Create durable").ok_or("missing description")?;
        assert_eq!(y, 2, "one blank row under the header");
        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer[(x - 2, y)].symbol(),
            "│",
            "one column right of the rule"
        );
        assert_eq!(buffer[(x - 1, y)].symbol(), " ");
        assert_eq!(position(&terminal, "Usage:"), Some((x, y + 2)));
        for needle in ["Usage:", "gitserious [", "Options:", "-h, --help"] {
            let (x, y) = position(&terminal, needle).ok_or(needle)?;
            assert!(buffer[(x, y)].modifier.contains(Modifier::BOLD), "{needle}");
            assert_eq!(buffer[(x, y)].fg, theme().text, "{needle}");
        }
        for (needle, color) in [("[COMMAND]", theme().text), ("Display", theme().dim)] {
            let (x, y) = position(&terminal, needle).ok_or(needle)?;
            assert!(
                !buffer[(x, y)].modifier.contains(Modifier::BOLD),
                "{needle}"
            );
            assert_eq!(buffer[(x, y)].fg, color, "{needle}");
        }
        Ok(())
    }

    #[test]
    fn command_pages_open_with_their_help_command_and_no_prompt()
    -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Home, &services);
        let help = pages(1);
        shell.handle_event(key(KeyCode::BackTab), &services, &help);
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        for (digit, command) in [
            ('1', None),
            ('2', Some("init")),
            ('3', Some("config")),
            ('4', Some("commit")),
        ] {
            shell.handle_event(key(KeyCode::Char(digit)), &services, &help);
            terminal.draw(|frame| shell.render(frame, &help))?;
            let text = rendered(&terminal);
            assert!(!text.contains("$ "), "{text}");
            let Some(command) = command else {
                assert_eq!(top(&terminal).as_deref(), Some("line 1"), "{text}");
                continue;
            };

            let header = format!("gitserious help: {command}");
            assert_eq!(top(&terminal).as_deref(), Some(header.as_str()), "{text}");
            let (x, y) = position(&terminal, &header).ok_or("missing header")?;
            assert_eq!(position(&terminal, "line 1"), Some((x, y + 2)), "{text}");
            let buffer = terminal.backend().buffer();
            for (column, color) in [(x, theme().dim), (x + 17, theme().text)] {
                assert_eq!(buffer[(column, y)].fg, color, "{header} at {column}");
                assert!(!buffer[(column, y)].modifier.contains(Modifier::BOLD));
            }
        }
        Ok(())
    }

    #[test]
    fn help_keeps_a_wrapped_description_under_its_column() -> Result<(), Box<dyn std::error::Error>>
    {
        const DESCRIPTION: &str = "Author and create a durable commit from the staged index";
        let listing = |width: u16| {
            let room = usize::from(width) - 10;
            let mut rows = vec![String::new()];
            for word in DESCRIPTION.split(' ') {
                let row = rows.last().map_or(0, String::len);
                if row > 0 && row + 1 + word.len() > room {
                    rows.push(String::new());
                }
                if let Some(row) = rows.last_mut() {
                    if !row.is_empty() {
                        row.push(' ');
                    }
                    row.push_str(word);
                }
            }
            rows.iter()
                .enumerate()
                .map(|(index, row)| {
                    if index == 0 {
                        vec![
                            span("  ", false),
                            span("commit", true),
                            span("  ", false),
                            span(row, false),
                        ]
                    } else {
                        vec![span(&format!("{:10}{row}", ""), false)]
                    }
                })
                .collect()
        };
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Home, &services);
        let help = vec![HelpPage::new("Overview", None, listing)];
        shell.handle_event(key(KeyCode::BackTab), &services, &help);
        let mut terminal = Terminal::new(TestBackend::new(66, 18))?;
        terminal.draw(|frame| shell.render(frame, &help))?;

        let text = rendered(&terminal);
        let (left, _) = position(&terminal, "  commit  Author").ok_or("missing listing")?;
        let rows: Vec<String> = text
            .lines()
            .map(|row| row.chars().skip(usize::from(left)).collect())
            .collect();
        let (column, y) = position(&terminal, "Author").ok_or("missing description")?;
        let continuation = &rows[usize::from(y) + 1];
        assert_eq!(
            continuation.len() - continuation.trim_start().len(),
            usize::from(column - left),
            "{text}"
        );
        assert_eq!(
            format!(
                "{} {}",
                rows[usize::from(y)].trim_end(),
                continuation.trim()
            ),
            format!("  commit  {DESCRIPTION}"),
            "{text}"
        );
        Ok(())
    }

    #[test]
    fn help_lists_its_pages_beside_a_rule_and_highlights_the_chosen_one()
    -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Home, &services);
        let help = pages(1);
        shell.handle_event(key(KeyCode::BackTab), &services, &help);
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| shell.render(frame, &help))?;

        let text = rendered(&terminal);
        for (index, label) in ["1  Overview", "2  Init", "3  Config", "4  Commit"]
            .into_iter()
            .enumerate()
        {
            let y = 2 + u16::try_from(index)?;
            assert_eq!(position(&terminal, label), Some((1, y)), "{text}");
        }
        // The widest label and one column on each side.
        let rule = 1 + 11 + 1;
        let fifth: String = text
            .lines()
            .nth(6)
            .unwrap_or_default()
            .chars()
            .take(rule)
            .collect();
        assert_eq!(fifth.trim(), "", "no fifth page: {text}");
        let rule = u16::try_from(rule)?;
        let buffer = terminal.backend().buffer();
        for y in 1..MINIMUM_HEIGHT - FOOTER_HEIGHT {
            assert_eq!(buffer[(rule, y)].symbol(), "│", "row {y}");
            assert_eq!(buffer[(rule, y)].fg, theme().dim, "row {y}");
        }
        assert_eq!(position(&terminal, "line 1"), Some((rule + 2, 2)));

        for x in 0..rule {
            let cell = &buffer[(x, 2)];
            assert_eq!((cell.fg, cell.bg), (theme().ground, theme().text), "{x}");
            assert!(cell.modifier.contains(Modifier::BOLD), "{x}");
        }
        let (x, y) = position(&terminal, "2  Init").ok_or("missing Init")?;
        assert_eq!(
            (buffer[(x, y)].fg, buffer[(x, y)].bg),
            (theme().dim, theme().ground)
        );
        assert!(
            footer(&terminal).starts_with(" → read  ↑↓ pages  1-4 jump "),
            "{}",
            footer(&terminal)
        );
        Ok(())
    }

    #[test]
    fn pages_focus_arrows_digits_and_clicks_choose_the_page_without_scrolling()
    -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Home, &services);
        let help = pages(100);
        shell.handle_event(key(KeyCode::BackTab), &services, &help);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert_eq!(top(&terminal).as_deref(), Some("line 1"));

        for code in [KeyCode::Right, KeyCode::Down, KeyCode::Down, KeyCode::Left] {
            shell.handle_event(key(code), &services, &help);
        }
        terminal.draw(|frame| shell.render(frame, &help))?;
        let scrolled = rendered(&terminal);
        assert_eq!(
            top(&terminal).as_deref(),
            Some("line 3"),
            "reading scrolled the page"
        );
        shell.handle_event(key(KeyCode::Up), &services, &help);
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert_eq!(
            rendered(&terminal),
            scrolled,
            "up on the first page scrolls nothing"
        );

        shell.handle_event(key(KeyCode::Down), &services, &help);
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert_eq!(top(&terminal).as_deref(), Some("gitserious help: init"));
        shell.handle_event(key(KeyCode::Up), &services, &help);
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert_eq!(
            top(&terminal).as_deref(),
            Some("line 1"),
            "a new page starts at its top"
        );

        shell.handle_event(key(KeyCode::Char('3')), &services, &help);
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert_eq!(top(&terminal).as_deref(), Some("gitserious help: config"));
        shell.handle_event(key(KeyCode::Char('5')), &services, &help);
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert_eq!(top(&terminal).as_deref(), Some("gitserious help: config"));

        shell.handle_event(key(KeyCode::Right), &services, &help);
        let (x, y) = position(&terminal, "4  Commit").ok_or("missing Commit")?;
        shell.handle_event(click(Rect::new(x, y, 1, 1)), &services, &help);
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert_eq!(top(&terminal).as_deref(), Some("gitserious help: commit"));
        assert!(footer(&terminal).contains("→ read  ↑↓ pages  1-4 jump"));
        assert_eq!(terminal.backend().buffer()[(x, y)].bg, theme().text);
        shell.handle_event(key(KeyCode::Down), &services, &help);
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert_eq!(
            top(&terminal).as_deref(),
            Some("gitserious help: commit"),
            "the last page stays chosen"
        );
        Ok(())
    }

    #[test]
    fn reading_focus_scrolls_by_line_and_by_screen() -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Home, &services);
        let help = pages(100);
        shell.handle_event(key(KeyCode::BackTab), &services, &help);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| shell.render(frame, &help))?;
        // The numbered line at the top of the content, if any.
        let mut press = |code| -> Result<Option<u16>, Box<dyn std::error::Error>> {
            shell.handle_event(key(code), &services, &help);
            terminal.draw(|frame| shell.render(frame, &help))?;
            Ok(top(&terminal)
                .as_deref()
                .and_then(|row| row.strip_prefix("line "))
                .and_then(|number| number.parse().ok()))
        };

        assert_eq!(press(KeyCode::Right)?, Some(1), "reading starts at the top");
        assert_eq!(press(KeyCode::Down)?, Some(2));
        // 30 rows less the header, the row under it, and the footer.
        assert_eq!(press(KeyCode::PageDown)?, Some(2 + 27));
        assert_eq!(press(KeyCode::PageUp)?, Some(2));
        assert_eq!(press(KeyCode::Up)?, Some(1));
        assert_eq!(press(KeyCode::Up)?, Some(1), "the top stops scrolling");

        let buffer = terminal.backend().buffer();
        let (x, y) = position(&terminal, "1  Overview").ok_or("missing Overview")?;
        assert_eq!(
            (buffer[(x, y)].fg, buffer[(x, y)].bg),
            (theme().text, theme().ground)
        );
        assert!(
            buffer[(x, y)]
                .modifier
                .contains(Modifier::BOLD | Modifier::UNDERLINED)
        );
        assert_eq!(buffer[(x - 1, y)].bg, theme().ground, "no highlight bar");
        let (x, y) = position(&terminal, "2  Init").ok_or("missing Init")?;
        assert_eq!(buffer[(x, y)].fg, theme().dim);
        assert!(
            footer(&terminal).starts_with(" ↑↓ scroll  pgup/pgdn screen  ← pages "),
            "{}",
            footer(&terminal)
        );

        shell.handle_event(key(KeyCode::Left), &services, &help);
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert!(footer(&terminal).contains("→ read  ↑↓ pages  1-4 jump"));
        shell.handle_event(key(KeyCode::Enter), &services, &help);
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert!(footer(&terminal).contains("↑↓ scroll"), "enter reads too");
        Ok(())
    }

    #[test]
    fn help_reads_to_its_last_line_and_keeps_the_way_back_at_the_minimum_size()
    -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Home, &services);
        let help = pages(40);
        shell.handle_event(key(KeyCode::BackTab), &services, &help);
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert!(!rendered(&terminal).contains("line 40"));
        let (left, _) = position(&terminal, "line 1").ok_or("missing first line")?;
        assert!(footer(&terminal).contains("→ read  ↑↓ pages  1-4 jump"));

        shell.handle_event(key(KeyCode::Right), &services, &help);
        for _ in 0..2 {
            shell.handle_event(key(KeyCode::PageDown), &services, &help);
            terminal.draw(|frame| shell.render(frame, &help))?;
        }
        let last = MINIMUM_HEIGHT - FOOTER_HEIGHT - 1;
        assert_eq!(position(&terminal, "line 40"), Some((left, last)));
        let hints = footer(&terminal);
        assert!(hints.contains("↑↓ scroll  ← pages"), "{hints}");
        assert!(hints.contains("tab nav bar  / commands  q quit"), "{hints}");

        shell.handle_event(key(KeyCode::Up), &services, &help);
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert_eq!(position(&terminal, "line 39"), Some((left, last)));
        Ok(())
    }

    #[test]
    fn returning_to_help_starts_in_the_sidebar_on_the_page_left_last()
    -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Home, &services);
        let help = pages(1);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        shell.handle_event(key(KeyCode::BackTab), &services, &help);
        shell.handle_event(key(KeyCode::Char('3')), &services, &help);
        shell.handle_event(key(KeyCode::Right), &services, &help);
        shell.handle_event(key(KeyCode::Esc), &services, &help);
        assert_eq!(
            shell.section,
            Section::Home,
            "esc still goes Home while reading"
        );

        let palette = [KeyCode::Char('/')]
            .into_iter()
            .map(key)
            .chain([Event::Paste("open help".into()), key(KeyCode::Enter)]);
        for event in palette {
            shell.handle_event(event, &services, &help);
        }
        assert_eq!(shell.section, Section::Help);
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert_eq!(top(&terminal).as_deref(), Some("gitserious help: config"));
        assert!(footer(&terminal).contains("→ read  ↑↓ pages  1-4 jump"));
        let (x, y) = position(&terminal, "3  Config").ok_or("missing Config")?;
        assert_eq!(terminal.backend().buffer()[(x, y)].bg, theme().text);

        shell.handle_event(key(KeyCode::Right), &services, &help);
        shell.handle_event(key(KeyCode::Tab), &services, &help);
        assert_eq!(
            shell.section,
            Section::Home,
            "tab still leaves while reading"
        );
        shell.handle_event(
            click(shell.root_areas[Section::Help.index()]),
            &services,
            &help,
        );
        terminal.draw(|frame| shell.render(frame, &help))?;
        assert!(footer(&terminal).contains("→ read  ↑↓ pages  1-4 jump"));
        shell.handle_event(key(KeyCode::Right), &services, &help);
        assert_eq!(
            shell.handle_event(key(KeyCode::Char('q')), &services, &help),
            Some(Action::Quit)
        );
        Ok(())
    }

    #[test]
    fn config_drafts_survive_section_visits_and_quit_requires_confirmation() {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Config, &services);
        shell.handle_event(key(KeyCode::Char('i')), &services, &[]);
        assert!(shell.config.dirty());
        shell.handle_event(key(KeyCode::Esc), &services, &[]);
        assert_eq!(shell.config.shell_keys(), ShellKeys::Navigation);
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
        let mut shell = Shell::new(InteractiveEntry::Home, &services);
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
        let mut shell = Shell::new(InteractiveEntry::Config, &services);
        shell.handle_event(key(KeyCode::Char('i')), &services, &[]);
        shell.handle_event(key(KeyCode::Esc), &services, &[]);
        shell.handle_event(key(KeyCode::Char('/')), &services, &[]);
        shell.handle_event(Event::Paste("quit".into()), &services, &[]);
        shell.handle_event(key(KeyCode::Enter), &services, &[]);
        assert!(matches!(shell.overlay, Some(Overlay::QuitConfirm)));
        assert_eq!(shell.config.shell_keys(), ShellKeys::Navigation);
        assert_eq!(
            shell.handle_event(key(KeyCode::Char('y')), &services, &[]),
            Some(Action::Quit)
        );
    }

    #[test]
    fn quit_confirmation_footer_shows_only_the_dialogs_choices()
    -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Config, &services);
        shell.handle_event(key(KeyCode::Char('i')), &services, &[]);
        shell.handle_event(key(KeyCode::Esc), &services, &[]);
        shell.handle_event(key(KeyCode::Char('q')), &services, &[]);
        assert!(matches!(shell.overlay, Some(Overlay::QuitConfirm)));
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert!(rendered(&terminal).contains("Discard unapplied changes and quit?"));
        let buffer = terminal.backend().buffer();
        let footer = (0..100)
            .map(|x| buffer[(x, 29)].symbol())
            .collect::<String>();
        assert_eq!(footer.trim_end(), " y discard and quit  enter/esc/n stay");
        assert_eq!(
            shell.handle_event(key(KeyCode::Char('n')), &services, &[]),
            None
        );
        assert!(shell.overlay.is_none());
        Ok(())
    }

    #[test]
    fn tab_and_shift_tab_switch_sections_in_one_press_and_arrows_never_do()
    -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Home, &services);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        let header: String = (0..100)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
            .collect();
        let version = format!("gitserious v{} ", env!("CARGO_PKG_VERSION"));
        assert!(header.starts_with(" Home  Commit  Config  Help "));
        assert!(header.ends_with(&version));
        let original = shell.root_areas;

        for expected in [
            Section::Commit,
            Section::Config,
            Section::Help,
            Section::Home,
        ] {
            shell.handle_event(key(KeyCode::Tab), &services, &[]);
            assert_eq!(shell.section, expected, "one tab moves one section");
            terminal.draw(|frame| shell.render(frame, &[]))?;
            let active = shell.root_areas[expected.index()];
            let cell = terminal.backend().buffer()[(active.x, 0)].clone();
            assert_eq!(cell.fg, theme().text);
            assert!(
                cell.modifier
                    .contains(Modifier::UNDERLINED | Modifier::BOLD)
            );
            let text = rendered(&terminal);
            assert!(!text.contains("tab back"), "no separate nav bar step");
            assert!(text.contains("tab nav bar"), "{expected:?}");
        }
        for expected in [
            Section::Help,
            Section::Config,
            Section::Commit,
            Section::Home,
        ] {
            shell.handle_event(key(KeyCode::BackTab), &services, &[]);
            assert_eq!(
                shell.section, expected,
                "one shift+tab moves back one section"
            );
        }
        for code in [KeyCode::Left, KeyCode::Right] {
            shell.handle_event(key(code), &services, &[]);
            assert_eq!(shell.section, Section::Home, "arrows never switch sections");
        }

        terminal.backend_mut().resize(66, 18);
        terminal.resize(Rect::new(0, 0, 66, 18))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert_eq!(shell.root_areas, original, "tabs stay left-aligned");
        let help = shell.root_areas[Section::Help.index()];
        shell.handle_event(click(help), &services, &[]);
        assert_eq!(shell.section, Section::Help);
        Ok(())
    }

    #[test]
    fn home_draws_a_static_wordmark_and_tagline_in_bone() -> Result<(), Box<dyn std::error::Error>>
    {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Home, &services);
        assert_eq!(shell.next_frame_in(), None, "Home never redraws while idle");
        for (width, height) in [(87, 40), (100, 30), (120, 30), (66, 18)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| shell.render(frame, &[]))?;
            let text = rendered(&terminal);
            let rows: Vec<&str> = text.lines().collect();
            let wordmark = rows
                .iter()
                .position(|row| row.contains("▄████ ███ █████"))
                .ok_or("missing wordmark")?;
            let tagline = rows
                .iter()
                .position(|row| row.contains(home::TAGLINE))
                .ok_or("missing tagline")?;
            assert_eq!(
                tagline,
                wordmark + 6,
                "one blank row under the five-row wordmark"
            );
            let start = rows[wordmark].chars().take_while(|c| *c == ' ').count();
            assert!(
                start.abs_diff((usize::from(width) - 49) / 2) <= 1,
                "wordmark is centered"
            );
            let footer = rows[usize::from(height) - 1];
            assert!(
                footer.ends_with("tab nav bar  / commands  q quit "),
                "{footer}"
            );

            let buffer = terminal.backend().buffer();
            for y in [0, height - 1] {
                assert_eq!(buffer[(0, y)].bg, theme().band);
            }
            assert_eq!(buffer[(0, 1)].bg, theme().ground);
            let tab = footer.find("tab nav").ok_or("missing tab hint")?;
            assert!(
                buffer[(u16::try_from(tab)?, height - 1)]
                    .modifier
                    .contains(Modifier::BOLD)
            );
            assert_eq!(
                buffer[(u16::try_from(tab + 4)?, height - 1)].fg,
                theme().dim
            );
            let palette = [
                theme().ground,
                theme().band,
                theme().text,
                theme().dim,
                theme().cursor,
            ];
            for cell in buffer.content() {
                assert!(
                    palette.contains(&cell.fg),
                    "{cell:?} uses a color outside Bone"
                );
                assert!(
                    palette.contains(&cell.bg),
                    "{cell:?} uses a color outside Bone"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn config_review_survives_root_clicks() -> Result<(), Box<dyn std::error::Error>> {
        let services = Services;
        let mut shell = Shell::new(InteractiveEntry::Config, &services);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        let original_header = shell.root_areas;
        shell.handle_event(key(KeyCode::Char('i')), &services, &[]);
        assert_eq!(shell.config.shell_keys(), ShellKeys::None);
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
        assert_eq!(shell.config.shell_keys(), ShellKeys::None);
        terminal.draw(|frame| shell.render(frame, &[]))?;
        assert_eq!(rendered(&terminal), review);
        Ok(())
    }
}
