use std::path::PathBuf;

use crate::confirmation::{self, Answer, ConfirmDialog, Confirmation, REMOVE};
use gitserious_app::{
    ConfigurationWorkspace, DraftEdit, EditRefusal, LibraryDraft, NodeId, NodeRef, ProjectSnapshot,
    RepositoryRoot, TaxonomyOrigin,
};
#[cfg(test)]
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
    focus_frame_style, frame_style, render_footer, render_footer_block, render_navigation_row,
    section_heading_style, selected_style, theme,
};

#[cfg(test)]
mod alert_tests;
#[cfg(test)]
mod copy_tests;
#[cfg(test)]
mod direct_tests;
mod home_overlay;
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
mod project_overview;
#[cfg(test)]
mod project_preview_tests;
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
        Self {
            state: State::new(workspace),
        }
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
    fn rail_row(self, here: bool, focused: bool) -> Paragraph<'static> {
        let label = format!(" {}  {}", self.index() + 1, self.label());
        let line = match (here, focused) {
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

/// Which draft the taxonomy outline edits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Editor {
    Library,
    Project,
}

#[derive(Clone, Copy, Debug)]
enum PendingAction {
    Library(LibraryAction),
    Delete,
}

enum Notice {
    Error(String),
}

impl Notice {
    fn error(message: String) -> Self {
        Self::Error(sentence(&message))
    }
}

#[derive(Clone, Debug)]
enum Screen {
    Home,
    /// Asks before a taxonomy, type, or property leaves the Library.
    ConfirmRemove(NodeId),
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

struct State {
    /// The working Library; it holds the configuration it was loaded from.
    global: Option<LibraryDraft>,
    global_error: String,
    project: Option<ProjectSnapshot>,
    /// The project's taxonomies as a draft the read-only outline browses.
    project_outline: Option<LibraryDraft>,
    project_error: String,
    category: Category,
    home_focus: HomeFocus,
    taxonomies: TaxonomiesView,
    /// The read-only outline over the project's installed taxonomies.
    project_taxonomies: TaxonomiesView,
    project_section: ProjectSection,
    screen: Screen,
    home_overlay: Option<CommandPaletteState>,
    status: Option<Notice>,
    dialog: ConfirmDialog,
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
        let (global, global_error) = split_result(workspace.load_library());
        let (project, project_error) = split_result(workspace.load_project());
        Self {
            global,
            global_error,
            project_outline: outline(project.as_ref()),
            project,
            project_error,
            category: Category::Project,
            home_focus: HomeFocus::Menu,
            taxonomies: TaxonomiesView::default(),
            project_taxonomies: TaxonomiesView::read_only(),
            project_section: ProjectSection::Commit,
            screen: Screen::Home,
            home_overlay: None,
            status: None,
            dialog: ConfirmDialog::default(),
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
            Screen::Home => return None,
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
    ) -> bool {
        let Some(answer) = self.dialog.handle_event(choices, event) else {
            return false;
        };
        self.dialog.invalidate();
        match (self.screen.clone(), answer) {
            (Screen::ConfirmRemove(node), Answer::Confirm) => self.remove(&node),
            _ => self.screen = Screen::Home,
        }
        false
    }

    /// The active outline: Library, or the shell's installed project preview.
    fn editor(&self) -> Option<Editor> {
        match self.category {
            Category::Taxonomies => Some(Editor::Library),
            Category::Project if self.project_outline.is_some() => Some(Editor::Project),
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
            Editor::Project => self.project_outline.as_ref(),
        }
    }

    fn editor_parts_mut(&mut self) -> Option<(&mut TaxonomiesView, &mut LibraryDraft)> {
        match self.editor()? {
            Editor::Library => Some((&mut self.taxonomies, self.global.as_mut()?)),
            Editor::Project => Some((&mut self.project_taxonomies, self.project_outline.as_mut()?)),
        }
    }

