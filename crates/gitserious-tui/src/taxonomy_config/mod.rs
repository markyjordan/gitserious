use std::path::PathBuf;

use crate::confirmation::{
    self, APPLY_OR_DISCARD, Answer, ConfirmDialog, Confirmation, DISCARD_AND_QUIT, REMOVE,
    REPLACE_OR_COPY,
};
use gitserious_app::{
    ConfigurationDestination, ConfigurationSession, ConfigurationWorkspace, CopyIn, DraftEdit,
    EditRefusal, LibraryDraft, NodeId, NodeRef, ProjectConfiguration, RepositoryRoot,
    TaxonomyOrigin,
};
use gitserious_core::{Taxonomy, TaxonomyId};
use ratatui::Frame;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::layout::{Constraint, Layout, Rect};
#[cfg(test)]
use ratatui::style::Color;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::theme::{
    FOOTER_HEIGHT, MINIMUM_HEIGHT, MINIMUM_WIDTH, SHELL_HINTS, block_title, centered_rect,
    focus_frame_style, frame_style, navigation_key_style, render_footer, render_footer_block,
    render_navigation_row, row_style, section_heading_style, selected_style, theme,
};

#[cfg(test)]
mod alert_tests;
#[cfg(test)]
mod copy_tests;
#[cfg(test)]
mod direct_tests;
mod home_overlay;
mod library_copy;
#[cfg(test)]
mod library_edit_tests;
mod library_header;
#[cfg(test)]
mod library_policy_tests;
#[cfg(test)]
mod library_rows_tests;
#[cfg(test)]
mod library_type_tests;
#[cfg(test)]
mod model_check;
mod naming;
#[cfg(test)]
mod project_editor_tests;
mod project_overview;
#[cfg(test)]
mod project_preview_tests;
mod review;
mod taxonomies;
#[cfg(test)]
mod tests;
use home_overlay::{CommandPaletteState, HomeCommand};
use naming::sentence;
use taxonomies::{LibraryAction, Nav, TaxonomiesView};

/// Configuration content hosted by the unified shell's terminal runtime.
pub(crate) struct ConfigView {
    state: State,
}

impl ConfigView {
    pub(crate) fn new(workspace: &dyn ConfigurationWorkspace) -> Self {
        let mut state = State::new(workspace);
        state.embedded = true;
        state.project_taxonomies = TaxonomiesView::read_only();
        Self { state }
    }

    pub(crate) fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.state.render_at(frame, area);
    }

    /// Routes requests requiring the shell's application services.
    pub(crate) fn handle_event(
        &mut self,
        event: Event,
        workspace: &dyn ConfigurationWorkspace,
    ) -> Option<ConfigRequest> {
        if self.state.handle_event(event, workspace) {
            Some(ConfigRequest::Quit)
        } else {
            self.state.shell_request.take()
        }
    }

    /// Which keys the shell may take before Config sees them.
    pub(crate) fn shell_keys(&self) -> ShellKeys {
        match (&self.state.screen, &self.state.home_overlay) {
            (Screen::Home, None) if self.state.editor().is_some() => {
                self.state.editor_view().shell_keys()
            }
            (Screen::Home, None) => ShellKeys::Navigation,
            _ => ShellKeys::None,
        }
    }

    pub(crate) fn modal(&self) -> bool {
        self.state.home_overlay.is_some() || self.state.editor_view().editing()
    }

    /// Seed a pending standalone-adapter review to exercise the shell's discard boundary.
    #[cfg(test)]
    pub(crate) fn stage_legacy_project_review(&mut self) {
        self.state
            .request_project_action(PendingAction::InitializeProject);
    }

    pub(crate) fn dirty(&self) -> bool {
        self.state.any_dirty() || self.state.settings.is_some()
    }
}

/// Actions the unified configuration view delegates to the shell.
pub(crate) enum ConfigRequest {
    Initialize,
    Quit,
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

