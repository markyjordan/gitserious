use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::confirmation::{
    self, APPLY_OR_DISCARD, Answer, ConfirmDialog, Confirmation, DISCARD_AND_QUIT, REMOVE,
};
use gitserious_app::{
    ConfigurationDestination, ConfigurationEditor, ConfigurationSession, ConfigurationWorkspace,
    DraftEdit, EditRefusal, LibraryDraft, NodeId, NodeRef, ProjectConfig, RepositoryRoot,
    TaxonomyOrigin, resolve_effective_configuration,
};
use gitserious_core::{Taxonomy, TaxonomyId};
use ratatui::Frame;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::theme::{
    ACCENT, FOOTER_HEIGHT, JET_BLACK, MINIMUM_HEIGHT, MINIMUM_WIDTH, block_title, centered_rect,
    focus_frame_style, frame_style, navigation_key_style, normalize_background,
    render_footer_block, render_navigation_row, row_style, section_heading_style, selected_style,
};

#[cfg(test)]
mod copy_tests;
#[cfg(test)]
mod direct_tests;
mod home_overlay;
#[cfg(test)]
mod model_check;
mod naming;
mod review;
#[cfg(test)]
mod summary_scroll_tests;
mod taxonomies;
use home_overlay::{CommandPaletteState, HomeCommand};
use naming::sentence;
use taxonomies::{LibraryAction, Nav, TaxonomiesView};

/// Taxonomy-first terminal configuration adapter.
#[derive(Clone, Debug, Default)]
pub struct RatatuiTaxonomyConfigurationEditor {
    library_path: Option<PathBuf>,
}

impl RatatuiTaxonomyConfigurationEditor {
    /// Supplies the resolved user-library source for Project resolution details.
    #[must_use]
    pub fn with_library_path(path: PathBuf) -> Self {
        Self {
            library_path: Some(path),
        }
    }
}

impl ConfigurationEditor for RatatuiTaxonomyConfigurationEditor {
    fn edit(&self, workspace: &dyn ConfigurationWorkspace) -> Result<(), String> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err("configuration editing requires an interactive terminal".into());
        }
        crate::runtime::run(|terminal| {
            let mut state = State::new(workspace);
            state.library_path.clone_from(&self.library_path);
            let mut previous = Instant::now();
            loop {
                let now = Instant::now();
                state.advance(now.duration_since(previous), true);
                previous = now;
                terminal.draw(|frame| state.render(frame))?;
                let Some(event) = crate::runtime::next_event(state.next_frame_in())? else {
                    continue;
                };
                previous = Instant::now();
                if state.handle_event(event, workspace) {
                    return Ok::<_, io::Error>(());
                }
            }
        })
        .map_err(|error| format!("configuration editing failed: {error}"))
    }
}

/// Configuration content hosted by the unified shell's terminal runtime.
pub(crate) struct ConfigView {
    state: State,
}

impl ConfigView {
    pub(crate) fn new(
        workspace: &dyn ConfigurationWorkspace,
        library_path: Option<PathBuf>,
    ) -> Self {
        let mut state = State::new(workspace);
        state.embedded = true;
        state.library_path = library_path;
        Self { state }
    }

    pub(crate) fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.state.render_at(frame, area);
    }

    pub(crate) fn advance(&mut self, elapsed: Duration, host_visible: bool) {
        self.state.advance(elapsed, host_visible);
    }

    pub(crate) fn next_frame_in(&self) -> Option<Duration> {
        self.state.next_frame_in()
    }

    /// Returns true when the configuration content requests that the shell quit.
    pub(crate) fn handle_event(
        &mut self,
        event: Event,
        workspace: &dyn ConfigurationWorkspace,
    ) -> bool {
        self.state.handle_event(event, workspace)
    }

    /// Which keys the shell may take before Config sees them.
    pub(crate) fn shell_keys(&self) -> ShellKeys {
        match (&self.state.screen, &self.state.home_overlay) {
            (Screen::Home, None) if self.state.category == Category::Taxonomies => {
                self.state.taxonomies.shell_keys()
            }
            (Screen::Home, None) => ShellKeys::Navigation,
            _ => ShellKeys::None,
        }
    }

    pub(crate) fn modal(&self) -> bool {
        self.state.home_overlay.is_some() || self.state.taxonomies.editing()
    }

    pub(crate) fn dirty(&self) -> bool {
        self.state.any_dirty() || self.state.settings.is_some()
    }
}

/// The keys the shell may take before Config sees them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellKeys {
    /// The shell switches sections on tab, goes Home on esc, and quits on q.
    Navigation,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Category {
    Project,
    Taxonomies,
}

impl Category {
    const ALL: [Self; 2] = [Self::Project, Self::Taxonomies];

    const fn index(self) -> usize {
        match self {
            Self::Project => 0,
            Self::Taxonomies => 1,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Taxonomies => "Library",
        }
    }