    fn save_editor_edit(&mut self, editor: Editor, workspace: &dyn ConfigurationWorkspace) {
        if editor == Editor::Project {
            self.project_taxonomies.finish_edit();
            self.status = None;
            return;
        }
        let Some(library) = self.global.as_ref() else {
            return;
        };
        let result = if library.is_changed() {
            workspace.save_library(library).map(Some)
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
                    && let Some(library) = self.global.as_mut()
                {
                    *library = candidate;
                }
                self.taxonomies.finish_edit();
                self.status = None;
                self.reload_project(workspace);
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
        let immediate_change =
            !before_view.editing() && !self.taxonomies.editing() && self.library_dirty();
        if immediate_change && let Some(library) = self.global.as_ref() {
            let candidate = library.saved().ok();
            let result = if library.is_changed() {
                workspace.save_library(library).map(Some)
            } else {
                Ok(candidate)
            };
            match result {
                Ok(candidate) => {
                    if let Some(candidate) = candidate
                        && let Some(library) = self.global.as_mut()
                    {
                        *library = candidate;
                    }
                    self.status = None;
                    self.reload_project(workspace);
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
            return self.handle_home_overlay(event);
        }
        if let Some(question) = self.confirmation() {
            return self.handle_confirmation_event(question.choices, &event);
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
            Screen::ConfirmRemove(_) => {
                // The node is gone, so there is nothing left to confirm.
                self.screen = Screen::Home;
                false
            }
        }
    }

    fn handle_home_overlay(&mut self, event: Event) -> bool {
        let Some(overlay) = self.home_overlay.take() else {
            return false;
        };
        self.handle_palette_event(event, overlay)
    }

    fn handle_palette_event(&mut self, event: Event, mut palette: CommandPaletteState) -> bool {
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
                        return self.dispatch_home_command(command);
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

    fn dispatch_home_command(&mut self, command: HomeCommand) -> bool {
        match command {
            HomeCommand::Initialize => {
                if self.empty_project_overview() {
                    self.shell_request = Some(ConfigRequest::Initialize);
                }
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
            // The shell confirms a retained commit review before it quits.
            HomeCommand::Quit => return true,
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
        if self.category == Category::Project && key.code == KeyCode::Enter {
            self.home_focus = HomeFocus::Content;
        }
        if self.category == Category::Project
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
                    project_preview: Some(!self.empty_project_overview()),
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
            KeyCode::Esc | KeyCode::Char('q') => return true,
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

    /// Reads the project again, since a Library save can follow another process's init.
    fn reload_project(&mut self, workspace: &dyn ConfigurationWorkspace) {
        let (project, error) = split_result(workspace.load_project());
        self.project_outline = outline(project.as_ref());
        self.project = project;
        self.project_error = error;
    }

    /// Routes an outline action to the Library. The project preview takes none.
    fn request_editor_action(&mut self, editor: Editor, action: PendingAction) {
        match editor {
            Editor::Library => self.request_global_action(action),
            Editor::Project => {}
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
        self.perform_pending_action(action);
    }

    fn perform_pending_action(&mut self, action: PendingAction) {
        self.screen = Screen::Home;
        match action {
            PendingAction::Library(action) => {
                self.category = Category::Taxonomies;
                self.home_focus = HomeFocus::Content;
                if let Some(library) = self.global.as_mut()
                    && let Err(error) = self.taxonomies.act(action, library)
                {
                    self.status = Some(Notice::error(error));
                }
            }
            PendingAction::Delete => {
                if let Some(node) = self
                    .library()
                    .and_then(|library| self.taxonomies.selected(library))
                {
                    self.screen = Screen::ConfirmRemove(node.id().clone());
                }
            }
        }
    }

    fn library_error(&self) -> String {
        if self.global_error.is_empty() {
            "The Library is unavailable.".into()
        } else {
            self.global_error.clone()
        }
    }

    /// The working Library.
    fn library(&self) -> Option<&LibraryDraft> {
        self.global.as_ref()
    }

    /// Whether the Library differs from what was last loaded or saved.
    fn library_dirty(&self) -> bool {
        self.global.as_ref().is_some_and(LibraryDraft::is_changed)
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
                    let line = self
                        .editor_view()
                        .status_line(self.editor_draft(), footer.width);
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
                    render_footer(frame, footer, &hints, &SHELL_HINTS);
                    Vec::new()
                }
                Screen::ConfirmRemove(_) => Vec::new(),
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
        let top = area.y + content_padding(frame.area());
        for (category, y) in Category::ALL.into_iter().zip(top..area.bottom()) {
            let row = Rect::new(area.x, y, area.width, 1);
            frame.render_widget(category.rail_row(category == self.category, focused), row);
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
        self.category == Category::Project
            && self
                .project
                .as_ref()
                .is_some_and(|project| project.configuration().is_none())
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
            .map(ProjectSnapshot::root)
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
        let paragraph = Paragraph::new(self.project_detail()).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, detail);
    }

    /// The section pane's text. Sections show only while the project cannot load.
    fn project_detail(&self) -> String {
        self.project_error.clone()
    }

    fn render_taxonomies(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let focused = matches!(self.screen, Screen::Home) && self.home_focus == HomeFocus::Content;
        if self.editor() == Some(Editor::Project) {
            let draft = self.project_outline.as_ref();
            let header =
                self.project_taxonomies
                    .render(frame, area, library_header::HEIGHT, draft, focused);
            if let Some(session) = &self.project {
                project_overview::render_header(frame, header, session, self.home.as_deref());
            }
        } else {
            let library = self.global.as_ref();
            let header =
                self.taxonomies
                    .render(frame, area, library_header::HEIGHT, library, focused);
            library_header::render(frame, header, &self.library_path, self.home.as_deref());
            if self.taxonomies.editing() {
                crate::theme::dim_region(frame, header);
            }
        }
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

fn split_result<T>(result: Result<T, String>) -> (Option<T>, String) {
    match result {
        Ok(value) => (Some(value), String::new()),
        Err(error) => (None, error),
    }
}

/// The draft the read-only project outline browses.
fn outline(project: Option<&ProjectSnapshot>) -> Option<LibraryDraft> {
    project?
        .configuration()
        .cloned()
        .map(LibraryDraft::from_project)
}

fn contains(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x && x < area.right() && y >= area.y && y < area.bottom()
}