    /// The rail row: the location underlined, or the filled cursor while the rail has focus.
    fn rail_row(self, here: bool, focused: bool, pending: bool) -> Paragraph<'static> {
        let label = format!(" {}  {}", self.index() + 1, self.label());
        let mut line = match (here, focused) {
            (true, false) => Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    label.trim_start().to_owned(),
                    Style::new()
                        .fg(theme().text)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
            ]),
            _ => Line::from(label),
        };
        if pending {
            line.push_span(Span::styled(" ●", Style::new().fg(theme().pending)));
        }
        Paragraph::new(line).style(match (here, focused) {
            (true, true) => Style::new()
                .fg(theme().ground)
                .bg(theme().cursor)
                .add_modifier(Modifier::BOLD),
            (true, false) => Style::new(),
            (false, _) => Style::new().fg(theme().dim),
        })
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
    Taxonomies,
    Files,
}

impl ProjectSection {
    const ALL: [Self; 3] = [Self::Commit, Self::Taxonomies, Self::Files];

    const fn index(self) -> usize {
        match self {
            Self::Commit => 0,
            Self::Taxonomies => 1,
            Self::Files => 2,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Commit => "Commit",
            Self::Taxonomies => "Taxonomies",
            Self::Files => "Files",
        }
    }
}

/// What Config > Project shows: its sections, or the outline over its own taxonomies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectView {
    Sections,
    Outline,
}

/// Which draft the taxonomy outline edits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Editor {
    Library,
    Project,
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
    /// Lists the Library's taxonomies to copy into the project.
    Import {
        selected: usize,
    },
    /// Asks whether a Library taxonomy replaces the project's different copy or joins as new.
    ImportConflict(TaxonomyId),
    /// Asks whether a project taxonomy replaces the Library's different one or joins as new.
    ExportConflict(TaxonomyId),
}

/// The Config rail's width at every terminal size, before the rule that ends it.
const RAIL_WIDTH: u16 = 13;
/// Where the content starts, right of the rail and its rule.
#[cfg(test)]
const CONTENT_X: u16 = RAIL_WIDTH + 1;
/// Terminals this tall pad the content with a blank row above and below.
const PADDED_HEIGHT: u16 = 24;

/// The blank rows above and below the rail and the Library on a `terminal` this size.
fn content_padding(terminal: Rect) -> u16 {
    u16::from(terminal.height >= PADDED_HEIGHT)
}

#[derive(Clone, Debug)]
struct SettingsDraft {
    selected: usize,
    default: TaxonomyId,
    available: Vec<TaxonomyId>,
}

struct State {
    embedded: bool,
    global: Option<ConfigurationSession>,
    global_error: String,
    project: Option<ConfigurationSession>,
    project_error: String,
    active_scope: ConfigurationDestination,
    category: Category,
    home_focus: HomeFocus,
    taxonomies: TaxonomiesView,
    /// The outline over the project's own taxonomies, shown while `project_view` is `Outline`.
    project_taxonomies: TaxonomiesView,
    project_view: ProjectView,
    project_section: ProjectSection,
    screen: Screen,
    home_overlay: Option<CommandPaletteState>,
    scroll: u16,
    project_scroll: u16,
    status: Option<Notice>,
    dialog: ConfirmDialog,
    settings: Option<SettingsDraft>,
    too_small: bool,
    shell_request: Option<ConfigRequest>,
    project_area: Rect,
    tab_areas: [Rect; 2],
    library_path: PathBuf,
    /// The directory the Library header shortens to `~`.
    home: Option<PathBuf>,
}