    const fn next(self) -> Self {
        match self {
            Self::Project => Self::Taxonomies,
            Self::Taxonomies => Self::Project,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HomeFocus {
    Menu,
    Content,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectSection {
    Commit,
    Resolution,
    Files,
}

impl ProjectSection {
    const ALL: [Self; 3] = [Self::Commit, Self::Resolution, Self::Files];

    const fn index(self) -> usize {
        match self {
            Self::Commit => 0,
            Self::Resolution => 1,
            Self::Files => 2,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Commit => "Commit",
            Self::Resolution => "Resolution",
            Self::Files => "Files",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum PendingAction {
    EditProject,
    InitializeProject,
    ReviewProject,
    Library(LibraryAction),
    Delete,
}

enum Notice {
    Error(String),
    Info(String),
}

impl Notice {
    fn error(message: String) -> Self {
        Self::Error(sentence(&message))
    }

    fn info(message: String) -> Self {
        Self::Info(sentence(&message))
    }
}

#[derive(Clone, Debug)]
enum Screen {
    Home,
    Settings(ConfigurationDestination),
    Review(ConfigurationDestination),
    Leave,
    ScopeChange(PendingAction),
    /// Asks before a taxonomy, type, or property leaves the Library.
    ConfirmRemove(NodeId),
}

const MENU_WIDTH: u16 = 16;
#[derive(Clone, Debug)]
struct SettingsDraft {
    selected: usize,
    default: TaxonomyId,
    available: Vec<TaxonomyId>,
    inherit_default: bool,
    inherit_available: bool,
}

struct State {
    embedded: bool,
    library_path: Option<PathBuf>,
    global: Option<ConfigurationSession>,
    global_error: String,
    project: Option<ConfigurationSession>,
    project_error: String,
    active_scope: ConfigurationDestination,
    category: Category,
    home_focus: HomeFocus,
    taxonomies: TaxonomiesView,
    project_section: ProjectSection,
    screen: Screen,
    home_overlay: Option<CommandPaletteState>,
    scroll: u16,
    project_scroll: u16,
    status: Option<Notice>,
    dialog: ConfirmDialog,
    settings: Option<SettingsDraft>,
    too_small: bool,
    host_visible: bool,
    project_area: Rect,
    tab_areas: [Rect; 2],
}

impl State {
    fn new(workspace: &dyn ConfigurationWorkspace) -> Self {
        let global_result = workspace.load(ConfigurationDestination::Global);
        let project_result = workspace.load(ConfigurationDestination::Project);
        let (global, global_error) = split_result(global_result);
        let (project, project_error) = split_result(project_result);
        Self {
            embedded: false,
            library_path: None,
            global,
            global_error,
            project,
            project_error,
            active_scope: ConfigurationDestination::Project,
            category: Category::Project,
            home_focus: HomeFocus::Menu,
            taxonomies: TaxonomiesView::default(),
            project_section: ProjectSection::Commit,
            screen: Screen::Home,
            home_overlay: None,
            scroll: 0,
            project_scroll: 0,
            status: None,
            dialog: ConfirmDialog::default(),
            settings: None,
            too_small: false,
            host_visible: true,
            project_area: Rect::default(),
            tab_areas: [Rect::default(); 2],
        }
    }

    /// The question the current screen asks, if it is a confirmation. Its choices drive the
    /// dialog's keys and buttons and the footer's hints.
    fn confirmation(&self) -> Option<Confirmation<'static>> {
        let (title, message, choices) = match &self.screen {
            Screen::ConfirmRemove(node) => {
                let library = self.library()?;
                match library.node(node)? {
                    NodeRef::Taxonomy(taxonomy) => (
                        "Delete Taxonomy",
                        format!("Delete taxonomy \"{}\" from the Library?", taxonomy.name()),
                        REMOVE,
                    ),
                    NodeRef::Type { commit_type, .. } => (
                        "Remove Type",
                        format!("Remove type \"{}\" from this taxonomy?", commit_type.name()),
                        REMOVE,
                    ),
                    NodeRef::Property {
                        commit_type,
                        property,
                        ..
                    } => (
                        "Remove Property",
                        format!(
                            "Remove property \"{}\" from type \"{}\"?",
                            property.name(),
                            commit_type.name()
                        ),
                        REMOVE,
                    ),
                }
            }
            Screen::Leave => (
                "Quit",
                "Discard unapplied changes and quit?".to_owned(),
                DISCARD_AND_QUIT,
            ),
            Screen::ScopeChange(_) => (
                "Unapplied Changes",
                "This project has unapplied changes. Apply or discard them first?".into(),
                APPLY_OR_DISCARD,
            ),
            _ => return None,
        };
        Some(Confirmation {
            title,
            message,
            choices,
        })
    }

    fn handle_confirmation_event(
        &mut self,
        choices: &[confirmation::Choice],
        event: &Event,
        workspace: &dyn ConfigurationWorkspace,
    ) -> bool {
        let Some(answer) = self.dialog.handle_event(choices, event) else {
            return false;
        };
        self.dialog.invalidate();
        match (self.screen.clone(), answer) {
            (Screen::ConfirmRemove(node), Answer::Confirm) => self.remove(&node),
            (Screen::Leave, Answer::Confirm) => return true,
            (Screen::ScopeChange(action), Answer::Confirm) => {
                self.apply_scope_then(action, workspace)
            }
            (Screen::ScopeChange(action), Answer::Alternate) => {
                self.discard_scope_then(action, workspace);
            }
            _ => self.screen = Screen::Home,
        }
        false
    }

    fn animation_visible(&self) -> bool {
        self.host_visible
            && !self.too_small
            && matches!(self.screen, Screen::Home)
            && self.category == Category::Taxonomies
            && self.home_focus == HomeFocus::Content
            && self.home_overlay.is_none()
            && self.taxonomies.navigating()
    }

    fn advance(&mut self, elapsed: Duration, host_visible: bool) {
        self.host_visible = host_visible;
        if self.animation_visible() {
            self.taxonomies.advance(elapsed);
        } else {
            self.taxonomies.reset_animation();
        }
    }

    fn next_frame_in(&self) -> Option<Duration> {
        self.animation_visible()
            .then(|| self.taxonomies.next_frame_in())
            .flatten()
    }

    fn handle_event(&mut self, event: Event, workspace: &dyn ConfigurationWorkspace) -> bool {
        if matches!(event, Event::Resize(_, _)) {
            self.taxonomies.reset_animation();
        }
        let before_global = self.global.clone();
        let before_view = self.taxonomies.clone();
        let before_screen = self.screen.clone();
        let quit = self.handle_event_inner(event, workspace);
        let finished_edit = before_view.editing() && !self.taxonomies.editing();
        let immediate_change =
            !self.taxonomies.editing() && self.scope_dirty(ConfigurationDestination::Global);
        if (finished_edit || immediate_change)
            && let Some(session) = self.global.as_ref()
        {
            let candidate = session.library().and_then(|library| library.saved().ok());
            let result = if session.is_dirty() {
                workspace.save(session).map(|_| ())
            } else {
                Ok(())
            };
            match result {
                Ok(()) => {
                    if let Some(candidate) = candidate
                        && let Some(library) = self
                            .global
                            .as_mut()
                            .and_then(ConfigurationSession::library_mut)
                    {
                        *library = candidate;
                    }
                    if immediate_change {
                        self.status = Some(Notice::info("Saved.".into()));
                        if !self.scope_dirty(ConfigurationDestination::Project) {
                            self.replace_session(
                                ConfigurationDestination::Project,
                                workspace.load(ConfigurationDestination::Project),
                            );
                        }
                    }
                }
                Err(error) => {
                    self.global = before_global;
                    self.taxonomies = before_view;
                    self.screen = before_screen;
                    self.status = Some(Notice::error(error));
                    return false;
                }
            }
        }
        if !self.animation_visible() {
            self.taxonomies.reset_animation();
        }
        quit
    }

    fn handle_event_inner(&mut self, event: Event, workspace: &dyn ConfigurationWorkspace) -> bool {
        if matches!(&event, Event::Key(key) if key.kind == KeyEventKind::Release) {
            return false;
        }
        if self.home_overlay.is_some() {
            return self.handle_home_overlay(event, workspace);
        }
        if let Some(question) = self.confirmation() {
            return self.handle_confirmation_event(question.choices, &event, workspace);
        }
        if matches!(event, Event::Resize(_, _)) {
            self.dialog.invalidate();
        }
        if matches!(
            &event,
            Event::Key(_)
                | Event::Paste(_)
                | Event::Mouse(ratatui::crossterm::event::MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    ..
                })
        ) {
            self.status = None;
        }
        if let Event::Paste(text) = &event {
            if matches!(self.screen, Screen::Home)
                && self.category == Category::Taxonomies
                && let Some(library) = self
                    .global
                    .as_mut()
                    .and_then(ConfigurationSession::library_mut)
                && let Err(error) = self.taxonomies.paste(text, library)
            {
                self.status = Some(Notice::error(error));
            }
            return false;
        }
        if let Event::Mouse(mouse) = event {
            if self.too_small
                || self.taxonomies.picker_hints().is_some()
                || self.taxonomies.editing()
            {
                return false;
            }
            if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                if matches!(self.screen, Screen::Home)
                    && let Some(category) = Category::ALL.into_iter().find(|category| {
                        contains(self.tab_areas[category.index()], mouse.column, mouse.row)
                    })
                {
                    self.home_focus = HomeFocus::Menu;
                    if self.category != category {
                        self.category = category;
                        self.status = None;
                    }
                } else if matches!(self.screen, Screen::Home)
                    && self.category == Category::Project
                    && contains(self.project_area, mouse.column, mouse.row)
                {
                    let index = usize::from(mouse.row.saturating_sub(self.project_area.y));
                    if let Some(section) = ProjectSection::ALL.get(index) {
                        self.home_focus = HomeFocus::Content;
                        self.project_section = *section;
                        self.project_scroll = 0;
                    }
                } else if matches!(self.screen, Screen::Home)
                    && self.category == Category::Taxonomies
                    && let Some(library) =
                        self.global.as_ref().and_then(ConfigurationSession::library)
                    && self.taxonomies.click(mouse.column, mouse.row, library)
                {
                    self.home_focus = HomeFocus::Content;
                }
            }
            return false;
        }
        let Event::Key(key) = event else {
            return false;
        };
        if self.too_small && !matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
            return false;
        }
        match self.screen.clone() {
            Screen::Home => self.home_key(key, workspace),
            Screen::Settings(destination) => self.settings_key(key, destination),
            Screen::Review(destination) => self.review_key(key, destination, workspace),
            Screen::ConfirmRemove(_) => {
                // The node is gone, so there is nothing left to confirm.
                self.screen = Screen::Home;
                false
            }
            Screen::Leave | Screen::ScopeChange(_) => false,
        }
    }

    fn handle_home_overlay(
        &mut self,
        event: Event,
        workspace: &dyn ConfigurationWorkspace,
    ) -> bool {
        let Some(overlay) = self.home_overlay.take() else {
            return false;
        };
        self.handle_palette_event(event, workspace, overlay)
    }

    fn handle_palette_event(
        &mut self,
        event: Event,
        workspace: &dyn ConfigurationWorkspace,
        mut palette: CommandPaletteState,
    ) -> bool {
        match event {
            Event::Key(key) => match key.code {
                KeyCode::Esc => return false,
                KeyCode::Up | KeyCode::Down => {
                    palette.move_selection(self.category, key.code == KeyCode::Down);
                }
                KeyCode::Backspace => {
                    let mut query = palette.query.clone();
                    query.pop();
                    palette.set_query(query);
                }
                KeyCode::Enter => {
                    if let Some(command) = palette.selected_command(self.category) {
                        self.status = None;
                        return self.dispatch_home_command(command, workspace);
                    }
                }
                KeyCode::Char(character)
                    if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
                {
                    let mut query = palette.query.clone();
                    query.push(character);
                    palette.set_query(query);
                }
                _ => {}
            },
            Event::Paste(text) => {
                let mut query = palette.query.clone();
                query.extend(text.chars().filter(|character| !character.is_control()));
                palette.set_query(query);
            }
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                if contains(palette.list_area, mouse.column, mouse.row) {
                    let index = palette.list_offset
                        + usize::from(mouse.row.saturating_sub(palette.list_area.y));
                    let count = palette.visible(self.category).len();
                    if index < count {
                        palette.selected = index;
                    }
                } else {
                    return false;
                }
            }
            _ => {}
        }
        self.home_overlay = Some(palette);
        false
    }

    fn dispatch_home_command(
        &mut self,
        command: HomeCommand,
        workspace: &dyn ConfigurationWorkspace,
    ) -> bool {
        match command {
            HomeCommand::Configure => self.request_project_action(PendingAction::EditProject),
            HomeCommand::Initialize => {
                self.request_project_action(PendingAction::InitializeProject);
            }
            HomeCommand::ReviewProject => {
                self.request_project_action(PendingAction::ReviewProject);
            }
            HomeCommand::New => {
                self.request_global_action(PendingAction::Library(LibraryAction::Create));
            }
            HomeCommand::Edit => {
                self.request_global_action(PendingAction::Library(LibraryAction::Edit));
            }
            HomeCommand::Fork => {
                self.request_global_action(PendingAction::Library(LibraryAction::Fork));
            }
            HomeCommand::Delete => self.request_global_action(PendingAction::Delete),
            HomeCommand::SwitchTab => self.category = self.category.next(),
            HomeCommand::Quit => {
                // The shell confirms every pending draft once, including a retained commit review.
                if self.embedded {
                    return true;
                }
                return self.home_key(
                    KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
                    workspace,
                );
            }
        }
        false
    }

    fn home_key(&mut self, key: KeyEvent, _workspace: &dyn ConfigurationWorkspace) -> bool {
        if self.category == Category::Taxonomies && self.taxonomies.editing() {
            if let Some(library) = self
                .global
                .as_mut()
                .and_then(ConfigurationSession::library_mut)
                && let Err(error) = self.taxonomies.edit_key(key, library)
            {
                self.status = Some(Notice::error(error));
            }
            return false;
        }
        if self.category == Category::Taxonomies
            && self.home_focus == HomeFocus::Content
            && let Some(library) = self.global.as_ref().and_then(ConfigurationSession::library)
        {
            match self.taxonomies.key(key, library) {
                Nav::Handled => return false,
                Nav::Sidebar => {
                    self.home_focus = HomeFocus::Menu;
                    return false;
                }
                Nav::Act(action) => {
                    self.request_global_action(PendingAction::Library(action));
                    return false;
                }
                Nav::Unhandled => {}
            }
        }
        match key.code {
            KeyCode::Char('/') if key.modifiers.is_empty() => {
                self.home_overlay = Some(CommandPaletteState::default());
            }
            KeyCode::Char(digit @ ('1' | '2')) if key.modifiers.is_empty() => {
                let category = if digit == '1' {
                    Category::Project
                } else {
                    Category::Taxonomies
                };
                if self.category != category {
                    self.category = category;
                    self.status = None;
                }
                self.home_focus = HomeFocus::Content;
            }
            KeyCode::Up | KeyCode::Down if self.home_focus == HomeFocus::Menu => {
                let category = if key.code == KeyCode::Up {
                    Category::Project
                } else {
                    Category::Taxonomies
                };
                if self.category != category {
                    self.category = category;
                    self.status = None;
                }
            }
            KeyCode::Right if self.home_focus == HomeFocus::Menu => {
                self.home_focus = HomeFocus::Content;
            }
            KeyCode::Left
                if self.home_focus == HomeFocus::Menu || self.category == Category::Project =>
            {
                self.home_focus = HomeFocus::Menu;
            }
            KeyCode::Up | KeyCode::Down if self.category == Category::Project => {
                let index = self.project_section.index();
                let index = match key.code {
                    KeyCode::Up => index.saturating_sub(1),
                    _ => (index + 1).min(ProjectSection::ALL.len() - 1),
                };
                self.project_section = ProjectSection::ALL[index];
                self.project_scroll = 0;
            }
            KeyCode::PageUp if self.category == Category::Project => {
                self.project_scroll = self.project_scroll.saturating_sub(8);
            }
            KeyCode::PageDown if self.category == Category::Project => {
                self.project_scroll = self.project_scroll.saturating_add(8);
            }
            KeyCode::Enter if self.category == Category::Project => {
                if self.project_section == ProjectSection::Commit {
                    self.request_project_action(PendingAction::EditProject);
                }
            }
            KeyCode::Char('i') if self.category == Category::Project => {
                self.request_project_action(PendingAction::InitializeProject);
            }
            KeyCode::Char(letter @ ('e' | 'f' | 'n'))
                if self.category == Category::Taxonomies && key.modifiers.is_empty() =>
            {
                self.request_global_action(PendingAction::Library(match letter {
                    'e' => LibraryAction::Edit,
                    'f' => LibraryAction::Fork,
                    _ => LibraryAction::Create,
                }));
            }
            KeyCode::Char('d')
                if self.category == Category::Taxonomies && key.modifiers.is_empty() =>
            {
                self.request_global_action(PendingAction::Delete);
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.category == Category::Project {
                    self.request_project_action(PendingAction::ReviewProject);
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                if self.any_dirty() {
                    self.screen = Screen::Leave;
                } else {
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    fn request_project_action(&mut self, action: PendingAction) {
        self.status = None;
        let Some(session) = &self.project else {
            self.status = Some(Notice::error(self.project_error.clone()));
            return;
        };
        match action {
            PendingAction::EditProject if session.project_config().is_none() => {
                self.status = Some(Notice::error(
                    "Initialize the project before editing.".into(),
                ));
                return;
            }
            PendingAction::InitializeProject if session.project_config().is_some() => {
                self.status = Some(Notice::info("Project is already initialized.".into()));
                return;
            }
            _ => {}
        }
        self.perform_pending_action(action);
    }

    fn open_settings(&mut self, destination: ConfigurationDestination) {
        let Some(session) = self.session(destination) else {
            return;
        };
        let Ok(catalog) = session.catalog() else {
            return;
        };
        let (default, available, inherit_default, inherit_available) = match destination {
            ConfigurationDestination::Global => {
                let Some(library) = session.library() else {
                    return;
                };
                (
                    library.default_taxonomy().clone(),
                    library.available_taxonomies().to_vec(),
                    false,
                    false,
                )
            }
            ConfigurationDestination::Project => {
                let (Some(config), Some(global)) = (session.project_config(), session.global())
                else {
                    return;
                };
                (
                    config
                        .default_taxonomy()
                        .unwrap_or_else(|| global.default_taxonomy())
                        .clone(),
                    config
                        .available_taxonomies()
                        .unwrap_or_else(|| global.available_taxonomies())
                        .to_vec(),
                    config.default_taxonomy().is_none(),
                    config.available_taxonomies().is_none(),
                )
            }
        };
        let selected = catalog
            .taxonomies()
            .iter()
            .position(|taxonomy| taxonomy.id() == &default)
            .unwrap_or(0);
        self.settings = Some(SettingsDraft {
            selected,
            default,
            available,
            inherit_default,
            inherit_available,
        });
        self.screen = Screen::Settings(destination);
    }

    fn settings_key(&mut self, key: KeyEvent, destination: ConfigurationDestination) -> bool {
        let count = self.catalog().map_or(0, |items| items.len());
        let selected_id = self.catalog().and_then(|items| {
            items
                .get(self.settings.as_ref()?.selected)
                .map(|item| item.id().clone())
        });
        let Some(settings) = &mut self.settings else {
            return false;
        };
        match key.code {
            KeyCode::Up if key.modifiers.contains(KeyModifiers::ALT) => {
                reorder_available(&mut settings.available, &selected_id, false)
            }
            KeyCode::Down if key.modifiers.contains(KeyModifiers::ALT) => {
                reorder_available(&mut settings.available, &selected_id, true)
            }
            KeyCode::Up => settings.selected = settings.selected.saturating_sub(1),
            KeyCode::Down => {
                settings.selected = (settings.selected + 1).min(count.saturating_sub(1))
            }
            KeyCode::Char(' ') => {
                if let Some(id) = selected_id {
                    settings.inherit_available = false;
                    if id == settings.default {
                        self.status = Some(Notice::error(
                            "Choose another default before removing this taxonomy.".into(),
                        ));
                    } else if let Some(index) =
                        settings.available.iter().position(|item| item == &id)
                    {
                        settings.available.remove(index);
                    } else {
                        settings.available.push(id);
                    }
                }
            }
            KeyCode::Enter => {
                if let Some(id) = selected_id {
                    settings.inherit_default = false;
                    settings.default = id.clone();
                    if !settings.available.contains(&id) {
                        settings.inherit_available = false;
                        settings.available.push(id);
                    }
                }
            }
            KeyCode::Char('d') if destination == ConfigurationDestination::Project => {
                settings.inherit_default = true;
                if let Some(global) = self.project.as_ref().and_then(ConfigurationSession::global) {
                    settings.default = global.default_taxonomy().clone();
                }
            }
            KeyCode::Char('a') if destination == ConfigurationDestination::Project => {
                settings.inherit_available = true;
                if let Some(global) = self.project.as_ref().and_then(ConfigurationSession::global) {
                    settings.available = global.available_taxonomies().to_vec();
                }
            }
            KeyCode::Char('r') if destination == ConfigurationDestination::Project => {
                settings.inherit_default = true;
                settings.inherit_available = true;
                if let Some(global) = self.project.as_ref().and_then(ConfigurationSession::global) {
                    settings.default = global.default_taxonomy().clone();
                    settings.available = global.available_taxonomies().to_vec();
                }
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let result = match destination {
                    ConfigurationDestination::Global => self
                        .global
                        .as_mut()
                        .ok_or("global configuration is unavailable".to_owned())
                        .and_then(|session| {
                            session.configure_global(
                                settings.default.clone(),
                                settings.available.clone(),
                            )
                        }),
                    ConfigurationDestination::Project => self
                        .project
                        .as_mut()
                        .ok_or("project configuration is unavailable".to_owned())
                        .and_then(|session| {
                            session.configure_project(
                                (!settings.inherit_default).then(|| settings.default.clone()),
                                (!settings.inherit_available).then(|| settings.available.clone()),
                            )
                        }),
                };
                match result {
                    Ok(()) => {
                        self.settings = None;
                        self.screen = Screen::Home;
                        self.status = Some(Notice::info(
                            "Configuration change staged. Ctrl+S reviews before applying.".into(),
                        ));
                    }
                    Err(error) => self.status = Some(Notice::error(error)),
                }
            }
            KeyCode::Esc => {
                self.settings = None;
                self.screen = Screen::Home;
            }
            _ => {}
        }
        false
    }

    /// Removes `node` after its dialog confirmed it and lands on its neighbor.
    fn remove(&mut self, node: &NodeId) {
        self.screen = Screen::Home;
        let Some(library) = self
            .global
            .as_mut()
            .and_then(ConfigurationSession::library_mut)
        else {
            return;
        };
        match library.edit(node, DraftEdit::Remove) {
            Ok(landing) => {
                self.taxonomies.select(landing);
            }
            Err(refusal) => {
                self.status = Some(Notice::error(naming::refusal_sentence(refusal, None)));
            }
        }
    }

    fn apply_scope_then(&mut self, action: PendingAction, workspace: &dyn ConfigurationWorkspace) {
        let destination = self.active_scope;
        if self.refuse_apply(destination) {
            self.screen = Screen::Home;
            return;
        }
        let Some(session) = self.session(destination).cloned() else {
            return;
        };
        match workspace.save(&session) {
            Ok(saved) => {
                self.replace_session(destination, Ok(saved));
                self.screen = Screen::Home;
                self.perform_pending_action(action);
            }
            Err(error) => {
                self.screen = Screen::Home;
                self.status = Some(Notice::error(error));
            }
        }
    }

    fn discard_scope_then(
        &mut self,
        action: PendingAction,
        workspace: &dyn ConfigurationWorkspace,
    ) {
        self.replace_session(self.active_scope, workspace.load(self.active_scope));
        self.screen = Screen::Home;
        self.perform_pending_action(action);
    }

    /// Replaces a reviewed or refreshed Project session.
    fn replace_session(
        &mut self,
        destination: ConfigurationDestination,
        result: Result<ConfigurationSession, String>,
    ) {
        debug_assert_eq!(destination, ConfigurationDestination::Project);
        let (session, error) = split_result(result);
        self.project = session;
        self.project_error = error;
    }

    fn request_global_action(&mut self, action: PendingAction) {
        if self.global.is_none() {
            self.status = Some(Notice::error(self.library_error()));
            return;
        }
        if matches!(
            action,
            PendingAction::Library(LibraryAction::Edit | LibraryAction::Copy)
                | PendingAction::Delete
        ) && self
            .library()
            .and_then(|library| self.taxonomies.selected(library))
            .is_some_and(|node| node.taxonomy().origin() == TaxonomyOrigin::BuiltIn)
        {
            self.status = Some(Notice::error(naming::refusal_sentence(
                EditRefusal::ReadOnly,
                None,
            )));
            return;
        }
        if matches!(action, PendingAction::Delete)
            && let Some(blocker) = self.library().and_then(|library| {
                library.removal_blocker(self.taxonomies.selected(library)?.id())
            })
        {
            self.status = Some(Notice::error(blocker.to_string()));
            return;
        }
        self.status = None;
        if self.active_scope == ConfigurationDestination::Project
            && self.scope_dirty(ConfigurationDestination::Project)
        {
            self.screen = Screen::ScopeChange(action);
            return;
        }
        self.perform_pending_action(action);
    }

    fn perform_pending_action(&mut self, action: PendingAction) {
        self.screen = Screen::Home;
        match action {
            PendingAction::EditProject => {
                self.active_scope = ConfigurationDestination::Project;
                self.open_settings(ConfigurationDestination::Project);
            }
            PendingAction::InitializeProject => {
                self.active_scope = ConfigurationDestination::Project;
                let Some(session) = &mut self.project else {
                    self.status = Some(Notice::error(self.project_error.clone()));
                    return;
                };
                match session
                    .initialize_project()
                    .and_then(|()| session.review().map(|_| ()))
                {
                    Ok(()) => self.screen = Screen::Review(ConfigurationDestination::Project),
                    Err(error) => self.status = Some(Notice::error(error)),
                }
            }
            PendingAction::ReviewProject => {
                self.active_scope = ConfigurationDestination::Project;
                self.open_review();
            }
            PendingAction::Library(action) => {
                self.active_scope = ConfigurationDestination::Global;
                self.category = Category::Taxonomies;
                self.home_focus = HomeFocus::Content;
                if let Some(library) = self
                    .global
                    .as_mut()
                    .and_then(ConfigurationSession::library_mut)
                    && let Err(error) = self.taxonomies.act(action, library)
                {
                    self.status = Some(Notice::error(error));
                }
            }
            PendingAction::Delete => {
                self.active_scope = ConfigurationDestination::Global;
                if let Some(node) = self
                    .library()
                    .and_then(|library| self.taxonomies.selected(library))
                {
                    self.screen = Screen::ConfirmRemove(node.id().clone());
                }
            }
        }
    }

    fn session(&self, destination: ConfigurationDestination) -> Option<&ConfigurationSession> {
        match destination {
            ConfigurationDestination::Global => self.global.as_ref(),
            ConfigurationDestination::Project => self.project.as_ref(),
        }
    }

    fn catalog(&self) -> Option<Vec<Taxonomy>> {
        self.global
            .as_ref()?
            .catalog()
            .ok()
            .map(|catalog| catalog.taxonomies().to_vec())
    }

    fn library_error(&self) -> String {
        if self.global_error.is_empty() {
            "The Library is unavailable.".into()
        } else {
            self.global_error.clone()
        }
    }

    /// The working Library of the Global session.
    fn library(&self) -> Option<&LibraryDraft> {
        self.global.as_ref().and_then(ConfigurationSession::library)
    }

    fn scope_dirty(&self, destination: ConfigurationDestination) -> bool {
        self.session(destination)
            .is_some_and(ConfigurationSession::is_dirty)
    }

    fn any_dirty(&self) -> bool {
        self.scope_dirty(ConfigurationDestination::Project)
    }

    fn render(&mut self, frame: &mut Frame<'_>) {
        self.render_at(frame, frame.area());
    }

    fn render_at(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.dialog.invalidate();
        frame.render_widget(
            Block::default().style(Style::default().bg(JET_BLACK).fg(Color::White)),
            area,
        );
        let terminal = frame.area();
        self.too_small = terminal.width < MINIMUM_WIDTH || terminal.height < MINIMUM_HEIGHT;
        if !self.animation_visible() {
            self.taxonomies.reset_animation();
        }
        if self.too_small {
            self.tab_areas = [Rect::default(); 2];
            if let Some(question) = self.confirmation() {
                self.dialog.render(frame, area, &question);
            } else {
                frame.render_widget(
                    Paragraph::new("Terminal too small\n\nResize or press esc/q to cancel")
                        .centered(),
                    centered_rect(54, 3, area),
                );
            }
            normalize_background(frame, area);
            return;
        }
        let [main, footer] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(FOOTER_HEIGHT)]).areas(area);
        let footer = render_footer_block(frame, footer);
        let [menu, content] =
            Layout::horizontal([Constraint::Length(MENU_WIDTH), Constraint::Min(1)]).areas(main);
        self.render_menu(frame, menu);
        let body = content;
        let hints = if let Some(question) = self.confirmation() {
            if self.category == Category::Project {
                self.render_project(frame, body);
            } else {
                self.render_taxonomies(frame, body);
            }
            self.dialog.render(frame, body, &question);
            confirmation::hints(question.choices)
        } else {
            match self.screen.clone() {
                Screen::Home if self.category == Category::Taxonomies => {
                    self.render_taxonomies(frame, body);
                    let line =
                        self.taxonomies
                            .status_line(self.library(), self.embedded, footer.width);
                    frame.render_widget(Paragraph::new(line), footer);
                    Vec::new()
                }
                Screen::Home => {
                    self.render_project(frame, body);
                    let mut hints = vec![("←/→", "focus"), ("↑/↓", "move"), ("/", "commands")];
                    if self.embedded {
                        hints.insert(0, ("tab/shift+tab", "switch"));
                    }
                    hints
                }
                Screen::Settings(destination) => {
                    self.render_settings(frame, body, destination);
                    vec![
                        ("space", "available"),
                        ("enter", "default"),
                        ("alt+↑/↓", "reorder"),
                        ("d/a/r", "inherit"),
                        ("ctrl+s", "stage"),
                        ("esc", "back"),
                    ]
                }
                Screen::Review(destination) => self.render_review(frame, body, destination),
                Screen::Leave | Screen::ScopeChange(_) | Screen::ConfirmRemove(_) => Vec::new(),
            }
        };
        // The Taxonomies outline draws its own footer and leaves no hints.
        if !hints.is_empty() {
            render_navigation_row(frame, footer, &hints);
        }
        if let Some(overlay) = &mut self.home_overlay {
            overlay.render(frame, main, self.category);
        } else {
            self.render_notice(frame, body);
        }
        normalize_background(frame, area);
    }

    fn render_menu(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.tab_areas = [Rect::default(); 2];
        let home = matches!(self.screen, Screen::Home);
        let style = focus_frame_style(home && self.home_focus == HomeFocus::Menu);
        frame.render_widget(
            Block::bordered()
                .border_style(style)
                .title_style(style)
                .title(block_title("Menu")),
            area,
        );
        for category in Category::ALL {
            let y = area
                .y
                .saturating_add(1)
                .saturating_add(u16::try_from(category.index()).unwrap_or(u16::MAX));
            if y >= area.bottom().saturating_sub(1) {
                break;
            }
            let row = Rect::new(area.x.saturating_add(1), y, area.width.saturating_sub(2), 1);
            let (digit, label) = if category == self.category {
                (
                    navigation_key_style(),
                    Style::default()
                        .fg(ACCENT)
                        .add_modifier(Modifier::UNDERLINED),
                )
            } else {
                (Style::default().fg(Color::Gray), Style::default())
            };
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(format!(" {} ", category.index() + 1), digit),
                    Span::raw(" "),
                    Span::styled(category.label(), label),
                ])),
                row,
            );
            if home {
                self.tab_areas[category.index()] = row;
            }
        }
    }

    fn render_notice(&self, frame: &mut Frame<'_>, area: Rect) {
        let load_error = if matches!(self.screen, Screen::Home) {
            match self.category {
                Category::Project => self.project.as_ref().map_or_else(
                    || Some(self.project_error.clone()),
                    ConfigurationSession::source_error,
                ),
                Category::Taxonomies => self.global.is_none().then(|| self.library_error()),
            }
        } else {
            None
        };
        let (message, style) = match (&self.status, load_error) {
            (Some(Notice::Error(message)), _) => (message.clone(), Style::default().fg(Color::Red)),
            (Some(Notice::Info(message)), _) => {
                (message.clone(), Style::default().fg(Color::White))
            }
            (None, Some(message)) => (sentence(&message), Style::default().fg(Color::Red)),
            (None, None) => return,
        };
        let width = area.width.min(56);
        let lines = Paragraph::new(message.as_str()).wrap(Wrap { trim: false });
        let height = u16::try_from(lines.line_count(width.saturating_sub(2)))
            .unwrap_or(u16::MAX)
            .saturating_add(2)
            .min(area.height);
        let popup = Rect::new(
            area.right().saturating_sub(width),
            area.bottom().saturating_sub(height),
            width,
            height,
        );
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Block::bordered()
                .border_style(frame_style())
                .style(Style::default().bg(JET_BLACK)),
            popup,
        );
        frame.render_widget(
            lines.style(style),
            Rect::new(
                popup.x.saturating_add(1),
                popup.y.saturating_add(1),
                popup.width.saturating_sub(2),
                popup.height.saturating_sub(2),
            ),
        );
    }