impl State {
    fn new(workspace: &dyn ConfigurationWorkspace) -> Self {
        let global_result = workspace.load(ConfigurationDestination::Global);
        let project_result = workspace.load(ConfigurationDestination::Project);
        let (global, global_error) = split_result(global_result);
        let (project, project_error) = split_result(project_result);
        Self {
            embedded: false,
            global,
            global_error,
            project,
            project_error,
            active_scope: ConfigurationDestination::Project,
            category: Category::Project,
            home_focus: HomeFocus::Menu,
            taxonomies: TaxonomiesView::default(),
            project_taxonomies: TaxonomiesView::default(),
            project_view: ProjectView::Sections,
            project_section: ProjectSection::Commit,
            screen: Screen::Home,
            home_overlay: None,
            scroll: 0,
            project_scroll: 0,
            status: None,
            dialog: ConfirmDialog::default(),
            settings: None,
            too_small: false,
            shell_request: None,
            project_area: Rect::default(),
            tab_areas: [Rect::default(); 2],
            library_path: workspace.library_path(),
            home: std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|home| home.is_absolute()),
        }
    }

    /// The question the current screen asks, if it is a confirmation. Its choices drive the
    /// dialog's keys and buttons and the footer's hints.
    fn confirmation(&self) -> Option<Confirmation<'static>> {
        let (title, message, choices) = match &self.screen {
            Screen::ConfirmRemove(node) => {
                let library = self.editor_draft()?;
                let owner = if library.is_project() {
                    "this project"
                } else {
                    "the Library"
                };
                match library.node(node)? {
                    NodeRef::Taxonomy(taxonomy) => (
                        "Delete Taxonomy",
                        format!("Delete taxonomy \"{}\" from {owner}?", taxonomy.name()),
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
            Screen::ImportConflict(id) => (
                "Different Taxonomy",
                format!(
                    "This project's \"{id}\" differs from the Library's. Replace the project copy, or add the Library's as a new taxonomy?"
                ),
                REPLACE_OR_COPY,
            ),
            Screen::ExportConflict(id) => (
                "Different Taxonomy",
                format!(
                    "The Library's \"{id}\" differs from this project's. Replace the Library's, or add the project's as a new taxonomy?"
                ),
                REPLACE_OR_COPY,
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
            (Screen::ImportConflict(id), Answer::Confirm) => {
                self.copy_into_project(&id, CopyIn::Replace);
            }
            (Screen::ImportConflict(id), Answer::Alternate) => {
                self.copy_into_project(&id, CopyIn::Renamed);
            }
            (Screen::ExportConflict(_), Answer::Confirm) => self.resolve_export(CopyIn::Replace),
            (Screen::ExportConflict(_), Answer::Alternate) => self.resolve_export(CopyIn::Renamed),
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

    /// The active outline: Library, the shell's installed project preview, or the standalone
    /// adapter's project editor while its taxonomies are open.
    fn editor(&self) -> Option<Editor> {
        match self.category {
            Category::Taxonomies => Some(Editor::Library),
            Category::Project
                if (self.embedded
                    && self
                        .project
                        .as_ref()
                        .is_some_and(|session| session.persisted_project_config().is_some()))
                    || (!self.embedded && self.project_view == ProjectView::Outline) =>
            {
                Some(Editor::Project)
            }
            Category::Project => None,
        }
    }

    fn editor_view(&self) -> &TaxonomiesView {
        match self.editor() {
            Some(Editor::Project) => &self.project_taxonomies,
            _ => &self.taxonomies,
        }
    }

    fn editor_draft(&self) -> Option<&LibraryDraft> {
        match self.editor()? {
            Editor::Library => self.library(),
            Editor::Project => self.project.as_ref()?.project_draft(),
        }
    }

    fn editor_parts_mut(&mut self) -> Option<(&mut TaxonomiesView, &mut LibraryDraft)> {
        match self.editor()? {
            Editor::Library => Some((&mut self.taxonomies, self.global.as_mut()?.library_mut()?)),
            Editor::Project => Some((
                &mut self.project_taxonomies,
                self.project.as_mut()?.project_draft_mut()?,
            )),
        }
    }

    fn save_editor_edit(&mut self, editor: Editor, workspace: &dyn ConfigurationWorkspace) {
        if editor == Editor::Project {
            self.project_taxonomies.finish_edit();
            self.status = None;
            return;
        }
        let Some(session) = self.global.as_ref() else {
            return;
        };
        let Some(library) = session.library() else {
            return;
        };
        let result = if session.is_dirty() {
            workspace
                .save(session)
                .map(|saved| saved.library().cloned())
        } else {
            library.saved().map(Some).map_err(|problems| {
                problems
                    .iter()
                    .map(|problem| naming::problem_sentence(library, problem))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        };
        match result {
            Ok(candidate) => {
                if let Some(candidate) = candidate
                    && let Some(library) = self
                        .global
                        .as_mut()
                        .and_then(ConfigurationSession::library_mut)
                {
                    *library = candidate;
                }
                self.taxonomies.finish_edit();
                self.status = None;
                if !self.scope_dirty(ConfigurationDestination::Project) {
                    self.replace_session(
                        ConfigurationDestination::Project,
                        workspace.load(ConfigurationDestination::Project),
                    );
                }
            }
            Err(error) => self.status = Some(Notice::error(error)),
        }
    }

    fn handle_event(&mut self, event: Event, workspace: &dyn ConfigurationWorkspace) -> bool {
        if self.too_small && self.editor_view().editing() {
            return false;
        }
        let before_global = self.global.clone();
        let before_view = self.taxonomies.clone();
        let before_screen = self.screen.clone();
        let quit = self.handle_event_inner(event, workspace);
        let immediate_change = !before_view.editing()
            && !self.taxonomies.editing()
            && self.scope_dirty(ConfigurationDestination::Global);
        if immediate_change && let Some(session) = self.global.as_ref() {
            let candidate = session.library().and_then(|library| library.saved().ok());
            let result = if session.is_dirty() {
                workspace
                    .save(session)
                    .map(|saved| saved.library().cloned())
            } else {
                Ok(candidate)
            };
            match result {
                Ok(candidate) => {
                    if let Some(candidate) = candidate
                        && let Some(library) = self
                            .global
                            .as_mut()
                            .and_then(ConfigurationSession::library_mut)
                    {
                        *library = candidate;
                    }
                    self.status = None;
                    if !self.scope_dirty(ConfigurationDestination::Project) {
                        self.replace_session(
                            ConfigurationDestination::Project,
                            workspace.load(ConfigurationDestination::Project),
                        );
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
                && let Some((view, library)) = self.editor_parts_mut()
                && let Err(error) = view.paste(text, library)
            {
                self.status = Some(Notice::error(error));
            }
            return false;
        }
        if let Event::Mouse(mouse) = event {
            if self.too_small
                || self.editor_view().picker_hints().is_some()
                || self.editor_view().editing()
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
                    && self.project_view == ProjectView::Sections
                    && self.editor().is_none()
                    && contains(self.project_area, mouse.column, mouse.row)
                {
                    if self.empty_project_overview() {
                        self.home_focus = HomeFocus::Content;
                        self.shell_request = Some(ConfigRequest::Initialize);
                        return false;
                    }
                    let index = usize::from(mouse.row.saturating_sub(self.project_area.y));
                    if let Some(section) = ProjectSection::ALL.get(index) {
                        self.home_focus = HomeFocus::Content;
                        self.project_section = *section;
                        self.project_scroll = 0;
                    }
                } else if matches!(self.screen, Screen::Home)
                    && let Some((view, library)) = self.editor_parts_mut()
                    && view.click(mouse.column, mouse.row, library)
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
            Screen::Import { selected } => {
                self.import_key(key, selected);
                false
            }
            Screen::Leave
            | Screen::ScopeChange(_)
            | Screen::ImportConflict(_)
            | Screen::ExportConflict(_) => false,
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
            HomeCommand::Initialize if self.empty_project_overview() => {
                self.shell_request = Some(ConfigRequest::Initialize);
            }
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

    fn home_key(&mut self, key: KeyEvent, workspace: &dyn ConfigurationWorkspace) -> bool {
        if self.empty_project_overview()
            && matches!(key.code, KeyCode::Enter | KeyCode::Char('i'))
            && key.modifiers.is_empty()
        {
            self.shell_request = Some(ConfigRequest::Initialize);
            return false;
        }
        if self.embedded && self.category == Category::Project && key.code == KeyCode::Enter {
            self.home_focus = HomeFocus::Content;
        }
        if self.embedded
            && self.category == Category::Project
            && (matches!(
                key.code,
                KeyCode::Char('i' | 'l' | 'e' | 'f' | 'n' | 'd' | 'c')
            ) || (key.code == KeyCode::Char('s')
                && key.modifiers.contains(KeyModifiers::CONTROL)))
        {
            return false;
        }
        let editor = self.editor();
        if let Some(editor) = editor
            && self.editor_view().editing()
        {
            if let Some((view, library)) = self.editor_parts_mut() {
                match view.edit_key(key, library) {
                    Ok(taxonomies::EditOutcome::SaveRequested) => {
                        self.save_editor_edit(editor, workspace)
                    }
                    Ok(taxonomies::EditOutcome::Cancelled) => self.status = None,
                    Ok(taxonomies::EditOutcome::Continue) => {}
                    Err(error) => self.status = Some(Notice::error(error)),
                }
            }
            return false;
        }
        if let Some(editor) = editor
            && self.home_focus == HomeFocus::Content
            && let Some((view, library)) = self.editor_parts_mut()
        {
            match view.key(key, library) {
                Nav::Handled => return false,
                Nav::Sidebar if editor == Editor::Project && !self.embedded => {
                    self.project_view = ProjectView::Sections;
                    return false;
                }
                Nav::Sidebar => {
                    self.home_focus = HomeFocus::Menu;
                    return false;
                }
                Nav::Act(action) => {
                    self.request_editor_action(editor, PendingAction::Library(action));
                    return false;
                }
                Nav::Unhandled => {}
            }
        }
        match key.code {
            KeyCode::Char('/') if key.modifiers.is_empty() => {
                self.home_overlay = Some(CommandPaletteState {
                    project_preview: self.embedded.then(|| !self.empty_project_overview()),
                    ..CommandPaletteState::default()
                });
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
            KeyCode::Up | KeyCode::Down if self.category == Category::Project && !self.embedded => {
                let index = self.project_section.index();
                let index = match key.code {
                    KeyCode::Up => index.saturating_sub(1),
                    _ => (index + 1).min(ProjectSection::ALL.len() - 1),
                };
                self.project_section = ProjectSection::ALL[index];
                self.project_scroll = 0;
            }
            KeyCode::PageUp if self.category == Category::Project && !self.embedded => {
                self.project_scroll = self.project_scroll.saturating_sub(8);
            }
            KeyCode::PageDown if self.category == Category::Project && !self.embedded => {
                self.project_scroll = self.project_scroll.saturating_add(8);
            }
            KeyCode::Enter if self.category == Category::Project && !self.embedded => match self
                .project_section
            {
                ProjectSection::Commit => self.request_project_action(PendingAction::EditProject),
                ProjectSection::Taxonomies => self.open_project_taxonomies(),
                ProjectSection::Files => {}
            },
            KeyCode::Char('i') if editor == Some(Editor::Project) && key.modifiers.is_empty() => {
                self.open_import();
            }
            KeyCode::Char('l') if editor == Some(Editor::Project) && key.modifiers.is_empty() => {
                self.copy_to_library();
            }
            KeyCode::Char('i') if self.category == Category::Project => {
                self.request_project_action(PendingAction::InitializeProject);
            }
            KeyCode::Char(letter @ ('e' | 'f' | 'n' | 'd')) if key.modifiers.is_empty() => {
                if let Some(editor) = editor {
                    self.request_editor_action(
                        editor,
                        match letter {
                            'e' => PendingAction::Library(LibraryAction::Edit),
                            'f' => PendingAction::Library(LibraryAction::Fork),
                            'n' => PendingAction::Library(LibraryAction::Create),
                            _ => PendingAction::Delete,
                        },
                    );
                }
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
        let (default, available) = match destination {
            ConfigurationDestination::Global => {
                let Some(library) = session.library() else {
                    return;
                };
                (
                    library.default_taxonomy().clone(),
                    library.available_taxonomies().to_vec(),
                )
            }
            ConfigurationDestination::Project => {
                let Some(config) = session.project_config() else {
                    return;
                };
                (
                    config.default_taxonomy().clone(),
                    config
                        .taxonomies()
                        .iter()
                        .map(|taxonomy| taxonomy.id().clone())
                        .collect(),
                )
            }
        };
        let selected = self
            .settings_rows(destination)
            .iter()
            .position(|taxonomy| taxonomy.id() == &default)
            .unwrap_or(0);
        self.settings = Some(SettingsDraft {
            selected,
            default,
            available,
        });
        self.screen = Screen::Settings(destination);
    }

    fn settings_key(&mut self, key: KeyEvent, destination: ConfigurationDestination) -> bool {
        let rows = self.settings_rows(destination);
        let count = rows.len();
        let selected_id = self
            .settings
            .as_ref()
            .and_then(|settings| rows.get(settings.selected))
            .map(|item| item.id().clone());
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
                    settings.default = id.clone();
                    if !settings.available.contains(&id) {
                        settings.available.push(id);
                    }
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
                            session.configure_project(settings.default.clone(), &settings.available)
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
        let Some((view, library)) = self.editor_parts_mut() else {
            return;
        };
        match library.edit(node, DraftEdit::Remove) {
            Ok(landing) => {
                view.select(landing);
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

    /// Routes an outline action to the Library or, while its taxonomies are open, the project.
    fn request_editor_action(&mut self, editor: Editor, action: PendingAction) {
        match editor {
            Editor::Library => self.request_global_action(action),
            Editor::Project if !self.embedded => self.project_edit(action),
            Editor::Project => {}
        }
    }

    /// Opens the outline over the project's own taxonomies.
    fn open_project_taxonomies(&mut self) {
        if self
            .project
            .as_ref()
            .and_then(ConfigurationSession::project_draft)
            .is_none()
        {
            self.status = Some(Notice::error(
                "Initialize the project before editing its taxonomies.".into(),
            ));
            return;
        }
        self.active_scope = ConfigurationDestination::Project;
        self.project_view = ProjectView::Outline;
        self.home_focus = HomeFocus::Content;
    }

    /// Stages an outline action on the project draft. Nothing is written until Apply.
    fn project_edit(&mut self, action: PendingAction) {
        self.active_scope = ConfigurationDestination::Project;
        self.status = None;
        match action {
            PendingAction::Library(action) => {
                if let Some((view, draft)) = self.editor_parts_mut()
                    && let Err(error) = view.act(action, draft)
                {
                    self.status = Some(Notice::error(error));
                }
            }
            PendingAction::Delete => {
                let Some(draft) = self.editor_draft() else {
                    return;
                };
                let Some(node) = self.project_taxonomies.selected(draft) else {
                    return;
                };
                if let Some(blocker) = draft.removal_blocker(node.id()) {
                    self.status = Some(Notice::error(format!(
                        "{blocker}. Choose another default before deleting it."
                    )));
                    return;
                }
                self.screen = Screen::ConfirmRemove(node.id().clone());
            }
            _ => {}
        }
    }

    fn request_global_action(&mut self, action: PendingAction) {
        if self.global.is_none() {
            self.status = Some(Notice::error(self.library_error()));
            return;
        }
        // Only creating applies on the new taxonomy row, which selects no node.
        if !matches!(action, PendingAction::Library(LibraryAction::Create))
            && self
                .library()
                .is_some_and(|library| self.taxonomies.selected(library).is_none())
        {
            self.status = Some(Notice::error(naming::refusal_sentence(
                EditRefusal::NotApplicable,
                None,
            )));
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
                let Some(library) = self.library().and_then(LibraryDraft::persisted).cloned()
                else {
                    self.status = Some(Notice::error(format!(
                        "Initializing copies taxonomies from the Library. {}",
                        self.library_error()
                    )));
                    return;
                };
                let Some(session) = &mut self.project else {
                    self.status = Some(Notice::error(self.project_error.clone()));
                    return;
                };
                match session
                    .initialize_project(&library)
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

    /// The taxonomies a settings screen lists: the Library for Global, the project's own for Project.
    fn settings_rows(&self, destination: ConfigurationDestination) -> Vec<Taxonomy> {
        match destination {
            ConfigurationDestination::Global => self.catalog().unwrap_or_default(),
            ConfigurationDestination::Project => self
                .project
                .as_ref()
                .and_then(|session| {
                    session
                        .persisted_project_config()
                        .cloned()
                        .or_else(|| session.project_config())
                })
                .map(|config| config.taxonomies().to_vec())
                .unwrap_or_default(),
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

    #[cfg(test)]
    fn render(&mut self, frame: &mut Frame<'_>) {
        self.render_at(frame, frame.area());
    }

    fn render_at(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.dialog.invalidate();
        frame.render_widget(
            Block::default().style(Style::default().bg(theme().ground).fg(theme().text)),
            area,
        );
        let terminal = frame.area();
        self.too_small = terminal.width < MINIMUM_WIDTH || terminal.height < MINIMUM_HEIGHT;
        if self.too_small {
            self.tab_areas = [Rect::default(); 2];
            if let Some(question) = self.confirmation() {
                self.dialog.render(frame, &question);
            } else {
                let message = if self.editor_view().editing() {
                    "Terminal too small\n\nResize to continue editing"
                } else {
                    "Terminal too small\n\nResize or press esc/q to cancel"
                };
                frame.render_widget(
                    Paragraph::new(message).centered(),
                    centered_rect(54, 3, area),
                );
            }
            return;
        }
        let [main, footer] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(FOOTER_HEIGHT)]).areas(area);
        let footer = render_footer_block(frame, footer);
        let [rail, rule, content] = Layout::horizontal([
            Constraint::Length(RAIL_WIDTH),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas(main);
        self.render_rail(frame, rail);
        frame.render_widget(
            Block::new()
                .borders(Borders::LEFT)
                .border_style(Style::new().fg(theme().dim)),
            rule,
        );
        let body = content;
        let hints = if let Some(question) = self.confirmation() {
            if self.editor().is_some() {
                self.render_taxonomies(frame, body);
            } else {
                self.render_project(frame, body);
            }
            self.dialog.render(frame, &question);
            confirmation::hints(question.choices)
        } else {
            match self.screen.clone() {
                Screen::Home if self.editor().is_some() => {
                    self.render_taxonomies(frame, body);
                    let line = self.editor_view().status_line(
                        self.editor_draft(),
                        self.embedded,
                        footer.width,
                    );
                    frame.render_widget(Paragraph::new(line), footer);
                    Vec::new()
                }
                Screen::Home => {
                    self.render_project(frame, body);
                    let hints = if self.empty_project_overview() {
                        [("enter", "initialize"), ("←/→", "focus")]
                    } else {
                        [("←/→", "focus"), ("↑/↓", "move")]
                    };
                    if self.embedded {
                        render_footer(frame, footer, &hints, &SHELL_HINTS);
                        Vec::new()
                    } else {
                        vec![hints[0], hints[1], ("/", "commands")]
                    }
                }
                Screen::Settings(destination) => {
                    self.render_settings(frame, body, destination);
                    vec![
                        ("space", "available"),
                        ("enter", "default"),
                        ("alt+↑/↓", "reorder"),
                        ("ctrl+s", "stage"),
                        ("esc", "back"),
                    ]
                }
                Screen::Review(destination) => self.render_review(frame, body, destination),
                Screen::Import { selected } => {
                    self.render_import(frame, body, selected);
                    vec![
                        ("↑/↓", "move"),
                        ("enter", "copy into project"),
                        ("esc", "back"),
                    ]
                }
                Screen::Leave
                | Screen::ScopeChange(_)
                | Screen::ConfirmRemove(_)
                | Screen::ImportConflict(_)
                | Screen::ExportConflict(_) => Vec::new(),
            }
        };
        // Screens that draw their own footer leave no hints.
        if !hints.is_empty() {
            render_navigation_row(frame, footer, &hints);
        }
        if let Some(overlay) = &mut self.home_overlay {
            overlay.render(frame, self.category);
        } else {
            self.render_notice(frame, body);
        }
        if self.editor_view().editing() {
            crate::theme::dim_region(frame, rail);
            crate::theme::dim_region(frame, rule);
        }
    }

    fn render_rail(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.tab_areas = [Rect::default(); 2];
        let home = matches!(self.screen, Screen::Home);
        let focused = home && self.home_focus == HomeFocus::Menu;
        let pending = self.scope_dirty(ConfigurationDestination::Project);
        let top = area.y + content_padding(frame.area());
        for (category, y) in Category::ALL.into_iter().zip(top..area.bottom()) {
            let row = Rect::new(area.x, y, area.width, 1);
            frame.render_widget(
                category.rail_row(
                    category == self.category,
                    focused,
                    pending && category == Category::Project,
                ),
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
                Category::Project => self.project.is_none().then(|| self.project_error.clone()),
                Category::Taxonomies => self.global.is_none().then(|| self.library_error()),
            }
        } else {
            None
        };
        let (message, style) = match (&self.status, load_error) {
            (Some(Notice::Error(message)), _) => {
                (message.clone(), Style::default().fg(theme().pending))
            }
            (Some(Notice::Info(message)), _) => {
                (message.clone(), Style::default().fg(theme().text))
            }
            (None, Some(message)) => (sentence(&message), Style::default().fg(theme().pending)),
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
        frame.render_widget(crate::theme::popup_block(), popup);
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

    fn empty_project_overview(&self) -> bool {
        self.embedded
            && self.category == Category::Project
            && self
                .project
                .as_ref()
                .is_some_and(|session| session.persisted_project_config().is_none())
    }

    fn render_project(&mut self, frame: &mut Frame<'_>, area: Rect) {
        if self.empty_project_overview()
            && let Some(session) = &self.project
        {
            self.project_area =
                project_overview::render_empty(frame, area, session, self.home.as_deref());
            return;
        }
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
        let status = project_status(session);
        let guidance = project_guidance(status);
        let Some(root) = session.root() else {
            return "Repository unavailable.".into();
        };
        let path = root.as_path().join("gitserious.toml");
        if session.project_draft().is_none() {
            return format!(
                "Commit taxonomy\n  None\n\nProject configuration\n  {}\n  Absent\n\nStatus\n  {status}\n  {guidance}",
                path.display()
            );
        }
        let Some(config) = session
            .project_config()
            .or_else(|| session.persisted_project_config().cloned())
        else {
            return format!("Status\n  {status}\n  {guidance}");
        };
        let config = &config;
        let ids = |config: &ProjectConfiguration| {
            config
                .taxonomies()
                .iter()
                .map(|taxonomy| taxonomy.id().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        match self.project_section {
            ProjectSection::Commit => {
                let applied = session.persisted_project_config().unwrap_or(config);
                let staged = if session.is_dirty() {
                    format!(
                        "\n\nStaged\n  Default: {}\n  Available: {}",
                        config.default_taxonomy(),
                        ids(config)
                    )
                } else {
                    String::new()
                };
                format!(
                    "Commit taxonomy\n  {}\n\nAvailable taxonomies\n  {}{staged}\n\nSource\n  gitserious.toml only; the Library is not consulted\n\nStatus\n  {status}\n  {guidance}",
                    applied.default_taxonomy(),
                    ids(applied),
                )
            }
            ProjectSection::Taxonomies => {
                let entries = config
                    .taxonomies()
                    .iter()
                    .map(|taxonomy| {
                        format!(
                            "{}\n  {}\n  Types: {}",
                            taxonomy.reference(),
                            taxonomy.description(),
                            taxonomy
                                .commit_types()
                                .iter()
                                .map(|definition| definition.id().to_string())
                                .collect::<Vec<_>>()
                                .join(", "),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n");
                format!(
                    "{entries}\n\nEdit\n  Enter opens these taxonomies; Ctrl+S reviews changes before applying.\n\nStatus\n  {status}"
                )
            }
            ProjectSection::Files => format!(
                "Project configuration\n  {}\n  {}\n\nStatus\n  {status}\n  {guidance}",
                path.display(),
                if session.persisted_project_config().is_some() {
                    "Present"
                } else {
                    "Absent"
                },
            ),
        }
    }

    fn render_taxonomies(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let focused = matches!(self.screen, Screen::Home) && self.home_focus == HomeFocus::Content;
        if self.editor() == Some(Editor::Project) {
            let draft = self
                .project
                .as_ref()
                .and_then(ConfigurationSession::project_draft);
            let header = self.project_taxonomies.render(
                frame,
                area,
                if self.embedded {
                    library_header::HEIGHT
                } else {
                    0
                },
                draft,
                focused,
            );
            if self.embedded
                && let Some(session) = &self.project
            {
                project_overview::render_header(frame, header, session, self.home.as_deref());
            }
        } else {
            let library = self.global.as_ref().and_then(ConfigurationSession::library);
            let header =
                self.taxonomies
                    .render(frame, area, library_header::HEIGHT, library, focused);
            library_header::render(frame, header, &self.library_path, self.home.as_deref());
            if self.taxonomies.editing() {
                crate::theme::dim_region(frame, header);
            }
        }
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
        let items = self.settings_rows(destination);
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
        let inheritance = match destination {
            ConfigurationDestination::Project => "taxonomies stored in gitserious.toml",
            ConfigurationDestination::Global => "global commit environment",
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
    if session.persisted_project_config().is_none() {
        if session.project_config().is_some() {
            "Initialization staged"
        } else {
            "Not initialized"
        }
    } else if session.project_config().is_none() {
        "Invalid"
    } else if session.is_dirty() {
        "Changes staged"
    } else {
        "Valid"
    }
}

fn project_guidance(status: &str) -> &'static str {
    match status {
        "Not initialized" => "Press i to initialize and review.",
        "Initialization staged" => "Review and apply initialization with Ctrl+S.",
        "Changes staged" => "Review Ctrl+S; commits use gitserious.toml as last applied.",
        "Invalid" => {
            "Fix the problems shown in Taxonomies; commits use gitserious.toml as last applied."
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