    fn render_project(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let columns = pane_columns(area);
        let focused = self.home_focus == HomeFocus::Content;
        let style = focus_frame_style(focused);
        frame.render_widget(
            Block::bordered()
                .border_style(style)
                .title_style(style)
                .title(block_title("Project")),
            columns[0],
        );
        let root = self
            .project
            .as_ref()
            .and_then(ConfigurationSession::root)
            .map(RepositoryRoot::as_path);
        let name = root.and_then(|path| path.file_name()).map_or_else(
            || "Project".into(),
            |name| name.to_string_lossy().into_owned(),
        );
        let path = root.map_or_else(
            || "Repository unavailable".into(),
            |path| path.display().to_string(),
        );
        frame.render_widget(
            Paragraph::new(name).style(section_heading_style()),
            Rect::new(
                columns[0].x + 1,
                columns[0].y + 1,
                columns[0].width.saturating_sub(2),
                1,
            ),
        );
        frame.render_widget(
            Paragraph::new(path),
            Rect::new(
                columns[0].x + 1,
                columns[0].y + 2,
                columns[0].width.saturating_sub(2),
                1,
            ),
        );
        frame.render_widget(
            Paragraph::new("Configuration"),
            Rect::new(
                columns[0].x + 1,
                columns[0].y + 4,
                columns[0].width.saturating_sub(2),
                1,
            ),
        );
        let nav = Rect::new(
            columns[0].x + 1,
            columns[0].y + 5,
            columns[0].width.saturating_sub(2),
            columns[0].height.saturating_sub(6),
        );
        self.project_area = nav;
        let rows = ProjectSection::ALL
            .into_iter()
            .map(|section| {
                ListItem::new(format!(
                    "{}{}",
                    if section == self.project_section {
                        "› "
                    } else {
                        "  "
                    },
                    section.label()
                ))
            })
            .collect::<Vec<_>>();
        let mut state = ListState::default();
        state.select(Some(self.project_section.index()));
        frame.render_stateful_widget(
            List::new(rows).highlight_style(selected_style(focused)),
            nav,
            &mut state,
        );

        frame.render_widget(
            Block::bordered()
                .border_style(frame_style())
                .title(block_title(self.project_section.label())),
            columns[1],
        );
        let detail = Rect::new(
            columns[1].x + 1,
            columns[1].y + 1,
            columns[1].width.saturating_sub(2),
            columns[1].height.saturating_sub(2),
        );
        let text = self.project_detail();
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
        let total_rows = u16::try_from(paragraph.line_count(detail.width)).unwrap_or(u16::MAX);
        let max_scroll = total_rows.saturating_sub(detail.height);
        self.project_scroll = self.project_scroll.min(max_scroll);
        frame.render_widget(paragraph.scroll((self.project_scroll, 0)), detail);
    }

    fn project_detail(&self) -> String {
        let Some(session) = &self.project else {
            return self.project_error.clone();
        };
        let inherited = ProjectConfig::inherited();
        let config = session.project_config().unwrap_or(&inherited);
        let Some(baseline) = session.global() else {
            return self.project_error.clone();
        };
        let effective = resolve_effective_configuration(baseline, config);
        let available = effective.as_ref().map_or_else(
            |error| format!("Unresolved: {error}"),
            |value| {
                value
                    .taxonomies()
                    .iter()
                    .map(|taxonomy| taxonomy.id().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            },
        );
        let default = effective.as_ref().map_or_else(
            |error| format!("Unresolved: {error}"),
            |value| value.default_taxonomy().to_string(),
        );
        let pinned = session
            .persisted_project_config()
            .and_then(|config| session.lock().filter(|lock| lock.matches(config)));
        let commit_default = pinned.map_or_else(
            || format!("{default} (preview; commit blocked)"),
            |lock| lock.default_taxonomy().to_string(),
        );
        let commit_available = pinned.map_or_else(
            || format!("{available} (preview; commit blocked)"),
            |lock| {
                lock.taxonomies()
                    .iter()
                    .map(|taxonomy| taxonomy.id().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            },
        );
        let status = project_status(session);
        match self.project_section {
            ProjectSection::Commit => {
                let persisted = session.persisted_project_config().unwrap_or(&inherited);
                let staged = if session.is_dirty() {
                    format!("\n\nStaged resolution\n  Default: {default}\n  Available: {available}")
                } else {
                    String::new()
                };
                format!(
                    "Commit taxonomy\n  {commit_default}\n\nSource\n  {}\n\nLibrary baseline\n  {}\n\nAvailable taxonomies\n  {commit_available}{staged}\n\nStatus\n  {status}\n  {}",
                    if persisted.default_taxonomy().is_some() {
                        "Project override"
                    } else {
                        "Inherited Library baseline"
                    },
                    baseline.default_taxonomy(),
                    project_guidance(status),
                )
            }
            ProjectSection::Resolution => {
                let library_path = self.library_path.as_ref().map_or_else(
                    || "Library config location unavailable".into(),
                    |path| path.display().to_string(),
                );
                let project_path = session.root().map_or_else(String::new, |root| {
                    root.as_path().join("gitserious.toml").display().to_string()
                });
                let project_default = config
                    .default_taxonomy()
                    .map_or_else(|| "inherit".into(), ToString::to_string);
                let project_available = config.available_taxonomies().map_or_else(
                    || "inherit".into(),
                    |values| {
                        values
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    },
                );
                let pinned_default = session.lock().map_or_else(
                    || "Unavailable".to_owned(),
                    |lock| lock.default_taxonomy().to_string(),
                );
                let pinned_available = session.lock().map_or_else(
                    || "Unavailable".to_owned(),
                    |lock| {
                        lock.taxonomies()
                            .iter()
                            .map(|taxonomy| taxonomy.id().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    },
                );
                format!(
                    "Library baseline\n  {library_path}\n  Default: {}\n  Available: {}\n\n{}\n  {project_path}\n  Default: {project_default}\n  Available: {project_available}\n\nCurrent resolution\n  Default: {default}\n  Available: {available}\n\nPinned lock\n  Default: {pinned_default}\n  Available: {pinned_available}\n\nStatus\n  {status}",
                    baseline.default_taxonomy(),
                    baseline
                        .available_taxonomies()
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                    if session.is_dirty() {
                        "Project overrides (staged)"
                    } else {
                        "Project overrides"
                    },
                )
            }
            ProjectSection::Files => {
                let Some(root) = session.root() else {
                    return "Repository unavailable.".into();
                };
                format!(
                    "Project configuration\n  {}\n  {}\n\nLock\n  {}\n  {}\n\nStatus\n  {status}\n  {}",
                    root.as_path().join("gitserious.toml").display(),
                    if session.persisted_project_config().is_some() {
                        "Present"
                    } else {
                        "Absent"
                    },
                    root.as_path().join("gitserious.lock").display(),
                    if session.lock().is_some() {
                        "Present"
                    } else {
                        "Absent"
                    },
                    project_guidance(status),
                )
            }
        }
    }

    fn render_taxonomies(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let focused = matches!(self.screen, Screen::Home) && self.home_focus == HomeFocus::Content;
        let animate = self.animation_visible();
        let library = self.global.as_ref().and_then(ConfigurationSession::library);
        self.taxonomies
            .render(frame, area, library, focused, animate);
    }

    fn render_settings(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        destination: ConfigurationDestination,
    ) {
        let Some(settings) = &self.settings else {
            return;
        };
        let items = self.catalog().unwrap_or_default();
        let rows = items
            .iter()
            .enumerate()
            .map(|(index, taxonomy)| {
                let available = if settings.available.contains(taxonomy.id()) {
                    "✓"
                } else {
                    "○"
                };
                let default = if &settings.default == taxonomy.id() {
                    "default"
                } else {
                    ""
                };
                let order = settings
                    .available
                    .iter()
                    .position(|id| id == taxonomy.id())
                    .map_or_else(|| "-".to_owned(), |position| (position + 1).to_string());
                ListItem::new(format!(
                    "{available}  {order:>2}  {:<20} {default}",
                    taxonomy.id()
                ))
                .style(row_style(index))
            })
            .collect::<Vec<_>>();
        let mut state = ListState::default();
        state.select(Some(settings.selected));
        let inheritance = if destination == ConfigurationDestination::Project {
            format!(
                "default: {} | available: {} | r: reset both",
                if settings.inherit_default {
                    "inherited"
                } else {
                    "override"
                },
                if settings.inherit_available {
                    "inherited"
                } else {
                    "override"
                }
            )
        } else {
            "global commit environment".into()
        };
        let parts = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).split(area);
        frame.render_widget(
            Paragraph::new(inheritance).style(section_heading_style()),
            parts[0],
        );
        frame.render_stateful_widget(
            List::new(rows)
                .block(
                    Block::bordered()
                        .border_style(frame_style())
                        .title(block_title(match destination {
                            ConfigurationDestination::Global => "Global availability",
                            ConfigurationDestination::Project => "Project availability",
                        })),
                )
                .highlight_style(navigation_key_style()),
            parts[1],
            &mut state,
        );
    }
}

/// Wraps each line of `text` at spaces to `width` columns. Wrapped rows start under the line's
/// subject: past its leading spaces and, when a gap of two or more spaces follows its first word,
/// past that gap too. A word wider than its column is left for the paragraph to break.
fn hanging_wrap(text: &str, width: usize) -> Vec<Line<'static>> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let body = line.trim_start();
        let mut hang = line.len() - body.len();
        if let Some(word) = body.find(' ')
            && body[word..].starts_with("  ")
        {
            hang += body.len() - body[word..].trim_start().len();
        }
        let (subject, rest) = line.split_at(hang);
        let indent = " ".repeat(Span::raw(subject).width());
        let column = width.saturating_sub(indent.len());
        let mut row = subject.to_owned();
        let mut used = 0;
        for word in rest.split(' ') {
            let word_width = Span::raw(word).width();
            if used > 0 && used + 1 + word_width > column {
                rows.push(Line::from(std::mem::replace(&mut row, indent.clone())));
                used = 0;
            } else if used > 0 {
                row.push(' ');
                used += 1;
            }
            row.push_str(word);
            used += word_width;
        }
        rows.push(Line::from(row));
    }
    rows
}

fn pane_columns(area: Rect) -> std::rc::Rc<[Rect]> {
    let left = (area.width.saturating_mul(38) / 100)
        .max(26)
        .min(area.width.saturating_sub(28));
    Layout::horizontal([Constraint::Length(left), Constraint::Min(1)]).split(area)
}

fn project_status(session: &ConfigurationSession) -> &'static str {
    let Some(config) = session.persisted_project_config() else {
        return if session.project_config().is_some() {
            "Initialization staged"
        } else {
            "Not initialized"
        };
    };
    let Some(lock) = session.lock() else {
        return "Lock missing";
    };
    if !lock.matches(config) {
        "Stale lock"
    } else if session.is_dirty() {
        "Changes staged"
    } else if session.source_error().is_some() {
        "Source unavailable"
    } else if session.update_available() {
        "Update available"
    } else {
        "In sync"
    }
}

fn project_guidance(status: &str) -> &'static str {
    match status {
        "Not initialized" => "Press i to initialize and review.",
        "Initialization staged" => "Review and apply initialization with Ctrl+S.",
        "Changes staged" => "Review Ctrl+S; commits still use the pinned lock.",
        "Lock missing" => "Run gitserious init to create the missing lock.",
        "Stale lock" | "Update available" => "Review and refresh the lock with Ctrl+S.",
        "Source unavailable" => {
            "Pinned commits use the lock; restore Library source before refresh."
        }
        _ => "",
    }
}

fn split_result(
    result: Result<ConfigurationSession, String>,
) -> (Option<ConfigurationSession>, String) {
    match result {
        Ok(value) => (Some(value), String::new()),
        Err(error) => (None, error),
    }
}

fn contains(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x && x < area.right() && y >= area.y && y < area.bottom()
}

fn reorder_available(available: &mut [TaxonomyId], selected: &Option<TaxonomyId>, down: bool) {
    let Some(selected) = selected else {
        return;
    };
    let Some(index) = available.iter().position(|id| id == selected) else {
        return;
    };
    let target = if down {
        (index + 1).min(available.len().saturating_sub(1))
    } else {
        index.saturating_sub(1)
    };
    available.swap(index, target);
}

#[cfg(test)]
mod tests {
    use crate::library_lookup::LibraryLookup;
    use std::cell::{Cell, RefCell};

    use gitserious_app::{
        DraftField, GlobalConfiguration, ProjectLock, ProjectState, RepositoryRoot,
    };
    use gitserious_core::{
        CommitTypeDefinition, CommitTypeId, Description, PropertyDefinition, PropertyKey,
        PropertyRequirement, SchemaVersion, TaxonomyVersion,
    };
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::naming::problem_sentence;
    use super::*;

    pub(super) struct Workspace {
        pub(super) global: RefCell<ConfigurationSession>,
        pub(super) project: RefCell<ConfigurationSession>,
        saves: Cell<usize>,
    }

    impl Workspace {
        pub(super) fn new() -> Self {
            let global = GlobalConfiguration::default();
            let root = RepositoryRoot::new(std::env::temp_dir()).unwrap_or_else(|_| unreachable!());
            Self {
                global: RefCell::new(ConfigurationSession::open_global(global.clone())),
                project: RefCell::new(
                    ConfigurationSession::open_project(root, global, ProjectState::Absent)
                        .unwrap_or_else(|_| unreachable!()),
                ),
                saves: Cell::new(0),
            }
        }

        pub(super) fn saves(&self) -> usize {
            self.saves.get()
        }
    }

    /// Forks `source` under the name `target`, as `f` and typing a name do.
    fn fork_as(
        session: &mut ConfigurationSession,
        source: &str,
        target: &str,
    ) -> Result<NodeId, Box<dyn std::error::Error>> {
        let library = session.library_mut().ok_or("a Global session")?;
        let source = library.find_path(&[source]).ok_or("missing source")?;
        let fork = library.fork(&source)?;
        library.edit(&fork, DraftEdit::Text(DraftField::Name, target))?;
        Ok(fork)
    }

    /// A Library with the custom taxonomy `original` and one type whose property's description
    /// is longer than its field's limit.
    fn custom_workspace() -> Result<(Workspace, Taxonomy), Box<dyn std::error::Error>> {
        let source = Taxonomy::new(
            TaxonomyId::new("original")?,
            TaxonomyVersion::new(3)?,
            Description::new("Original taxonomy")?,
            None,
            vec![CommitTypeDefinition::new(
                SchemaVersion::new(4)?,
                CommitTypeId::new("existing")?,
                "Existing type",
                vec![PropertyDefinition::new(
                    PropertyKey::new("notes")?,
                    "x".repeat(130),
                    PropertyRequirement::Optional,
                )?],
            )?],
        )?;
        let workspace = Workspace::new();
        *workspace.global.borrow_mut() =
            ConfigurationSession::open_global(GlobalConfiguration::new(
                1,
                TaxonomyId::new("conventional")?,
                vec![TaxonomyId::new("conventional")?],
                vec![source.clone()].into_iter().map(Into::into).collect(),
            )?);
        Ok((workspace, source))
    }

    fn library(state: &State) -> Result<&LibraryDraft, Box<dyn std::error::Error>> {
        Ok(state.library().ok_or("the Library loads")?)
    }

    fn cursor(state: &State) -> Vec<String> {
        state.taxonomies.observe(state.library()).cursor_path
    }

    impl ConfigurationWorkspace for Workspace {
        fn load(
            &self,
            destination: ConfigurationDestination,
        ) -> Result<ConfigurationSession, String> {
            Ok(match destination {
                ConfigurationDestination::Global => self.global.borrow().clone(),
                ConfigurationDestination::Project => self.project.borrow().clone(),
            })
        }

        fn save(&self, session: &ConfigurationSession) -> Result<ConfigurationSession, String> {
            self.saves.set(self.saves.get() + 1);
            match session.root() {
                None => {
                    let library = session.library().ok_or("missing Library")?;
                    if self.global.borrow().library().map(LibraryDraft::persisted)
                        != Some(library.persisted())
                    {
                        return Err("global configuration changed while editing".into());
                    }
                    let mut saved = session.clone();
                    *saved.library_mut().ok_or("missing Library")? = library
                        .saved()
                        .map_err(|problems| format!("{problems:?}"))?;
                    *self.global.borrow_mut() = saved.clone();
                    return Ok(saved);
                }
                Some(_) => *self.project.borrow_mut() = session.clone(),
            }
            Ok(session.clone())
        }
    }

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn ctrl(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::CONTROL))
    }

    fn click(area: Rect) -> Event {
        Event::Mouse(ratatui::crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.x + 1,
            row: area.y,
            modifiers: KeyModifiers::NONE,
        })
    }

    fn select_taxonomy(state: &mut State, index: usize) {
        let id = state.library().map_or_else(
            || unreachable!("the Library loads"),
            |library| library.taxonomies()[index].id().clone(),
        );
        state.taxonomies.select(id);
    }

    fn text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    fn row_text(terminal: &Terminal<TestBackend>, x: u16, y: u16, width: u16) -> String {
        let buffer = terminal.backend().buffer();
        (x..x + width)
            .map(|column| buffer[(column, y)].symbol())
            .collect::<Vec<_>>()
            .concat()
    }

    #[test]
    fn project_shows_inherited_preview_resolution_files_and_initialize()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.library_path = Some(PathBuf::from("/library/config.toml"));
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        let home = text(&terminal);
        assert!(!home.contains("gitserious config"));
        assert!(home.contains("Project"));
        assert!(home.contains("Library"));
        assert!(home.contains("Commit"));
        assert!(home.contains("Resolution"));
        assert!(home.contains("Files"));
        assert!(home.contains("conventional"));
        assert!(home.contains("Not initialized"));
        assert!(
            state
                .project
                .as_ref()
                .ok_or("missing project")?
                .project_config()
                .is_none()
        );
        let buffer = terminal.backend().buffer();
        assert!(row_text(&terminal, 0, 0, 100).starts_with("┌ Menu ─"));
        assert!(row_text(&terminal, MENU_WIDTH, 0, 100 - MENU_WIDTH).starts_with("┌ Project ─"));
        assert!(home.contains("┌ Commit ─"));
        assert_eq!(buffer[(0, 0)].symbol(), "┌");
        assert_eq!(buffer[(MENU_WIDTH - 1, 0)].symbol(), "┐");
        assert_eq!(buffer[(MENU_WIDTH - 1, 27)].symbol(), "┘");
        assert_eq!(buffer[(MENU_WIDTH, 0)].symbol(), "┌");
        assert_eq!(buffer[(99, 0)].symbol(), "┐");
        assert_ne!(buffer[(MENU_WIDTH + 1, 1)].symbol(), " ");
        assert_eq!(buffer[(MENU_WIDTH + 1, 1)].fg, ACCENT);
        let project_columns = pane_columns(Rect::new(MENU_WIDTH, 0, 100 - MENU_WIDTH, 28));
        assert!(
            row_text(
                &terminal,
                project_columns[1].x + 1,
                project_columns[1].y + 1,
                project_columns[1].width - 2,
            )
            .starts_with("Commit taxonomy")
        );
        assert_eq!(buffer[(2, 1)].symbol(), "1");
        assert_eq!(buffer[(2, 1)].bg, ACCENT);
        assert_eq!(buffer[(5, 1)].fg, ACCENT);
        assert!(buffer[(5, 1)].modifier.contains(Modifier::UNDERLINED));
        assert_eq!(buffer[(0, 27)].symbol(), "└");
        assert_eq!(buffer[(1, 28)].symbol(), "←");
        assert_eq!(buffer[(4, 28)].symbol(), ":");
        assert!((1..99).all(|x| buffer[(x, 28)].bg == ACCENT));

        state.handle_event(key(KeyCode::Right), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(state.project_section, ProjectSection::Resolution);
        terminal.draw(|frame| state.render(frame))?;
        let resolution = text(&terminal);
        assert!(resolution.contains("/library/config.toml"));
        assert!(resolution.contains("Project overrides"));
        assert!(state.project_detail().contains("Current resolution"));
        assert!(state.project_detail().contains("Pinned lock"));
        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(state.project_section, ProjectSection::Files);
        terminal.draw(|frame| state.render(frame))?;
        let files = text(&terminal);
        assert!(files.contains("Project configuration"));
        assert!(state.project_detail().contains("gitserious.toml"));
        assert!(state.project_detail().contains("gitserious.lock"));
        assert!(files.contains("Absent"));
        state.handle_event(key(KeyCode::Up), &workspace);
        state.handle_event(key(KeyCode::Up), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(matches!(state.status, Some(Notice::Error(_))));
        state.handle_event(key(KeyCode::Char('i')), &workspace);
        assert!(matches!(
            state.screen,
            Screen::Review(ConfigurationDestination::Project)
        ));
        assert_eq!(workspace.saves.get(), 0);
        state.project_section = ProjectSection::Files;
        assert!(state.project_detail().contains("Initialization staged"));
        assert!(state.project_detail().contains("Absent"));
        Ok(())
    }

    #[test]
    fn project_panels_distinguish_overrides_lock_drift_and_missing_sources()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        let global = GlobalConfiguration::default();
        let root = RepositoryRoot::new(std::env::temp_dir())?;
        let inherited = ProjectConfig::inherited();
        let inherited_lock = ProjectLock::capture(
            &inherited,
            &resolve_effective_configuration(&global, &inherited)?,
        )?;
        let override_config = ProjectConfig::new(1, Some(TaxonomyId::new("research")?), None)?;
        let override_lock = ProjectLock::capture(
            &override_config,
            &resolve_effective_configuration(&global, &override_config)?,
        )?;
        state.project = Some(ConfigurationSession::open_project(
            root.clone(),
            global.clone(),
            ProjectState::Initialized {
                config: override_config.clone(),
                lock: override_lock,
            },
        )?);
        let commit = state.project_detail();
        assert!(commit.contains("research"));
        assert!(commit.contains("Project override"));
        assert!(commit.contains("Library baseline\n  conventional"));
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(
            state.screen,
            Screen::Settings(ConfigurationDestination::Project)
        ));
        state.handle_event(key(KeyCode::Esc), &workspace);
        state.project_section = ProjectSection::Files;
        assert!(state.project_detail().contains("In sync"));

        state
            .project
            .as_mut()
            .ok_or("missing project")?
            .configure_project(Some(TaxonomyId::new("infra-ops")?), None)?;
        state.project_section = ProjectSection::Commit;
        let staged = state.project_detail();
        assert!(staged.contains("Commit taxonomy\n  research"));
        assert!(staged.contains("Staged resolution\n  Default: infra-ops"));
        assert!(staged.contains("Changes staged"));
        assert!(!staged.contains("commit blocked"));
        state.project_section = ProjectSection::Files;
        assert!(state.project_detail().contains("Present"));

        state.project = Some(ConfigurationSession::open_project(
            root.clone(),
            global.clone(),
            ProjectState::ConfigOnly(override_config.clone()),
        )?);
        assert!(state.project_detail().contains("Lock missing"));
        assert!(state.project_detail().contains("gitserious init"));

        state.project = Some(ConfigurationSession::open_project(
            root.clone(),
            global.clone(),
            ProjectState::Initialized {
                config: override_config,
                lock: inherited_lock.clone(),
            },
        )?);
        assert!(state.project_detail().contains("Stale lock"));
        state.project_section = ProjectSection::Commit;
        assert!(state.project_detail().contains("commit blocked"));

        let changed_global = GlobalConfiguration::new(
            1,
            TaxonomyId::new("research")?,
            global.available_taxonomies().to_vec(),
            Vec::new(),
        )?;
        state.project = Some(ConfigurationSession::open_project(
            root.clone(),
            changed_global,
            ProjectState::Initialized {
                config: inherited.clone(),
                lock: inherited_lock.clone(),
            },
        )?);
        assert!(state.project_detail().contains("Update available"));
        state.project_section = ProjectSection::Commit;
        let commit = state.project_detail();
        assert!(commit.contains("Commit taxonomy\n  conventional"));
        assert!(commit.contains("Library baseline\n  research"));
        assert!(commit.contains("Update available"));

        let missing = ProjectConfig::new(1, Some(TaxonomyId::new("missing")?), None)?;
        state.project = Some(ConfigurationSession::open_project(
            root,
            global,
            ProjectState::Initialized {
                config: missing,
                lock: inherited_lock,
            },
        )?);
        state.project_section = ProjectSection::Commit;
        assert!(state.project_detail().contains("Unresolved"));
        assert!(
            state
                .project
                .as_ref()
                .ok_or("missing project")?
                .source_error()
                .is_some()
        );

        let mut authored = ConfigurationSession::open_global(GlobalConfiguration::default());
        fork_as(&mut authored, "conventional", "team")?;
        let authored = authored
            .library()
            .and_then(|library| library.build().ok())
            .ok_or("the fork builds")?;
        let team = TaxonomyId::new("team")?;
        let config = ProjectConfig::new(1, Some(team.clone()), Some(vec![team]))?;
        let lock = ProjectLock::capture(
            &config,
            &resolve_effective_configuration(&authored, &config)?,
        )?;
        state.project = Some(ConfigurationSession::open_project(
            RepositoryRoot::new(std::env::temp_dir())?,
            GlobalConfiguration::default(),
            ProjectState::Initialized { config, lock },
        )?);
        let pinned = state.project_detail();
        assert!(pinned.contains("Commit taxonomy\n  team"));
        assert!(pinned.contains("Source unavailable"));
        assert!(!pinned.contains("commit blocked"));
        Ok(())
    }

    fn rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| row_text(terminal, 0, y, buffer.area.width))
            .collect()
    }

    #[test]
    fn outline_lists_every_taxonomy_and_the_inspector_describes_the_selection()
    -> Result<(), Box<dyn std::error::Error>> {
        for (width, height) in [(87, 40), (MINIMUM_WIDTH, MINIMUM_HEIGHT)] {
            let (workspace, _) = custom_workspace()?;
            let mut state = State::new(&workspace);
            state.handle_event(key(KeyCode::Char('2')), &workspace);
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| state.render(frame))?;
            let screen = rows(&terminal);
            assert!(screen[0].starts_with("┌ Menu ─"));
            assert!(
                row_text(&terminal, MENU_WIDTH, 0, width - MENU_WIDTH)
                    .starts_with("┌ Commit Message Taxonomies ─")
            );
            let buffer = terminal.backend().buffer();
            assert_eq!(buffer[(MENU_WIDTH, 0)].fg, Color::Rgb(80, 160, 255));
            assert_eq!(buffer[(MENU_WIDTH + 1, 1)].fg, Color::Rgb(80, 160, 255));
            assert_eq!(buffer[(1, height - 2)].bg, Color::Rgb(80, 160, 255));
            for label in [" built-in ", " ◇ read-only "] {
                let mut count = 0;
                for (y, line) in screen.iter().enumerate() {
                    for (start, _) in line.match_indices(label) {
                        let x = line[..start].chars().count();
                        for dx in 0..label.chars().count() {
                            let cell = &buffer[(u16::try_from(x + dx)?, u16::try_from(y)?)];
                            assert_eq!(cell.fg, Color::Black);
                            assert_eq!(cell.bg, Color::Rgb(211, 211, 211));
                        }
                        count += 1;
                    }
                }
                assert!(count > 0, "missing {label}");
            }

            assert!(screen[2].contains(" 2  Library"));
            let outline = |name: &str| {
                screen
                    .iter()
                    .find(|row| row.contains(&format!("▸ {name} ")))
                    .cloned()
                    .unwrap_or_default()
            };
            assert!(outline("conventional").contains("│▌▸ conventional"));
            assert!(outline("conventional").trim_end().ends_with("built-in  │"));
            assert!(outline("infra-ops").trim_end().ends_with("built-in  │"));
            assert!(outline("original").trim_end().ends_with("custom  │"));
            let text = screen.join("\n");
            assert!(text.contains("┌ Inspector ─"));
            assert!(text.contains("Taxonomy conventional"));
            assert!(text.contains("Version      2"));
            assert!(
                screen[usize::from(height) - 2]
                    .starts_with("  NAV   ←/→ focus  ↑/↓ move  enter open")
            );
            if height == 40 {
                assert!(text.contains("Types        11"));
                assert!(text.contains("f fork to customize · built-ins are read-only"));
                assert!(text.contains("n new blank taxonomy"));
            }

            select_taxonomy(&mut state, 3);
            terminal.draw(|frame| state.render(frame))?;
            let screen = rows(&terminal);
            assert!(screen.join("\n").contains("Taxonomy original"));
            assert!(
                screen[usize::from(height) - 2]
                    .starts_with("  NAV   ←/→ focus  ↑/↓ move  enter open")
            );
        }
        Ok(())
    }

    #[test]
    fn custom_chips_keep_exact_colors_on_selected_rows_and_in_the_inspector()
    -> Result<(), Box<dyn std::error::Error>> {
        for embedded in [false, true] {
            for width in [MINIMUM_WIDTH, 87, 140] {
                let (workspace, _) = custom_workspace()?;
                let mut state = State::new(&workspace);
                state.embedded = embedded;
                state.handle_event(key(KeyCode::Char('2')), &workspace);
                let mut terminal = Terminal::new(TestBackend::new(width, 40))?;
                for selected in [false, true] {
                    if selected {
                        select_taxonomy(&mut state, 3);
                    }
                    terminal.draw(|frame| state.render(frame))?;
                    let buffer = terminal.backend().buffer();
                    let mut count = 0;
                    for (y, row) in rows(&terminal).iter().enumerate() {
                        for (start, _) in row.match_indices(" custom ") {
                            let x = row[..start].chars().count();
                            for (dx, character) in " custom ".chars().enumerate() {
                                let cell = &buffer[(u16::try_from(x + dx)?, u16::try_from(y)?)];
                                assert_eq!(cell.symbol(), character.to_string());
                                assert_eq!(cell.fg, Color::Black);
                                assert_eq!(cell.bg, Color::Rgb(144, 238, 144));
                                assert_eq!(cell.modifier, Modifier::empty());
                            }
                            count += 1;
                        }
                    }
                    assert_eq!(count, if selected { 2 } else { 1 });
                }
            }
        }
        Ok(())
    }

    #[test]
    fn built_in_origin_stays_on_roots_and_read_only_stays_in_the_inspector()
    -> Result<(), Box<dyn std::error::Error>> {
        for embedded in [false, true] {
            for width in [MINIMUM_WIDTH, 87, 140] {
                let workspace = Workspace::new();
                let mut state = State::new(&workspace);
                state.embedded = embedded;
                state.handle_event(key(KeyCode::Char('2')), &workspace);
                let mut terminal = Terminal::new(TestBackend::new(width, 40))?;
                for next in [
                    None,
                    Some(KeyCode::Enter),
                    Some(KeyCode::Down),
                    Some(KeyCode::Enter),
                    Some(KeyCode::Down),
                ] {
                    if let Some(next) = next {
                        state.handle_event(key(next), &workspace);
                    }
                    terminal.draw(|frame| state.render(frame))?;
                    let observation = state.taxonomies.observe(state.library());
                    let area = observation.outline_area;
                    for (index, id) in observation.visible.iter().enumerate() {
                        let row = row_text(
                            &terminal,
                            area.x,
                            area.y + u16::try_from(index)?,
                            area.width,
                        );
                        let root = id
                            .as_ref()
                            .and_then(|id| state.library()?.node(id))
                            .is_some_and(|node| matches!(node, NodeRef::Taxonomy(_)));
                        assert_eq!(row.contains(" built-in "), root, "{row}");
                        assert!(!row.contains("read-only"), "{row}");
                    }
                    let screen = rows(&terminal);
                    assert_eq!(
                        screen
                            .iter()
                            .filter(|row| row.contains(" ◇ read-only "))
                            .count(),
                        1
                    );
                    let buffer = terminal.backend().buffer();
                    for label in [" built-in ", " ◇ read-only "] {
                        for (y, line) in screen.iter().enumerate() {
                            for (start, _) in line.match_indices(label) {
                                let x = line[..start].chars().count();
                                for (dx, character) in label.chars().enumerate() {
                                    let cell = &buffer[(u16::try_from(x + dx)?, u16::try_from(y)?)];
                                    assert_eq!(cell.symbol(), character.to_string());
                                    assert_eq!(cell.fg, Color::Black);
                                    assert_eq!(cell.bg, Color::Rgb(211, 211, 211));
                                    assert_eq!(cell.modifier, Modifier::empty());
                                }
                            }
                        }
                    }
                    let footer = row_text(&terminal, 0, 38, width);
                    for label in ["built-in", "read-only", "e edit", "c copy", "d delete"] {
                        assert!(!footer.contains(label), "{footer}");
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn outline_expands_types_and_properties_and_the_inspector_follows()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        let mut terminal = Terminal::new(TestBackend::new(87, 40))?;
        let mut press =
            |state: &mut State, code: KeyCode| -> Result<String, Box<dyn std::error::Error>> {
                state.handle_event(key(code), &workspace);
                terminal.draw(|frame| state.render(frame))?;
                Ok(rows(&terminal).join("\n"))
            };
        press(&mut state, KeyCode::Char('2'))?;
        press(&mut state, KeyCode::Enter)?;
        let screen = press(&mut state, KeyCode::Down)?;
        assert!(screen.contains("│▌├─ ▸ feat      intent · decision · constraints"));
        assert_eq!(
            screen
                .lines()
                .find(|row| row.contains("│▌├─ ▸ feat "))
                .and_then(|row| row.rsplit('│').nth(1))
                .map(str::trim_end),
            Some("▌├─ ▸ feat      intent · decision · constraints")
        );
        assert!(screen.contains("│ ├─ ▸ fix       cause · decision · constraints"));
        assert!(screen.contains("Type feat"));
        assert!(screen.contains("in conventional"));
        assert!(screen.contains("Properties   3"));
        assert!(screen.contains(" NAV "));

        let screen = press(&mut state, KeyCode::Enter)?;
        assert_eq!(
            screen
                .lines()
                .find(|row| row.contains("│▌├─ ▾ feat "))
                .and_then(|row| row.rsplit('│').nth(1))
                .map(str::trim_end),
            Some("▌├─ ▾ feat")
        );
        for _ in 0..3 {
            press(&mut state, KeyCode::Down)?;
        }
        let screen = press(&mut state, KeyCode::Enter)?;
        assert!(screen.contains("│▌│  └─ constraints  conditional"));
        assert!(screen.contains("│ │  ├─ intent       required"));
        assert!(screen.contains("│ └─ ▸ revert"));
        assert!(screen.contains("Property constraints"));
        assert!(screen.contains("in conventional › feat"));
        assert!(screen.contains("Policy       required recommended conditional optional"));
        assert!(screen.contains("Condition    non-obvious-requirements"));
        assert!(screen.contains("Rationale    Required when"));
        assert!(screen.contains(" NAV "));
        let buffer = terminal.backend().buffer();
        let (policy_x, policy_y) = (0..40)
            .find_map(|y| {
                row_text(&terminal, 0, y, 87)
                    .find("conditional optional")
                    .map(|x| (x, y))
            })
            .ok_or("missing policy row")?;
        let policy_x = u16::try_from(
            row_text(&terminal, 0, policy_y, 87)[..policy_x]
                .chars()
                .count(),
        )?;
        assert_eq!(buffer[(policy_x, policy_y)].fg, Color::Cyan);
        assert_eq!(buffer[(policy_x, policy_y)].bg, Color::Rgb(60, 60, 60));
        assert_eq!(buffer[(policy_x - 2, policy_y)].fg, Color::Gray);

        state.handle_event(key(KeyCode::Char('e')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(matches!(
            &state.status,
            Some(Notice::Error(message)) if message.starts_with("Built-in taxonomies are read-only")
        ));
        assert_eq!(cursor(&state), ["conventional", "feat", "constraints"]);
        for _ in 0..3 {
            state.handle_event(key(KeyCode::Up), &workspace);
        }
        assert_eq!(cursor(&state), ["conventional", "feat"]);
        state.handle_event(key(KeyCode::Enter), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(cursor(&state), ["conventional", "fix"]);
        state.handle_event(key(KeyCode::Up), &workspace);
        state.handle_event(key(KeyCode::Up), &workspace);
        assert_eq!(cursor(&state), ["conventional"]);
        state.handle_event(key(KeyCode::Enter), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(cursor(&state), ["research"]);
        Ok(())
    }

    #[test]
    fn outline_counts_hidden_rows_on_its_borders_and_takes_clicks_at_the_minimum_size()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.handle_event(key(KeyCode::Char('2')), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        let screen = rows(&terminal);
        let bottom = screen
            .iter()
            .position(|row| row.contains(" more ┘"))
            .ok_or("missing the hidden-rows count")?;
        let visible = bottom - 1;
        assert!(screen[bottom].contains(&format!("↓ {} more", 14 - visible)));
        assert!(!screen[0].contains("more"));
        for _ in 0..20 {
            state.handle_event(key(KeyCode::Down), &workspace);
        }
        terminal.draw(|frame| state.render(frame))?;
        let screen = rows(&terminal);
        assert!(screen[0].contains(&format!("↑ {} more", 14 - visible)));
        assert!(screen[bottom - 1].contains("│▌▸ infra-ops"));
        assert!(screen.join("\n").contains("Taxonomy infra-ops"));

        let first = row_text(&terminal, MENU_WIDTH + 1, 1, MINIMUM_WIDTH - MENU_WIDTH - 2);
        let name = first
            .split(['▸', '▾'])
            .nth(1)
            .and_then(|row| row.split_whitespace().next())
            .ok_or("missing first row")?;
        state.handle_event(click(Rect::new(MENU_WIDTH + 1, 1, 1, 1)), &workspace);
        assert_eq!(
            state.taxonomies.observe(state.library()).cursor_path,
            ["conventional", name]
        );
        Ok(())
    }

    #[test]
    fn embedded_footer_ends_with_the_shell_keys() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.embedded = true;
        state.handle_event(key(KeyCode::Char('2')), &workspace);
        let mut terminal = Terminal::new(TestBackend::new(87, 36))?;
        terminal.draw(|frame| state.render(frame))?;
        let footer = row_text(&terminal, 0, 34, 87);
        assert!(footer.starts_with("  NAV   ←/→ focus  ↑/↓ move  enter open"));
        assert!(footer.trim_end().ends_with("│ tab tabs  / cmds"));
        Ok(())
    }

    #[test]
    fn taxonomy_footer_keeps_open_and_close_shortcuts_at_compact_widths()
    -> Result<(), Box<dyn std::error::Error>> {
        for (width, height) in [(MINIMUM_WIDTH, MINIMUM_HEIGHT), (87, 40)] {
            for embedded in [false, true] {
                for (taxonomy, commit_type) in [
                    ("conventional", None),
                    ("conventional", Some("feat")),
                    ("original", None),
                    ("original", Some("existing")),
                ] {
                    let (workspace, _) = custom_workspace()?;
                    let mut state = State::new(&workspace);
                    state.embedded = embedded;
                    state.handle_event(key(KeyCode::Char('2')), &workspace);
                    let parent = library(&state)?
                        .find_path(&[taxonomy])
                        .ok_or("missing taxonomy")?;
                    state.taxonomies.select(parent);
                    if let Some(commit_type) = commit_type {
                        state.handle_event(key(KeyCode::Enter), &workspace);
                        let node = library(&state)?
                            .find_path(&[taxonomy, commit_type])
                            .ok_or("missing type")?;
                        state.taxonomies.select(node);
                    }
                    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
                    for (shortcut, absent, next) in [
                        ("enter open", "enter close", KeyCode::Enter),
                        ("enter close", "enter open", KeyCode::Enter),
                        ("enter open", "enter close", KeyCode::Enter),
                    ] {
                        terminal.draw(|frame| state.render(frame))?;
                        let footer = row_text(&terminal, 0, height - 2, width);
                        assert!(footer.contains(shortcut), "{footer}");
                        assert!(!footer.contains(absent), "{footer}");
                        assert!(!footer.contains('…'), "{footer}");
                        assert!(!footer.contains("? help"), "{footer}");
                        state.handle_event(key(next), &workspace);
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn library_footer_omits_node_context_in_nav_and_edit() -> Result<(), Box<dyn std::error::Error>>
    {
        for embedded in [false, true] {
            for width in [MINIMUM_WIDTH, 87, 140] {
                for path in [
                    vec!["original"],
                    vec!["original", "existing"],
                    vec!["original", "existing", "notes"],
                ] {
                    let (workspace, _) = custom_workspace()?;
                    let mut state = State::new(&workspace);
                    state.embedded = embedded;
                    state.handle_event(key(KeyCode::Char('2')), &workspace);
                    for depth in 1..=path.len() {
                        let node = library(&state)?.find_path(&path[..depth]).ok_or("node")?;
                        state.taxonomies.select(node);
                        if depth < path.len() {
                            state.handle_event(key(KeyCode::Enter), &workspace);
                        }
                    }
                    let mut terminal = Terminal::new(TestBackend::new(width, 40))?;
                    for editing in [false, true] {
                        if editing {
                            state.handle_event(key(KeyCode::Char('e')), &workspace);
                        }
                        terminal.draw(|frame| state.render(frame))?;
                        let footer = row_text(&terminal, 0, 38, width);
                        for context in ["· taxonomy", "· type", "· property"] {
                            assert!(!footer.contains(context), "{footer}");
                        }
                        if editing {
                            assert!(footer.starts_with("  EDIT   enter next field  esc save"));
                        } else {
                            assert!(footer.starts_with("  NAV   ←/→ focus"));
                            if width >= 87 {
                                assert!(
                                    footer.contains("e edit") && footer.contains("d delete"),
                                    "{footer}"
                                );
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn taxonomy_footer_orders_focus_move_enter_and_actions()
    -> Result<(), Box<dyn std::error::Error>> {
        for embedded in [false, true] {
            let (workspace, _) = custom_workspace()?;
            let mut state = State::new(&workspace);
            state.embedded = embedded;
            state.handle_event(key(KeyCode::Char('2')), &workspace);
            select_taxonomy(&mut state, 3);
            for next in [None, Some(KeyCode::Enter), Some(KeyCode::Down)] {
                if let Some(next) = next {
                    state.handle_event(key(next), &workspace);
                }
                let footer = state
                    .taxonomies
                    .status_line(state.library(), embedded, 138)
                    .to_string();
                let enter = if next == Some(KeyCode::Enter) {
                    "enter close"
                } else {
                    "enter open"
                };
                let mut end = 0;
                for hint in [
                    "←/→ focus",
                    "↑/↓ move",
                    enter,
                    "n new",
                    "f fork",
                    "e edit",
                    "d delete",
                ] {
                    let at = footer
                        .find(hint)
                        .ok_or(format!("missing {hint}: {footer}"))?;
                    assert!(at >= end, "{footer}");
                    end = at + hint.len();
                }
                for width in MINIMUM_WIDTH..=140 {
                    let line = state
                        .taxonomies
                        .status_line(state.library(), embedded, width - 2);
                    assert_eq!(line.width(), usize::from(width - 2));
                    let compact = line.to_string();
                    assert!(compact.contains("←/→ focus  ↑/↓ move"), "{compact}");
                    assert!(compact.contains(enter), "{compact}");
                    assert!(!compact.contains('…'), "{compact}");
                }
            }
        }
        Ok(())
    }

    #[test]
    fn taxonomy_footer_pills_keep_their_padding_inside_the_viewport()
    -> Result<(), Box<dyn std::error::Error>> {
        for (width, height) in [(MINIMUM_WIDTH, MINIMUM_HEIGHT), (87, 40)] {
            for (description, label, _background) in [
                (None, "+ new", Color::Rgb(20, 44, 24)),
                (
                    Some("Updated taxonomy"),
                    "~ modified",
                    Color::Rgb(52, 38, 12),
                ),
                (Some(""), "▲ invalid 1", Color::Rgb(56, 18, 18)),
            ] {
                let (workspace, _) = custom_workspace()?;
                let mut state = State::new(&workspace);
                state.handle_event(key(KeyCode::Char('2')), &workspace);
                let session = state.global.as_mut().ok_or("missing Global session")?;
                let node = if let Some(description) = description {
                    let draft = session.library_mut().ok_or("missing Library")?;
                    let node = draft.find_path(&["original"]).ok_or("missing original")?;
                    draft.edit(&node, DraftEdit::Text(DraftField::Description, description))?;
                    node
                } else {
                    fork_as(session, "conventional", "team")?
                };
                state.taxonomies.select(node);
                let mut terminal = Terminal::new(TestBackend::new(width, height))?;
                for editing in [false, true] {
                    if editing {
                        state.handle_event(key(KeyCode::Char('e')), &workspace);
                    }
                    terminal.draw(|frame| state.render(frame))?;
                    let y = height - 2;
                    let footer = row_text(&terminal, 0, y, width);
                    let mode = if editing { " EDIT " } else { " NAV " };
                    assert!(footer.starts_with(&format!(" {mode}  ")), "{footer}");
                    if !editing {
                        assert!(footer.contains("enter open"), "{footer}");
                    }
                    let buffer = terminal.backend().buffer();
                    let mode_width = u16::try_from(mode.chars().count())?;
                    let mode_background = if editing { crate::theme::AMBER } else { ACCENT };
                    assert!((1..=mode_width).all(|x| buffer[(x, y)].bg == mode_background));
                    for (index, symbol) in mode.chars().enumerate() {
                        let cell = &buffer[(1 + u16::try_from(index)?, y)];
                        assert_eq!(cell.symbol(), symbol.to_string());
                        assert_eq!(cell.fg, JET_BLACK);
                        assert_eq!(cell.modifier, Modifier::BOLD);
                    }
                    for padding_y in [y + 1] {
                        for x in 0..width {
                            assert_eq!(buffer[(x, padding_y)].symbol(), " ");
                            assert_eq!(buffer[(x, padding_y)].bg, JET_BLACK);
                        }
                    }

                    assert_eq!(buffer[(mode_width + 1, y)].bg, JET_BLACK);
                    let badge = format!(" {label} ");
                    assert!(!footer.contains(&badge));
                    assert!(!text(&terminal).contains(&badge));
                    for x in [0, width - 1] {
                        assert_eq!(buffer[(x, y)].symbol(), " ");
                        assert_eq!(buffer[(x, y)].bg, JET_BLACK);
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn arrows_switch_rail_focus_without_changing_any_tree_row()
    -> Result<(), Box<dyn std::error::Error>> {
        let (workspace, _) = custom_workspace()?;
        let mut state = State::new(&workspace);
        state.handle_event(key(KeyCode::Right), &workspace);
        assert_eq!(state.home_focus, HomeFocus::Content);
        state.handle_event(key(KeyCode::Left), &workspace);
        assert_eq!(state.home_focus, HomeFocus::Menu);
        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(state.category, Category::Taxonomies);
        state.handle_event(key(KeyCode::Right), &workspace);
        let probe = |state: &mut State| {
            let before = state.taxonomies.observe(state.library());
            for (code, focus) in [
                (KeyCode::Left, HomeFocus::Menu),
                (KeyCode::Right, HomeFocus::Content),
            ] {
                for _ in 0..2 {
                    state.handle_event(key(code), &workspace);
                    assert_eq!(state.home_focus, focus);
                    assert_eq!(cursor(state), before.cursor_path);
                    assert_eq!(
                        state.taxonomies.observe(state.library()).expanded,
                        before.expanded
                    );
                }
            }
        };
        probe(&mut state);
        for code in [KeyCode::Enter, KeyCode::Down, KeyCode::Enter, KeyCode::Down] {
            state.handle_event(key(code), &workspace);
            probe(&mut state);
        }
        open_notes(&mut state, &workspace);
        probe(&mut state);
        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(cursor(&state), ["original", "existing", "+ add property"]);
        probe(&mut state);
        Ok(())
    }

    #[test]
    fn home_palette_filters_commands_and_routes_through_existing_actions()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(row_text(&terminal, 0, 28, 100).contains("↑/↓ move  enter open  n new  f fork"));
        state.handle_event(key(KeyCode::Char('/')), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        assert!(text(&terminal).contains("Commands"));
        assert!(text(&terminal).contains("╔"));
        let Some(palette) = &state.home_overlay else {
            return Err("palette missing".into());
        };
        let palette_list = palette.list_area;
        let search_top = palette_list.y - 3;
        assert_eq!(
            terminal.backend().buffer()[(palette_list.x - 1, search_top)].symbol(),
            "┌"
        );
        assert_eq!(
            terminal.backend().buffer()[(palette_list.right(), search_top)].symbol(),
            "┐"
        );
        assert_eq!(
            terminal.backend().buffer()[(palette_list.x, search_top + 1)].symbol(),
            "/"
        );
        assert_eq!(
            terminal.backend().buffer()[(palette_list.right() - 1, palette_list.y)].symbol(),
            "n"
        );
        state.handle_event(Event::Paste("fork".into()), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        let Some(palette) = &state.home_overlay else {
            return Err("palette missing".into());
        };
        assert_eq!(palette.visible(state.category), vec![HomeCommand::Fork]);
        assert_eq!(
            terminal.backend().buffer()[(palette.list_area.right() - 1, palette.list_area.y)]
                .symbol(),
            "f"
        );
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(state.taxonomies.editing());
        assert_eq!(cursor(&state), ["conventional-fork"]);
        assert!(state.home_overlay.is_none());
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }

    #[test]
    fn library_and_creation_commands_are_lowercase_and_switch_has_no_shortcut()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        for (width, height) in [(MINIMUM_WIDTH, MINIMUM_HEIGHT), (120, 30)] {
            let mut state = State::new(&workspace);
            state.category = Category::Taxonomies;
            state.home_focus = HomeFocus::Content;
            state.handle_event(key(KeyCode::Char('/')), &workspace);
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| state.render(frame))?;
            assert!(text(&terminal).contains("create taxonomy"));
            let Some(palette) = &state.home_overlay else {
                return Err("missing palette".into());
            };
            assert_eq!(
                row_text(
                    &terminal,
                    palette.list_area.x,
                    palette.list_area.y + 4,
                    palette.list_area.width
                )
                .trim_end(),
                "switch to project"
            );
            for scroll in [false, true] {
                if scroll {
                    for _ in 0..7 {
                        state.handle_event(key(KeyCode::Down), &workspace);
                    }
                    terminal.draw(|frame| state.render(frame))?;
                }
                let Some(palette) = &state.home_overlay else {
                    return Err("missing palette".into());
                };
                for y in palette.list_area.y..palette.list_area.bottom() {
                    assert!(
                        row_text(&terminal, palette.list_area.x, y, palette.list_area.width)
                            .chars()
                            .filter(|character| character.is_alphabetic())
                            .all(char::is_lowercase)
                    );
                }
            }
            state.handle_event(key(KeyCode::Esc), &workspace);
            state.category = Category::Project;
            state.handle_event(key(KeyCode::Char('/')), &workspace);
            state.handle_event(Event::Paste("switch".into()), &workspace);
            terminal.draw(|frame| state.render(frame))?;
            let Some(palette) = &state.home_overlay else {
                return Err("missing palette".into());
            };
            assert_eq!(
                row_text(
                    &terminal,
                    palette.list_area.x,
                    palette.list_area.y,
                    palette.list_area.width
                )
                .trim_end(),
                "Switch to Library"
            );
            state.handle_event(key(KeyCode::Enter), &workspace);
            assert_eq!(state.category, Category::Taxonomies);
        }
        Ok(())
    }

    #[test]
    fn home_arrows_switch_tabs_and_m_does_nothing_at_compact_and_wide_widths()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        for (width, height) in [(MINIMUM_WIDTH, MINIMUM_HEIGHT), (100, 30)] {
            let mut state = State::new(&workspace);
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| state.render(frame))?;
            let footer = row_text(&terminal, 0, height - 2, width);
            assert!(footer.starts_with(" ←/→: focus  ↑/↓: move  /: commands"));
            assert!(!footer.contains('|'));
            state.handle_event(key(KeyCode::Tab), &workspace);
            state.handle_event(key(KeyCode::BackTab), &workspace);
            state.handle_event(key(KeyCode::Char('m')), &workspace);
            assert_eq!(state.category, Category::Project);
            state.handle_event(key(KeyCode::Down), &workspace);
            assert_eq!(state.category, Category::Taxonomies);
            assert!(state.home_overlay.is_none());
            terminal.draw(|frame| state.render(frame))?;
            let footer = row_text(&terminal, 0, height - 2, width);
            assert!(footer.starts_with("  NAV   ←/→ focus  ↑/↓ move  enter open"));
            if width >= 100 {
                assert!(footer.trim_end().ends_with("│ / cmds"));
            }
            assert!(!footer.contains('…'));
            state.handle_event(key(KeyCode::Char('/')), &workspace);
            assert!(state.home_overlay.is_some());
            state.handle_event(key(KeyCode::Esc), &workspace);
            assert_eq!(state.category, Category::Taxonomies);
            let project_tab = state.tab_areas[Category::Project.index()];
            state.handle_event(
                Event::Mouse(ratatui::crossterm::event::MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: project_tab.x + 2,
                    row: project_tab.y,
                    modifiers: KeyModifiers::NONE,
                }),
                &workspace,
            );
            assert_eq!(state.category, Category::Project);
        }
        Ok(())
    }

    #[test]
    fn palette_blocks_underlying_keys_and_reports_unavailable_actions()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.handle_event(key(KeyCode::Char('/')), &workspace);
        state.handle_event(Event::Paste("edit".into()), &workspace);
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        let Some(palette) = &state.home_overlay else {
            return Err("palette missing".into());
        };
        assert_eq!(palette.query, "editn");
        assert!(matches!(state.screen, Screen::Home));
        state.handle_event(key(KeyCode::Backspace), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(matches!(state.status, Some(Notice::Error(_))));
        assert!(state.home_overlay.is_none());
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }

    #[test]
    fn project_palette_keeps_the_review_boundary() {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.handle_event(key(KeyCode::Char('/')), &workspace);
        state.handle_event(Event::Paste("initialize".into()), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(
            state.screen,
            Screen::Review(ConfigurationDestination::Project)
        ));
        assert_eq!(workspace.saves.get(), 0);
    }

    #[test]
    fn question_mark_has_no_navigation_action_or_footer_hint()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        for category in Category::ALL {
            for embedded in [false, true] {
                let mut state = State::new(&workspace);
                state.category = category;
                state.embedded = embedded;
                for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT] {
                    state.handle_event(
                        Event::Key(KeyEvent::new(KeyCode::Char('?'), modifiers)),
                        &workspace,
                    );
                    assert!(state.home_overlay.is_none());
                    assert_eq!(state.category, category);
                }
                let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
                terminal.draw(|frame| state.render(frame))?;
                assert!(!row_text(&terminal, 0, 28, 100).contains("help"));
                state.handle_event(key(KeyCode::Char('/')), &workspace);
                state.handle_event(Event::Paste("help".into()), &workspace);
                assert!(
                    state
                        .home_overlay
                        .as_ref()
                        .ok_or("palette")?
                        .visible(category)
                        .is_empty()
                );
            }
        }
        Ok(())
    }

    #[test]
    fn palette_library_actions_respect_dirty_project_confirmation()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state
            .project
            .as_mut()
            .ok_or("missing project")?
            .initialize_project()?;
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.handle_event(key(KeyCode::Char('/')), &workspace);
        state.handle_event(Event::Paste("create".into()), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(
            state.screen,
            Screen::ScopeChange(PendingAction::Library(LibraryAction::Create))
        ));
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }

    #[test]
    fn command_popup_fits_the_minimum_and_blocks_underlying_mouse_targets()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(!row_text(&terminal, 0, MINIMUM_HEIGHT - 2, MINIMUM_WIDTH).contains("? help"));
        let project_tab = state.tab_areas[Category::Project.index()];
        state.handle_event(key(KeyCode::Char('/')), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        assert!(text(&terminal).contains("Commands"));
        assert!(text(&terminal).contains("╔"));
        assert!(text(&terminal).contains("┌ Search ─"));
        assert!(text(&terminal).contains("▌"));
        let Some(palette) = &state.home_overlay else {
            return Err("palette missing".into());
        };
        assert_eq!(palette.list_area.height, 6);
        assert_eq!(
            terminal.backend().buffer()[(palette.list_area.x - 1, palette.list_area.y - 3)]
                .symbol(),
            "┌"
        );
        assert_eq!(
            terminal.backend().buffer()[(palette.list_area.right(), palette.list_area.y - 3)]
                .symbol(),
            "┐"
        );
        for _ in 0..7 {
            state.handle_event(key(KeyCode::Down), &workspace);
        }
        terminal.draw(|frame| state.render(frame))?;
        let Some(palette) = &state.home_overlay else {
            return Err("palette missing".into());
        };
        assert_eq!(palette.selected, 5);
        assert_eq!(palette.list_offset, 0);
        assert!(
            row_text(
                &terminal,
                palette.list_area.x,
                palette.list_area.bottom() - 1,
                palette.list_area.width
            )
            .contains("quit")
        );
        let list = palette.list_area;
        state.handle_event(
            Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: list.x,
                row: list.y,
                modifiers: KeyModifiers::NONE,
            }),
            &workspace,
        );
        let Some(palette) = &state.home_overlay else {
            return Err("palette missing".into());
        };
        assert_eq!(palette.selected, 0);
        state.handle_event(
            Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: project_tab.x,
                row: project_tab.y,
                modifiers: KeyModifiers::NONE,
            }),
            &workspace,
        );
        assert!(state.home_overlay.is_none());
        assert_eq!(state.category, Category::Taxonomies);
        Ok(())
    }

    #[test]
    fn overlay_notices_replace_the_message_row() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        state.status = Some(Notice::Error("Invalid selection".into()));
        terminal.draw(|frame| state.render(frame))?;
        assert_eq!(terminal.backend().buffer()[(0, 24)].symbol(), "│");
        assert_eq!(terminal.backend().buffer()[(45, 26)].fg, Color::Red);
        assert!(text(&terminal).contains("Invalid selection"));

        state.status = Some(Notice::Info("Configuration applied.".into()));
        terminal.draw(|frame| state.render(frame))?;
        assert_eq!(terminal.backend().buffer()[(45, 26)].fg, Color::White);
        assert!(text(&terminal).contains("Configuration applied."));
        state.handle_event(key(KeyCode::Down), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        assert!(!text(&terminal).contains("Configuration applied."));
        Ok(())
    }

    #[test]
    fn load_errors_remain_visible_beneath_transient_notices()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        state.project = None;
        state.project_error = "project load failed".into();
        terminal.draw(|frame| state.render(frame))?;
        assert_eq!(terminal.backend().buffer()[(45, 26)].fg, Color::Red);

        state.status = Some(Notice::Info("Review pending".into()));
        terminal.draw(|frame| state.render(frame))?;
        assert_eq!(terminal.backend().buffer()[(45, 26)].fg, Color::White);
        assert!(text(&terminal).contains("Review pending"));

        state.status = None;
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.global = None;
        state.global_error = "global load failed".into();
        terminal.draw(|frame| state.render(frame))?;
        assert_eq!(terminal.backend().buffer()[(45, 26)].fg, Color::Red);
        assert!(text(&terminal).contains("Global load failed."));
        Ok(())
    }

    #[test]
    fn browsing_preserves_project_draft_until_global_action()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state
            .project
            .as_mut()
            .ok_or("missing project")?
            .initialize_project()?;
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        assert_eq!(row_text(&terminal, 0, 24, MENU_WIDTH), "│              │");
        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(state.category, Category::Taxonomies);
        terminal.draw(|frame| state.render(frame))?;
        assert!(text(&terminal).contains("Inspector"));
        assert!(state.scope_dirty(ConfigurationDestination::Project));
        assert_eq!(workspace.saves.get(), 0);
        state.handle_event(key(KeyCode::Char('e')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(matches!(state.status, Some(Notice::Error(_))));
        assert_eq!(workspace.saves.get(), 0);
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        assert!(matches!(
            state.screen,
            Screen::ScopeChange(PendingAction::Library(LibraryAction::Create))
        ));
        assert_eq!(workspace.saves.get(), 0);
        state.handle_event(key(KeyCode::Esc), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert_eq!(state.category, Category::Taxonomies);
        assert!(state.scope_dirty(ConfigurationDestination::Project));
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        state.handle_event(key(KeyCode::Char('a')), &workspace);
        assert_eq!(workspace.saves.get(), 1);
        assert!(matches!(state.screen, Screen::Home) && state.taxonomies.editing());
        assert_eq!(cursor(&state), [""]);
        assert_eq!(state.active_scope, ConfigurationDestination::Global);
        Ok(())
    }

    #[test]
    fn taxonomy_commands_target_selected_global_definition()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.handle_event(key(KeyCode::Char('e')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(matches!(state.status, Some(Notice::Error(_))));
        state.handle_event(key(KeyCode::Char('d')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(matches!(state.status, Some(Notice::Error(_))));
        state.handle_event(key(KeyCode::Char('f')), &workspace);
        assert!(state.taxonomies.editing());
        assert_eq!(cursor(&state), ["conventional-fork"]);
        let fork = library(&state)?
            .find_path(&["conventional-fork"])
            .ok_or("missing fork")?;
        state
            .global
            .as_mut()
            .and_then(ConfigurationSession::library_mut)
            .ok_or("missing global")?
            .edit(&fork, DraftEdit::Text(DraftField::Name, "team"))?;
        state.handle_event(key(KeyCode::Esc), &workspace);
        assert!(!state.taxonomies.editing());
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(text(&terminal).contains("Forked from  conventional@2"));
        let team = rows(&terminal)
            .into_iter()
            .find(|row| row.contains("▌▾ team"))
            .ok_or("team row")?;
        assert!(team.trim_end().ends_with("custom  │"));
        state.handle_event(key(KeyCode::Char('e')), &workspace);
        assert!(matches!(
            state.taxonomies.observe(state.library()).mode,
            taxonomies::Mode::Edit {
                field: DraftField::Description,
                ..
            }
        ));
        state.handle_event(key(KeyCode::Esc), &workspace);
        state.handle_event(key(KeyCode::Char('d')), &workspace);
        assert!(matches!(&state.screen, Screen::ConfirmRemove(node) if node == &fork));
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert_eq!(workspace.saves.get(), 1);
        Ok(())
    }

    #[test]
    fn unavailable_library_keeps_the_frame_and_reports_the_error()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.global = None;
        state.global_error = "global load failed".into();
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        let rendered = text(&terminal);
        assert!(rendered.contains("Taxonomies"));
        assert!(rendered.contains("Taxonomy library unavailable."));
        assert!(rendered.contains("/ cmds"));
        assert!(rendered.contains("Global load failed."));
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(matches!(state.status, Some(Notice::Error(_))));
        Ok(())
    }

    #[test]
    fn too_small_view_blocks_hidden_actions() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH - 1, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(text(&terminal).contains("Terminal too small"));
        let before = state.project_section;
        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(state.project_section, before);
        let mut short = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT - 1))?;
        short.draw(|frame| state.render(frame))?;
        assert!(text(&short).contains("Terminal too small"));
        let mut minimum = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        minimum.draw(|frame| state.render(frame))?;
        assert!(text(&minimum).contains("Project"));
        let buffer = minimum.backend().buffer();
        assert_eq!(buffer[(0, 0)].symbol(), "┌");
        assert_eq!(buffer[(MENU_WIDTH, 0)].symbol(), "┌");
        assert_eq!(buffer[(0, 15)].symbol(), "└");
        Ok(())
    }

    #[test]
    fn remove_dialog_invalidates_mouse_targets_after_resize()
    -> Result<(), Box<dyn std::error::Error>> {
        let (workspace, _) = custom_workspace()?;
        let mut state = State::new(&workspace);
        state.handle_event(key(KeyCode::Char('2')), &workspace);
        select_taxonomy(&mut state, 3);
        state.handle_event(key(KeyCode::Char('d')), &workspace);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(text(&terminal).contains("Delete taxonomy \"original\" from the Library?"));
        let old = state.dialog.buttons.ok_or("missing buttons")?;
        state.handle_event(Event::Resize(30, 7), &workspace);
        assert!(state.dialog.buttons.is_none());
        state.handle_event(click(old.confirm), &workspace);
        assert!(matches!(state.screen, Screen::ConfirmRemove(_)));
        let mut tiny = Terminal::new(TestBackend::new(30, 7))?;
        tiny.draw(|frame| state.render(frame))?;
        assert!(state.dialog.buttons.is_none());
        state.handle_event(click(old.keep), &workspace);
        assert!(matches!(state.screen, Screen::ConfirmRemove(_)));
        state.handle_event(key(KeyCode::Esc), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(!state.any_dirty());
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }

    #[test]
    fn notices_read_as_sentences_and_name_the_blank_field() {
        assert_eq!(sentence("global load failed"), "Global load failed.");
        assert_eq!(sentence(" Already a sentence. "), "Already a sentence.");
        assert_eq!(sentence("Apply now?"), "Apply now?");
        assert_eq!(sentence(""), "");

        let mut library = LibraryDraft::from_configuration(GlobalConfiguration::default());
        let sentences = |library: &LibraryDraft| {
            library
                .problems()
                .iter()
                .map(|problem| problem_sentence(library, problem))
                .collect::<Vec<_>>()
        };
        let edit = |library: &mut LibraryDraft, node: &NodeId, edit: DraftEdit<'_>| {
            library
                .edit(node, edit)
                .unwrap_or_else(|refusal| unreachable!("{refusal}"))
        };
        let team = library.create_taxonomy();
        assert_eq!(
            sentences(&library),
            [
                "Taxonomy name is required.",
                "Taxonomy description is required.",
            ]
        );
        edit(
            &mut library,
            &team,
            DraftEdit::Text(DraftField::Name, "Team"),
        );
        edit(
            &mut library,
            &team,
            DraftEdit::Text(DraftField::Description, "Team changes"),
        );
        let release = edit(&mut library, &team, DraftEdit::AddType);
        edit(
            &mut library,
            &release,
            DraftEdit::Text(DraftField::Name, "release"),
        );
        let notes = edit(&mut library, &release, DraftEdit::AddProperty);
        edit(
            &mut library,
            &notes,
            DraftEdit::Text(DraftField::Name, "notes"),
        );
        edit(
            &mut library,
            &notes,
            DraftEdit::Policy(gitserious_app::Policy::Conditional),
        );
        assert_eq!(
            sentences(&library),
            [
                "Type description is required.",
                "Property description is required.",
                "A conditional policy needs a condition.",
                "A conditional policy needs a rationale."
            ]
        );
    }

    #[test]
    fn a_long_notice_shows_in_full_at_the_smallest_size() -> Result<(), Box<dyn std::error::Error>>
    {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        let message = "invalid global configuration /home/someone/.config/gitserious/config.toml: \
                       TOML parse error at line 10, column 1: missing field properties in the \
                       commit type release of taxonomy team";
        state.status = Some(Notice::error(message.into()));
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        let rendered = text(&terminal);
        for word in ["Invalid", "parse", "properties", "team."] {
            assert!(rendered.contains(word), "{word} is cut off");
        }
        Ok(())
    }

    fn press(state: &mut State, workspace: &Workspace, keys: &[KeyCode]) {
        for &code in keys {
            state.handle_event(key(code), workspace);
        }
    }

    fn field(state: &State) -> Option<DraftField> {
        match state.taxonomies.observe(state.library()).mode {
            taxonomies::Mode::Edit { field, .. } => Some(field),
            taxonomies::Mode::Nav | taxonomies::Mode::CopyTo { .. } => None,
        }
    }

    fn text_of(
        state: &State,
        path: &[&str],
        field: DraftField,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let library = library(state)?;
        let node = library.find_path(path).ok_or("missing node")?;
        Ok(library
            .text(&node, field)
            .ok_or("missing field")?
            .to_owned())
    }

    fn notice(state: &State) -> Option<&str> {
        match &state.status {
            Some(Notice::Error(message) | Notice::Info(message)) => Some(message),
            None => None,
        }
    }

    /// `2`, then the `notes` property of `original` with its type expanded.
    fn open_notes(state: &mut State, workspace: &Workspace) {
        state.handle_event(key(KeyCode::Char('2')), workspace);
        select_taxonomy(state, 3);
        press(
            state,
            workspace,
            &[KeyCode::Enter, KeyCode::Down, KeyCode::Enter, KeyCode::Down],
        );
    }

    #[test]
    fn property_navigation_keeps_policy_and_frames_follow_nav_and_edit()
    -> Result<(), Box<dyn std::error::Error>> {
        use gitserious_app::Policy;
        for embedded in [false, true] {
            for (width, height) in [(MINIMUM_WIDTH, MINIMUM_HEIGHT), (87, 40)] {
                let (workspace, _) = custom_workspace()?;
                let mut state = State::new(&workspace);
                state.embedded = embedded;
                open_notes(&mut state, &workspace);
                let mut terminal = Terminal::new(TestBackend::new(width, height))?;
                let policy = |state: &State| -> Result<Policy, Box<dyn std::error::Error>> {
                    let library = library(state)?;
                    let id = library
                        .find_path(&["original", "existing", "notes"])
                        .ok_or("notes")?;
                    match library.node(&id) {
                        Some(NodeRef::Property { property, .. }) => Ok(property.policy()),
                        _ => Err("notes property".into()),
                    }
                };
                let initial = policy(&state)?;
                press(&mut state, &workspace, &[KeyCode::Left]);
                assert_eq!(cursor(&state), ["original", "existing", "notes"]);
                assert_eq!(policy(&state)?, initial);
                assert_eq!(state.home_focus, HomeFocus::Menu);
                press(&mut state, &workspace, &[KeyCode::Right]);
                terminal.draw(|frame| state.render(frame))?;
                let footer = row_text(&terminal, 0, height - 2, width);
                assert!(
                    footer.contains("e edit")
                        && footer.contains("c copy property")
                        && footer.contains("d delete"),
                    "{footer}"
                );
                assert!(!footer.contains("policy"));
                let inspector_y = (0..height)
                    .find(|y| {
                        row_text(&terminal, MENU_WIDTH, *y, width - MENU_WIDTH)
                            .starts_with("┌ Inspector")
                    })
                    .ok_or("Inspector")?;
                let frames = |terminal: &Terminal<TestBackend>, outline, inspector| {
                    let buffer = terminal.backend().buffer();
                    assert_eq!(buffer[(MENU_WIDTH, 0)].fg, outline);
                    assert_eq!(buffer[(MENU_WIDTH, inspector_y)].fg, inspector);
                };
                frames(&terminal, ACCENT, ACCENT);
                assert_eq!(
                    state.taxonomies.observe(state.library()).inspector_offset,
                    0
                );
                press(&mut state, &workspace, &[KeyCode::Left]);
                assert_eq!(cursor(&state), ["original", "existing", "notes"]);
                assert_eq!(state.home_focus, HomeFocus::Menu);
                press(&mut state, &workspace, &[KeyCode::Right]);
                assert_eq!(policy(&state)?, initial);
                press(
                    &mut state,
                    &workspace,
                    &[KeyCode::Char('e'), KeyCode::Enter, KeyCode::Enter],
                );
                assert_eq!(field(&state), Some(DraftField::Policy));
                let mut reached = std::collections::BTreeSet::new();
                for _ in 0..4 {
                    reached.insert(policy(&state)?.label());
                    press(&mut state, &workspace, &[KeyCode::Right]);
                    terminal.draw(|frame| state.render(frame))?;
                    frames(&terminal, ACCENT, crate::theme::AMBER);
                }
                assert_eq!(reached.len(), 4);
                press(&mut state, &workspace, &[KeyCode::Esc]);
                terminal.draw(|frame| state.render(frame))?;
                frames(&terminal, ACCENT, ACCENT);
                state.home_focus = HomeFocus::Menu;
                terminal.draw(|frame| state.render(frame))?;
                let inactive = crate::theme::focus_frame_style(false)
                    .fg
                    .ok_or("inactive color")?;
                frames(&terminal, inactive, inactive);
            }
        }
        Ok(())
    }

    #[test]
    fn property_commands_fit_resize_and_stay_out_of_editing()
    -> Result<(), Box<dyn std::error::Error>> {
        for embedded in [false, true] {
            for create in [false, true] {
                let (workspace, _) = custom_workspace()?;
                let mut state = State::new(&workspace);
                state.embedded = embedded;
                open_notes(&mut state, &workspace);
                if create {
                    press(&mut state, &workspace, &[KeyCode::Down, KeyCode::Enter]);
                } else {
                    press(&mut state, &workspace, &[KeyCode::Char('e')]);
                }
                for character in "ecd".chars() {
                    press(&mut state, &workspace, &[KeyCode::Char(character)]);
                }
                let name = if create { "ecd" } else { "notesecd" };
                assert_eq!(
                    text_of(&state, &["original", "existing", name], DraftField::Name)?,
                    name
                );
                assert!(matches!(state.screen, Screen::Home));
                assert!(state.confirmation().is_none());
                if create {
                    press(&mut state, &workspace, &[KeyCode::Enter]);
                    state.handle_event(Event::Paste("Property purpose".into()), &workspace);
                }
                press(&mut state, &workspace, &[KeyCode::Esc]);
                for width in MINIMUM_WIDTH..=140 {
                    let line = state
                        .taxonomies
                        .status_line(state.library(), embedded, width - 2);
                    assert_eq!(line.width(), usize::from(width - 2));
                    let footer = line.to_string();
                    assert!(
                        footer.contains("e edit  c copy property") && footer.contains("d delete"),
                        "{footer}"
                    );
                    assert!(!footer.contains('…'), "{footer}");
                    if embedded && footer.contains("/ cmds") {
                        assert!(footer.contains("tab tabs"), "{footer}");
                    }
                }
                press(&mut state, &workspace, &[KeyCode::Char('e')]);
                for width in [MINIMUM_WIDTH, 87, 140, MINIMUM_WIDTH, 140] {
                    let mut terminal = Terminal::new(TestBackend::new(width, 40))?;
                    terminal.draw(|frame| state.render(frame))?;
                    let footer = row_text(&terminal, 0, 38, width);
                    assert!(
                        footer.contains("EDIT")
                            && footer.contains("enter next field")
                            && footer.contains("esc save"),
                        "{footer}"
                    );
                    for label in ["e edit", "c copy", "d delete"] {
                        assert!(!footer.contains(label), "{footer}");
                    }
                    press(&mut state, &workspace, &[KeyCode::Esc]);
                    terminal.draw(|frame| state.render(frame))?;
                    let footer = row_text(&terminal, 0, 38, width);
                    assert!(footer.starts_with("  NAV   ←/→ focus"), "{footer}");
                    assert!(footer.contains("d delete"), "{footer}");
                    assert!(!footer.contains('…'), "{footer}");
                    assert_eq!(
                        footer.contains("to other types"),
                        width == 140 || (width == 87 && !embedded),
                        "{footer}"
                    );
                    assert!(!footer.contains("? help"), "{footer}");
                    assert!(footer.starts_with(' ') && footer.ends_with(' '));
                    assert!(row_text(&terminal, 0, 39, width).trim().is_empty());
                    press(&mut state, &workspace, &[KeyCode::Char('e')]);
                }
            }
        }
        Ok(())
    }

    #[test]
    fn a_custom_property_edits_in_place_and_each_field_holds_at_most_its_limit()
    -> Result<(), Box<dyn std::error::Error>> {
        for (width, height) in [(87, 40), (MINIMUM_WIDTH, MINIMUM_HEIGHT)] {
            let (workspace, _) = custom_workspace()?;
            let mut state = State::new(&workspace);
            open_notes(&mut state, &workspace);
            assert_eq!(cursor(&state), ["original", "existing", "notes"]);
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| state.render(frame))?;
            let screen = rows(&terminal).join("\n");
            assert!(screen.contains("130/100"), "{screen}");
            let footer = row_text(&terminal, 0, height - 2, width);
            assert!(footer.starts_with("  NAV   ←/→ focus"), "{footer}");
            assert!(!footer.contains("←/→ policy"), "{footer}");
            assert!(!footer.contains("enter open"), "{footer}");
            assert!(!footer.contains("enter close"), "{footer}");

            press(&mut state, &workspace, &[KeyCode::Char('e')]);
            assert_eq!(field(&state), Some(DraftField::Name));
            terminal.draw(|frame| state.render(frame))?;
            let screen = rows(&terminal).join("\n");
            assert!(screen.contains("5/80"), "{screen}");
            let footer = row_text(&terminal, 0, height - 2, width);
            assert!(
                footer.starts_with("  EDIT   enter next field  esc save"),
                "{footer}"
            );
            assert!(!footer.contains("ctrl+s review"), "{footer}");
            assert!(matches!(state.screen, Screen::Home));

            press(
                &mut state,
                &workspace,
                &[KeyCode::Char('q'), KeyCode::Char('1'), KeyCode::Char('/')],
            );
            assert!(state.home_overlay.is_none());
            let name = ["original", "existing", "notesq1/"];
            assert_eq!(text_of(&state, &name, DraftField::Name)?, "notesq1/");
            state.handle_event(Event::Paste("x".repeat(100)), &workspace);
            let long = format!("notesq1/{}", "x".repeat(72));
            assert_eq!(
                text_of(
                    &state,
                    &["original", "existing", long.as_str()],
                    DraftField::Name
                )?
                .len(),
                80
            );
            assert_eq!(notice(&state), Some("Name is limited to 80 characters."));

            press(&mut state, &workspace, &[KeyCode::Enter]);
            assert_eq!(field(&state), Some(DraftField::Description));
            let path = ["original", "existing", long.as_str()];
            press(&mut state, &workspace, &[KeyCode::Char('y')]);
            assert_eq!(text_of(&state, &path, DraftField::Description)?.len(), 130);
            assert_eq!(
                notice(&state),
                Some("Description is limited to 100 characters.")
            );
            press(&mut state, &workspace, &[KeyCode::Backspace]);
            assert_eq!(text_of(&state, &path, DraftField::Description)?.len(), 129);

            press(
                &mut state,
                &workspace,
                &[KeyCode::Enter, KeyCode::Right, KeyCode::Left, KeyCode::Left],
            );
            assert_eq!(field(&state), Some(DraftField::Policy));
            press(&mut state, &workspace, &[KeyCode::Enter]);
            assert_eq!(field(&state), Some(DraftField::Condition));
            for character in "risky".chars() {
                press(&mut state, &workspace, &[KeyCode::Char(character)]);
            }
            press(&mut state, &workspace, &[KeyCode::Enter]);
            assert_eq!(field(&state), Some(DraftField::Rationale));
            terminal.draw(|frame| state.render(frame))?;
            assert!(rows(&terminal).join("\n").contains("5/80"));
            press(&mut state, &workspace, &[KeyCode::Enter]);
            assert_eq!(field(&state), Some(DraftField::Name));
            press(&mut state, &workspace, &[KeyCode::Esc]);
            assert_eq!(field(&state), Some(DraftField::Rationale));
            assert!(matches!(state.screen, Screen::Home));
            assert!(state.scope_dirty(ConfigurationDestination::Global));
            press(&mut state, &workspace, &[KeyCode::Right, KeyCode::Left]);
            assert_eq!(text_of(&state, &path, DraftField::Condition)?, "risky");
            assert_eq!(workspace.saves.get(), 0);
        }
        Ok(())
    }

    #[test]
    fn a_stored_taxonomy_edits_its_description_and_keeps_its_name()
    -> Result<(), Box<dyn std::error::Error>> {
        let (workspace, _) = custom_workspace()?;
        let mut state = State::new(&workspace);
        state.handle_event(key(KeyCode::Char('2')), &workspace);
        select_taxonomy(&mut state, 3);
        press(
            &mut state,
            &workspace,
            &[KeyCode::Char('e'), KeyCode::Enter, KeyCode::Home],
        );
        assert_eq!(field(&state), Some(DraftField::Description));
        press(
            &mut state,
            &workspace,
            &[KeyCode::Char('A'), KeyCode::End, KeyCode::Char('.')],
        );
        assert_eq!(
            text_of(&state, &["original"], DraftField::Description)?,
            "AOriginal taxonomy."
        );
        assert_eq!(cursor(&state), ["original"]);
        let version = library(&state)?.version(
            &library(&state)?
                .find_path(&["original"])
                .ok_or("original")?,
        );
        assert_eq!(version, TaxonomyVersion::new(4).ok());
        Ok(())
    }

    #[test]
    fn add_rows_new_and_fork_create_blank_drafts_and_open_their_name()
    -> Result<(), Box<dyn std::error::Error>> {
        let (workspace, _) = custom_workspace()?;
        let mut state = State::new(&workspace);
        open_notes(&mut state, &workspace);
        press(&mut state, &workspace, &[KeyCode::Down]);
        assert_eq!(cursor(&state), ["original", "existing", "+ add property"]);
        let mut terminal = Terminal::new(TestBackend::new(87, 40))?;
        terminal.draw(|frame| state.render(frame))?;
        let screen = rows(&terminal).join("\n");
        assert!(screen.contains("│▌│  └─ + add property"), "{screen}");
        assert!(screen.contains("│ └─ + add type"), "{screen}");
        let footer = row_text(&terminal, 0, 38, 87);
        assert!(footer.contains("enter add"), "{footer}");
        assert!(!footer.contains("enter open"), "{footer}");
        assert!(!footer.contains("enter close"), "{footer}");

        press(&mut state, &workspace, &[KeyCode::Enter]);
        assert_eq!(field(&state), Some(DraftField::Name));
        assert_eq!(cursor(&state), ["original", "existing", ""]);
        for character in "verification".chars() {
            press(&mut state, &workspace, &[KeyCode::Char(character)]);
        }
        assert_eq!(cursor(&state), ["original", "existing", "verification"]);
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        press(&mut state, &workspace, &[KeyCode::Esc]);
        assert_eq!(field(&state), Some(DraftField::Description));
        assert_eq!(notice(&state), None);
        state.handle_event(Event::Paste("Verification evidence".into()), &workspace);
        press(
            &mut state,
            &workspace,
            &[KeyCode::Esc, KeyCode::Down, KeyCode::Down],
        );
        assert_eq!(cursor(&state), ["original", "+ add type"]);
        press(&mut state, &workspace, &[KeyCode::Enter]);
        assert_eq!(cursor(&state), ["original", ""]);
        assert_eq!(field(&state), Some(DraftField::Name));

        press(&mut state, &workspace, &[KeyCode::Esc, KeyCode::Char('n')]);
        assert_eq!(cursor(&state), [""]);
        assert_eq!(field(&state), Some(DraftField::Name));
        press(&mut state, &workspace, &[KeyCode::Esc]);
        select_taxonomy(&mut state, 1);
        press(&mut state, &workspace, &[KeyCode::Char('f')]);
        assert_eq!(cursor(&state), ["research-fork"]);
        assert_eq!(field(&state), Some(DraftField::Name));
        assert_eq!(workspace.saves.get(), 1);
        Ok(())
    }

    #[test]
    fn removing_a_property_asks_first_and_lands_on_its_type()
    -> Result<(), Box<dyn std::error::Error>> {
        let (workspace, _) = custom_workspace()?;
        let mut state = State::new(&workspace);
        open_notes(&mut state, &workspace);
        press(&mut state, &workspace, &[KeyCode::Char('d')]);
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(text(&terminal).contains("Remove property \"notes\" from type"));
        assert!(row_text(&terminal, 0, MINIMUM_HEIGHT - 2, MINIMUM_WIDTH).contains("y: remove"));
        press(&mut state, &workspace, &[KeyCode::Char('y')]);
        assert!(matches!(state.screen, Screen::Home));
        assert_eq!(cursor(&state), ["original", "existing"]);
        assert!(
            library(&state)?
                .find_path(&["original", "existing", "notes"])
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn alt_arrows_reorder_a_custom_type_and_the_cursor_follows()
    -> Result<(), Box<dyn std::error::Error>> {
        let (workspace, _) = custom_workspace()?;
        let mut state = State::new(&workspace);
        let draft = state
            .global
            .as_mut()
            .and_then(ConfigurationSession::library_mut)
            .ok_or("missing global")?;
        let original = draft.find_path(&["original"]).ok_or("missing original")?;
        let added = draft.edit(&original, DraftEdit::AddType)?;
        draft.edit(&added, DraftEdit::Text(DraftField::Name, "added"))?;
        draft.edit(
            &added,
            DraftEdit::Text(DraftField::Description, "Added type"),
        )?;
        let saved = workspace.save(state.global.as_ref().ok_or("Global")?)?;
        state.global = Some(saved);
        open_notes(&mut state, &workspace);
        press(&mut state, &workspace, &[KeyCode::Up]);
        assert_eq!(cursor(&state), ["original", "existing"]);
        let mut terminal = Terminal::new(TestBackend::new(120, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(row_text(&terminal, 0, 28, 120).contains("alt+↑/↓ reorder"));

        let alt = |code| Event::Key(KeyEvent::new(code, KeyModifiers::ALT));
        state.handle_event(alt(KeyCode::Down), &workspace);
        assert_eq!(cursor(&state), ["original", "existing"]);
        let types = |state: &State| -> Result<Vec<String>, Box<dyn std::error::Error>> {
            Ok(library(state)?.taxonomies()[3]
                .types()
                .iter()
                .map(|commit_type| commit_type.name().to_owned())
                .collect())
        };
        assert_eq!(types(&state)?, ["added", "existing"]);
        state.handle_event(alt(KeyCode::Down), &workspace);
        assert_eq!(types(&state)?, ["added", "existing"]);
        state.handle_event(alt(KeyCode::Up), &workspace);
        assert_eq!(types(&state)?, ["existing", "added"]);
        assert_eq!(cursor(&state), ["original", "existing"]);

        select_taxonomy(&mut state, 0);
        press(&mut state, &workspace, &[KeyCode::Enter, KeyCode::Down]);
        let before = cursor(&state);
        state.handle_event(alt(KeyCode::Down), &workspace);
        assert_ne!(
            cursor(&state),
            before,
            "alt+↓ on a built-in type only moves the cursor"
        );
        Ok(())
    }

    #[test]
    fn deleting_a_selected_taxonomy_is_refused_before_its_dialog()
    -> Result<(), Box<dyn std::error::Error>> {
        let (workspace, source) = custom_workspace()?;
        *workspace.global.borrow_mut() =
            ConfigurationSession::open_global(GlobalConfiguration::new(
                1,
                TaxonomyId::new("conventional")?,
                vec![
                    TaxonomyId::new("conventional")?,
                    TaxonomyId::new("original")?,
                ],
                vec![source.into()],
            )?);
        let mut state = State::new(&workspace);
        state.handle_event(key(KeyCode::Char('2')), &workspace);
        select_taxonomy(&mut state, 3);
        press(&mut state, &workspace, &[KeyCode::Char('d')]);
        assert!(matches!(state.screen, Screen::Home));
        assert_eq!(
            notice(&state),
            Some("Taxonomy \"original\" is not installed.")
        );
        assert!(library(&state)?.find_path(&["original"]).is_some());
        Ok(())
    }

    #[test]
    fn quitting_after_a_library_save_has_no_discard_prompt()
    -> Result<(), Box<dyn std::error::Error>> {
        let (workspace, _) = custom_workspace()?;
        let mut state = State::new(&workspace);
        open_notes(&mut state, &workspace);
        press(
            &mut state,
            &workspace,
            &[KeyCode::Char('e'), KeyCode::Char('q'), KeyCode::Esc],
        );
        assert!(!state.any_dirty());
        assert_eq!(workspace.saves(), 1);
        assert!(state.handle_event(key(KeyCode::Char('q')), &workspace));
        assert!(matches!(state.screen, Screen::Home));
        Ok(())
    }

    #[test]
    fn every_built_in_entry_fits_its_field_limits() {
        let library = LibraryDraft::from_configuration(GlobalConfiguration::default());
        let fields = [
            DraftField::Name,
            DraftField::Description,
            DraftField::Condition,
            DraftField::Rationale,
        ];
        for taxonomy in library.taxonomies() {
            let ids = std::iter::once(taxonomy.id()).chain(taxonomy.types().iter().flat_map(
                |commit_type| {
                    std::iter::once(commit_type.id()).chain(
                        commit_type
                            .properties()
                            .iter()
                            .map(gitserious_app::PropertyNode::id),
                    )
                },
            ));
            for id in ids {
                for field in fields {
                    let (Some(text), Some(limit)) = (library.text(id, field), field.limit()) else {
                        continue;
                    };
                    assert!(
                        text.chars().count() <= limit,
                        "{:?} {field:?} has {} characters",
                        library.node(id).map(NodeRef::names),
                        text.chars().count()
                    );
                }
            }
        }
    }
}
