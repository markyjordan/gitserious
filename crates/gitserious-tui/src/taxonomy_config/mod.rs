use std::io::{self, IsTerminal};
use std::path::PathBuf;

use gitserious_app::{
    ConfigurationDestination, ConfigurationEditor, ConfigurationSession, ConfigurationWorkspace,
    ProjectConfig, RepositoryRoot, TaxonomyOrigin, resolve_effective_configuration,
    revised_taxonomy,
};
use gitserious_core::{
    CommitTypeDefinition, CommitTypeId, ConditionId, Description, PropertyCondition,
    PropertyDefinition, PropertyKey, PropertyMultiplicity, PropertyRequirement, SchemaVersion,
    Taxonomy, TaxonomyId, TaxonomyLineage, TaxonomyVersion,
};
use ratatui::Frame;
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::theme::{
    JET_BLACK, MINIMUM_HEIGHT, MINIMUM_WIDTH, ZEBRA_BACKGROUND, centered_rect, frame_style,
    navigation_key_style, normalize_background, render_navigation_row, section_heading_style,
};

mod home_overlay;
mod library_view;
mod scrollbar;
use home_overlay::{CommandPaletteState, HomeCommand, HomeOverlay};
use library_view::{LibraryFocus, LibraryViewState};
use scrollbar::render_overflow_scrollbar;

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
        ratatui::run(|terminal| {
            let _guard = TerminalGuard::enable()?;
            let mut state = State::new(workspace);
            state.library_path.clone_from(&self.library_path);
            loop {
                terminal.draw(|frame| state.render(frame))?;
                if state.handle_event(event::read()?, workspace) {
                    return Ok::<_, io::Error>(());
                }
            }
        })
        .map_err(|error| format!("configuration editing failed: {error}"))
    }
}

struct TerminalGuard;

impl TerminalGuard {
    fn enable() -> io::Result<Self> {
        ratatui::crossterm::execute!(
            io::stdout(),
            event::EnableMouseCapture,
            event::EnableBracketedPaste
        )?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = ratatui::crossterm::execute!(
            io::stdout(),
            event::DisableBracketedPaste,
            event::DisableMouseCapture
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Category {
    Project,
    Library,
}

impl Category {
    const ALL: [Self; 2] = [Self::Project, Self::Library];

    const fn index(self) -> usize {
        match self {
            Self::Project => 0,
            Self::Library => 1,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Library => "Library",
        }
    }

    const fn next(self) -> Self {
        match self {
            Self::Project => Self::Library,
            Self::Library => Self::Project,
        }
    }
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
    Create,
    Edit,
    Fork,
    Delete,
    ReviewGlobal,
}

enum Notice {
    Error(String),
    Info(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditorMode {
    Create,
    Edit,
}

#[derive(Clone, Debug)]
enum Screen {
    Home,
    Settings(ConfigurationDestination),
    TaxonomyEditor(EditorMode),
    TypeEditor(usize),
    Form(FormKind),
    Fork,
    Delete,
    Review(ConfigurationDestination),
    Leave,
    ScopeChange(PendingAction),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FormKind {
    TaxonomyMetadata,
    NewType,
    EditType(usize),
    NewProperty(usize),
    EditProperty(usize, usize),
}

#[derive(Clone, Debug)]
struct PropertyDraft {
    key: String,
    description: String,
    requirement: PropertyRequirement,
    multiplicity: PropertyMultiplicity,
}

impl From<&PropertyDefinition> for PropertyDraft {
    fn from(value: &PropertyDefinition) -> Self {
        Self {
            key: value.key().to_string(),
            description: value.description().to_owned(),
            requirement: value.requirement().clone(),
            multiplicity: value.multiplicity(),
        }
    }
}

#[derive(Clone, Debug)]
struct TypeDraft {
    id: String,
    description: String,
    schema_version: u16,
    properties: Vec<PropertyDraft>,
}

impl From<&CommitTypeDefinition> for TypeDraft {
    fn from(value: &CommitTypeDefinition) -> Self {
        Self {
            id: value.id().to_string(),
            description: value.description().to_owned(),
            schema_version: value.schema_version().get(),
            properties: value.properties().iter().map(PropertyDraft::from).collect(),
        }
    }
}

#[derive(Clone, Debug)]
struct TaxonomyDraft {
    id: String,
    description: String,
    version: u16,
    derived_from: Option<TaxonomyLineage>,
    types: Vec<TypeDraft>,
}

impl TaxonomyDraft {
    fn empty() -> Self {
        Self {
            id: String::new(),
            description: String::new(),
            version: 1,
            derived_from: None,
            types: Vec::new(),
        }
    }

    fn from_taxonomy(value: &Taxonomy) -> Self {
        Self {
            id: value.id().to_string(),
            description: value.description().to_string(),
            version: value.version().get(),
            derived_from: value.derived_from().cloned(),
            types: value.commit_types().iter().map(TypeDraft::from).collect(),
        }
    }

    fn build(&self, mode: EditorMode) -> Result<Taxonomy, String> {
        let id = TaxonomyId::new(&self.id).map_err(|error| error.to_string())?;
        let description = Description::new(&self.description).map_err(|error| error.to_string())?;
        let version = match mode {
            EditorMode::Create => TaxonomyVersion::V1,
            EditorMode::Edit => TaxonomyVersion::new(self.version.saturating_add(1))
                .map_err(|error| error.to_string())?,
        };
        let types = self
            .types
            .iter()
            .map(TypeDraft::build)
            .collect::<Result<Vec<_>, _>>()?;
        Taxonomy::new(id, version, description, self.derived_from.clone(), types)
            .map_err(|error| error.to_string())
    }
}

impl TypeDraft {
    fn build(&self) -> Result<CommitTypeDefinition, String> {
        let id = CommitTypeId::new(&self.id).map_err(|error| error.to_string())?;
        let version = SchemaVersion::new(self.schema_version).map_err(|error| error.to_string())?;
        let properties = self
            .properties
            .iter()
            .map(PropertyDraft::build)
            .collect::<Result<Vec<_>, _>>()?;
        CommitTypeDefinition::new(version, id, self.description.clone(), properties)
            .map_err(|error| error.to_string())
    }
}

impl PropertyDraft {
    fn empty() -> Self {
        Self {
            key: String::new(),
            description: String::new(),
            requirement: PropertyRequirement::Required,
            multiplicity: PropertyMultiplicity::Single,
        }
    }

    fn build(&self) -> Result<PropertyDefinition, String> {
        let key = PropertyKey::new(&self.key).map_err(|error| error.to_string())?;
        PropertyDefinition::new(
            key,
            self.description.clone(),
            self.requirement.clone(),
            self.multiplicity,
        )
        .map_err(|error| error.to_string())
    }
}

#[derive(Clone, Debug)]
struct FormState {
    fields: Vec<String>,
    selected: usize,
    requirement: PropertyRequirement,
    multiplicity: PropertyMultiplicity,
}

impl FormState {
    fn metadata(id: String, description: String) -> Self {
        Self {
            fields: vec![id, description],
            selected: 0,
            requirement: PropertyRequirement::Required,
            multiplicity: PropertyMultiplicity::Single,
        }
    }

    fn property(value: PropertyDraft) -> Self {
        let (condition, rationale) = match &value.requirement {
            PropertyRequirement::Conditional(condition) => {
                (condition.id().to_string(), condition.rationale().to_owned())
            }
            _ => (String::new(), String::new()),
        };
        Self {
            fields: vec![value.key, value.description, condition, rationale],
            selected: 0,
            requirement: value.requirement,
            multiplicity: value.multiplicity,
        }
    }
}

#[derive(Clone, Debug)]
struct SettingsDraft {
    selected: usize,
    default: TaxonomyId,
    available: Vec<TaxonomyId>,
    inherit_default: bool,
    inherit_available: bool,
}

struct State {
    library_path: Option<PathBuf>,
    global: Option<ConfigurationSession>,
    global_error: String,
    project: Option<ConfigurationSession>,
    project_error: String,
    active_scope: ConfigurationDestination,
    category: Category,
    library: LibraryViewState,
    project_section: ProjectSection,
    screen: Screen,
    home_overlay: Option<HomeOverlay>,
    child_selected: usize,
    scroll: u16,
    project_scroll: u16,
    form_scroll: u16,
    form_viewport_height: u16,
    fork_scroll: u16,
    fork_viewport_height: u16,
    status: Option<Notice>,
    editor: Option<TaxonomyDraft>,
    editor_mode: EditorMode,
    form: Option<FormState>,
    settings: Option<SettingsDraft>,
    fork_target: String,
    too_small: bool,
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
            library_path: None,
            global,
            global_error,
            project,
            project_error,
            active_scope: ConfigurationDestination::Project,
            category: Category::Project,
            library: LibraryViewState::default(),
            project_section: ProjectSection::Commit,
            screen: Screen::Home,
            home_overlay: None,
            child_selected: 0,
            scroll: 0,
            project_scroll: 0,
            form_scroll: 0,
            form_viewport_height: 1,
            fork_scroll: 0,
            fork_viewport_height: 1,
            status: None,
            editor: None,
            editor_mode: EditorMode::Create,
            form: None,
            settings: None,
            fork_target: String::new(),
            too_small: false,
            project_area: Rect::default(),
            tab_areas: [Rect::default(); 2],
        }
    }

    fn handle_event(&mut self, event: Event, workspace: &dyn ConfigurationWorkspace) -> bool {
        if matches!(&event, Event::Key(key) if key.kind == KeyEventKind::Release) {
            return false;
        }
        if self.home_overlay.is_some() {
            return self.handle_home_overlay(event, workspace);
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
            if let Some(form) = &mut self.form {
                let identity_locked =
                    matches!(self.screen, Screen::Form(FormKind::TaxonomyMetadata))
                        && self.editor_mode == EditorMode::Edit
                        && form.selected == 0;
                if !identity_locked && let Some(field) = form.fields.get_mut(form.selected) {
                    field.push_str(text);
                }
            } else if matches!(self.screen, Screen::Fork) {
                self.fork_target.push_str(text);
            }
            return false;
        }
        if let Event::Mouse(mouse) = event {
            if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                if matches!(self.screen, Screen::Home)
                    && let Some(category) = Category::ALL.into_iter().find(|category| {
                        contains(self.tab_areas[category.index()], mouse.column, mouse.row)
                    })
                {
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
                        self.project_section = *section;
                        self.project_scroll = 0;
                    }
                } else if matches!(self.screen, Screen::Home)
                    && self.category == Category::Library
                    && contains(self.library.browser_area, mouse.column, mouse.row)
                {
                    if let Some(index) = self.library.taxonomy_at(
                        mouse.column,
                        mouse.row,
                        self.catalog().map_or(0, |items| items.len()),
                    ) {
                        self.library.select_taxonomy(index);
                        self.status = None;
                    } else if self.library.create_at(mouse.column, mouse.row) {
                        self.library.select_create();
                        self.request_global_action(PendingAction::Create);
                    }
                } else if matches!(self.screen, Screen::Home)
                    && self.category == Category::Library
                    && contains(self.library.type_area, mouse.column, mouse.row)
                    && let Some(index) = self.library.type_at(
                        mouse.column,
                        mouse.row,
                        self.selected_taxonomy()
                            .map_or(0, |taxonomy| taxonomy.commit_types().len()),
                    )
                {
                    self.library.select_type(index);
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
            Screen::TaxonomyEditor(mode) => self.taxonomy_editor_key(key, mode),
            Screen::TypeEditor(index) => self.type_editor_key(key, index),
            Screen::Form(kind) => self.form_key(key, kind),
            Screen::Fork => self.fork_key(key),
            Screen::Delete => self.delete_key(key),
            Screen::Review(destination) => self.review_key(key, destination, workspace),
            Screen::Leave => match key.code {
                KeyCode::Char('y') => true,
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('n') => {
                    self.screen = Screen::Home;
                    false
                }
                _ => false,
            },
            Screen::ScopeChange(target) => self.scope_change_key(key, target, workspace),
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
        match overlay {
            HomeOverlay::Palette(mut palette) => {
                match event {
                    Event::Key(key) => match key.code {
                        KeyCode::Esc => {}
                        KeyCode::Up | KeyCode::Down => {
                            palette.move_selection(self.category, key.code == KeyCode::Down);
                            self.home_overlay = Some(HomeOverlay::Palette(palette));
                        }
                        KeyCode::Backspace => {
                            let mut query = palette.query.clone();
                            query.pop();
                            palette.set_query(query);
                            self.home_overlay = Some(HomeOverlay::Palette(palette));
                        }
                        KeyCode::Enter => {
                            if let Some(command) = palette.selected_command(self.category) {
                                self.status = None;
                                return self.dispatch_home_command(command, workspace);
                            }
                            self.home_overlay = Some(HomeOverlay::Palette(palette));
                        }
                        KeyCode::Char(character)
                            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
                        {
                            let mut query = palette.query.clone();
                            query.push(character);
                            palette.set_query(query);
                            self.home_overlay = Some(HomeOverlay::Palette(palette));
                        }
                        _ => self.home_overlay = Some(HomeOverlay::Palette(palette)),
                    },
                    Event::Paste(text) => {
                        let mut query = palette.query.clone();
                        query.extend(text.chars().filter(|character| !character.is_control()));
                        palette.set_query(query);
                        self.home_overlay = Some(HomeOverlay::Palette(palette));
                    }
                    Event::Mouse(mouse)
                        if mouse.kind == MouseEventKind::Down(MouseButton::Left) =>
                    {
                        if contains(palette.list_area, mouse.column, mouse.row) {
                            let index = palette.list_offset
                                + usize::from(mouse.row.saturating_sub(palette.list_area.y));
                            if index < palette.visible(self.category).len() {
                                palette.selected = index;
                            }
                            self.home_overlay = Some(HomeOverlay::Palette(palette));
                        }
                    }
                    _ => self.home_overlay = Some(HomeOverlay::Palette(palette)),
                }
                false
            }
            HomeOverlay::Help => {
                if !matches!(
                    event,
                    Event::Key(KeyEvent {
                        code: KeyCode::Esc,
                        ..
                    })
                ) {
                    self.home_overlay = Some(HomeOverlay::Help);
                }
                false
            }
        }
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
            HomeCommand::New => self.request_global_action(PendingAction::Create),
            HomeCommand::Edit => self.request_global_action(PendingAction::Edit),
            HomeCommand::Fork => self.request_global_action(PendingAction::Fork),
            HomeCommand::Delete => self.request_global_action(PendingAction::Delete),
            HomeCommand::ReviewLibrary => self.request_global_action(PendingAction::ReviewGlobal),
            HomeCommand::SwitchTab => self.category = self.category.next(),
            HomeCommand::Help => self.home_overlay = Some(HomeOverlay::Help),
            HomeCommand::Quit => {
                return self.home_key(
                    KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
                    workspace,
                );
            }
        }
        false
    }

    fn home_key(&mut self, key: KeyEvent, _workspace: &dyn ConfigurationWorkspace) -> bool {
        match key.code {
            KeyCode::Char('/') if key.modifiers.is_empty() => {
                self.home_overlay = Some(HomeOverlay::Palette(CommandPaletteState::default()));
            }
            KeyCode::Char('?')
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.home_overlay = Some(HomeOverlay::Help);
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.category = self.category.next();
                self.status = None;
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
            KeyCode::Up | KeyCode::Down if self.library.focus == LibraryFocus::Taxonomies => {
                let count = self.catalog().map_or(0, |items| items.len());
                let show_create = self.show_create_row();
                self.library
                    .move_taxonomy(key.code == KeyCode::Down, count, show_create);
                self.status = None;
            }
            KeyCode::Up | KeyCode::Down => {
                let count = self
                    .selected_taxonomy()
                    .map_or(0, |taxonomy| taxonomy.commit_types().len());
                let index = match key.code {
                    KeyCode::Up => self.library.type_selected.saturating_sub(1),
                    _ => (self.library.type_selected + 1).min(count.saturating_sub(1)),
                };
                self.library.select_type(index);
            }
            KeyCode::Right
                if self.category == Category::Library && !self.library.create_selected =>
            {
                self.library.focus = LibraryFocus::Types;
            }
            KeyCode::Left if self.category == Category::Library => {
                self.library.focus = LibraryFocus::Taxonomies;
            }
            KeyCode::PageUp if self.category == Category::Project => {
                self.project_scroll = self.project_scroll.saturating_sub(8);
            }
            KeyCode::PageDown if self.category == Category::Project => {
                self.project_scroll = self.project_scroll.saturating_add(8);
            }
            KeyCode::PageUp if self.category == Category::Library => {
                if self.library.focus == LibraryFocus::Taxonomies {
                    let page = self.library.taxonomy_content_area.height.max(1);
                    self.library.taxonomy_scroll =
                        self.library.taxonomy_scroll.saturating_sub(page);
                } else {
                    let page = self.library.metadata_area.height.max(1);
                    self.library.property_scroll =
                        self.library.property_scroll.saturating_sub(page);
                }
            }
            KeyCode::PageDown if self.category == Category::Library => {
                if self.library.focus == LibraryFocus::Taxonomies {
                    let page = self.library.taxonomy_content_area.height.max(1);
                    self.library.taxonomy_scroll =
                        self.library.taxonomy_scroll.saturating_add(page);
                } else {
                    let page = self.library.metadata_area.height.max(1);
                    self.library.property_scroll =
                        self.library.property_scroll.saturating_add(page);
                }
            }
            KeyCode::Enter if self.category == Category::Project => {
                if self.project_section == ProjectSection::Commit {
                    self.request_project_action(PendingAction::EditProject);
                }
            }
            KeyCode::Enter
                if self.category == Category::Library && self.library.create_selected =>
            {
                self.request_global_action(PendingAction::Create);
            }
            KeyCode::Char('i') if self.category == Category::Project => {
                self.request_project_action(PendingAction::InitializeProject);
            }
            KeyCode::Char('n')
                if self.category == Category::Library && key.modifiers.is_empty() =>
            {
                self.request_global_action(PendingAction::Create);
            }
            KeyCode::Char('e')
                if self.category == Category::Library && key.modifiers.is_empty() =>
            {
                self.request_global_action(PendingAction::Edit);
            }
            KeyCode::Char('f')
                if self.category == Category::Library && key.modifiers.is_empty() =>
            {
                self.request_global_action(PendingAction::Fork);
            }
            KeyCode::Char('d')
                if self.category == Category::Library && key.modifiers.is_empty() =>
            {
                self.request_global_action(PendingAction::Delete);
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.category == Category::Library {
                    self.request_global_action(PendingAction::ReviewGlobal);
                } else {
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
            self.status = Some(Notice::Error(self.project_error.clone()));
            return;
        };
        match action {
            PendingAction::EditProject if session.project_config().is_none() => {
                self.status = Some(Notice::Error(
                    "Initialize the project before editing.".into(),
                ));
                return;
            }
            PendingAction::InitializeProject if session.project_config().is_some() => {
                self.status = Some(Notice::Info("Project is already initialized.".into()));
                return;
            }
            _ => {}
        }
        if self.active_scope == ConfigurationDestination::Global
            && self.scope_dirty(ConfigurationDestination::Global)
        {
            self.screen = Screen::ScopeChange(action);
            return;
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
            ConfigurationDestination::Global => (
                session.global().default_taxonomy().clone(),
                session.global().available_taxonomies().to_vec(),
                false,
                false,
            ),
            ConfigurationDestination::Project => {
                let Some(config) = session.project_config() else {
                    return;
                };
                (
                    config
                        .default_taxonomy()
                        .unwrap_or_else(|| session.global().default_taxonomy())
                        .clone(),
                    config
                        .available_taxonomies()
                        .unwrap_or_else(|| session.global().available_taxonomies())
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
                        self.status = Some(Notice::Error(
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
                if let Some(session) = self.project.as_ref() {
                    settings.default = session.global().default_taxonomy().clone();
                }
            }
            KeyCode::Char('a') if destination == ConfigurationDestination::Project => {
                settings.inherit_available = true;
                if let Some(session) = self.project.as_ref() {
                    settings.available = session.global().available_taxonomies().to_vec();
                }
            }
            KeyCode::Char('r') if destination == ConfigurationDestination::Project => {
                settings.inherit_default = true;
                settings.inherit_available = true;
                if let Some(session) = self.project.as_ref() {
                    settings.default = session.global().default_taxonomy().clone();
                    settings.available = session.global().available_taxonomies().to_vec();
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
                        self.status = Some(Notice::Info(
                            "Configuration change staged. Ctrl+S reviews before applying.".into(),
                        ));
                    }
                    Err(error) => self.status = Some(Notice::Error(error)),
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

    fn taxonomy_editor_key(&mut self, key: KeyEvent, mode: EditorMode) -> bool {
        let count = self
            .editor
            .as_ref()
            .map_or(1, |draft| draft.types.len() + 1);
        match key.code {
            KeyCode::Up if key.modifiers.contains(KeyModifiers::ALT) && self.child_selected > 1 => {
                if let Some(editor) = &mut self.editor {
                    editor
                        .types
                        .swap(self.child_selected - 1, self.child_selected - 2);
                    self.child_selected -= 1;
                }
            }
            KeyCode::Down
                if key.modifiers.contains(KeyModifiers::ALT)
                    && self.child_selected > 0
                    && self.child_selected < count - 1 =>
            {
                if let Some(editor) = &mut self.editor {
                    editor
                        .types
                        .swap(self.child_selected - 1, self.child_selected);
                    self.child_selected += 1;
                }
            }
            KeyCode::Up => self.child_selected = self.child_selected.saturating_sub(1),
            KeyCode::Down => {
                self.child_selected = (self.child_selected + 1).min(count.saturating_sub(1))
            }
            KeyCode::Enter if self.child_selected == 0 => self.open_taxonomy_metadata(mode),
            KeyCode::Enter => {
                let index = self.child_selected - 1;
                self.child_selected = 0;
                self.screen = Screen::TypeEditor(index);
            }
            KeyCode::Char('n') => {
                self.form = Some(FormState::metadata(String::new(), String::new()));
                self.form_scroll = 0;
                self.screen = Screen::Form(FormKind::NewType);
            }
            KeyCode::Char('d') if self.child_selected > 0 => {
                if let Some(editor) = &mut self.editor {
                    editor.types.remove(self.child_selected - 1);
                    self.child_selected = self.child_selected.saturating_sub(1);
                }
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.stage_editor(mode)
            }
            KeyCode::Esc => {
                self.editor = None;
                self.screen = Screen::Home;
            }
            _ => {}
        }
        false
    }

    fn type_editor_key(&mut self, key: KeyEvent, index: usize) -> bool {
        let count = self
            .editor
            .as_ref()
            .and_then(|draft| draft.types.get(index))
            .map_or(1, |draft| draft.properties.len() + 1);
        match key.code {
            KeyCode::Up if key.modifiers.contains(KeyModifiers::ALT) && self.child_selected > 1 => {
                if let Some(kind) = self
                    .editor
                    .as_mut()
                    .and_then(|draft| draft.types.get_mut(index))
                {
                    kind.properties
                        .swap(self.child_selected - 1, self.child_selected - 2);
                    self.child_selected -= 1;
                }
            }
            KeyCode::Down
                if key.modifiers.contains(KeyModifiers::ALT)
                    && self.child_selected > 0
                    && self.child_selected < count - 1 =>
            {
                if let Some(kind) = self
                    .editor
                    .as_mut()
                    .and_then(|draft| draft.types.get_mut(index))
                {
                    kind.properties
                        .swap(self.child_selected - 1, self.child_selected);
                    self.child_selected += 1;
                }
            }
            KeyCode::Up => self.child_selected = self.child_selected.saturating_sub(1),
            KeyCode::Down => {
                self.child_selected = (self.child_selected + 1).min(count.saturating_sub(1))
            }
            KeyCode::Enter if self.child_selected == 0 => self.open_type_form(index),
            KeyCode::Enter => self.open_property_form(index, self.child_selected - 1),
            KeyCode::Char('n') => {
                self.form = Some(FormState::property(PropertyDraft::empty()));
                self.form_scroll = 0;
                self.screen = Screen::Form(FormKind::NewProperty(index));
            }
            KeyCode::Char('d') if self.child_selected > 0 => {
                if let Some(kind) = self
                    .editor
                    .as_mut()
                    .and_then(|draft| draft.types.get_mut(index))
                {
                    kind.properties.remove(self.child_selected - 1);
                    self.child_selected = self.child_selected.saturating_sub(1);
                }
            }
            KeyCode::Esc => {
                self.child_selected = index + 1;
                self.screen = Screen::TaxonomyEditor(self.editor_mode);
            }
            _ => {}
        }
        false
    }

    fn form_key(&mut self, key: KeyEvent, kind: FormKind) -> bool {
        let Some(form) = &mut self.form else {
            return false;
        };
        match key.code {
            KeyCode::Tab => form.selected = (form.selected + 1) % form.fields.len(),
            KeyCode::PageUp => {
                self.form_scroll = self
                    .form_scroll
                    .saturating_sub(self.form_viewport_height.max(1));
            }
            KeyCode::PageDown => {
                self.form_scroll = self
                    .form_scroll
                    .saturating_add(self.form_viewport_height.max(1));
            }
            KeyCode::BackTab => {
                form.selected = if form.selected == 0 {
                    form.fields.len() - 1
                } else {
                    form.selected - 1
                }
            }
            KeyCode::Left
                if matches!(
                    kind,
                    FormKind::NewProperty(_) | FormKind::EditProperty(_, _)
                ) && form.selected == 2 =>
            {
                form.requirement = previous_requirement(&form.requirement)
            }
            KeyCode::Right
                if matches!(
                    kind,
                    FormKind::NewProperty(_) | FormKind::EditProperty(_, _)
                ) && form.selected == 2 =>
            {
                form.requirement = next_requirement(&form.requirement)
            }
            KeyCode::Left | KeyCode::Right
                if matches!(
                    kind,
                    FormKind::NewProperty(_) | FormKind::EditProperty(_, _)
                ) && form.selected == 3 =>
            {
                form.multiplicity = match form.multiplicity {
                    PropertyMultiplicity::Single => PropertyMultiplicity::Multiple,
                    PropertyMultiplicity::Multiple => PropertyMultiplicity::Single,
                }
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.submit_form(kind)
            }
            KeyCode::Esc => {
                self.form = None;
                self.screen = parent_screen(kind, self.editor_mode);
            }
            KeyCode::Backspace
                if !(kind == FormKind::TaxonomyMetadata
                    && self.editor_mode == EditorMode::Edit
                    && form.selected == 0) =>
            {
                if let Some(value) = form.fields.get_mut(form.selected) {
                    value.pop();
                }
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
                if let Some(value) = form.fields.get_mut(form.selected) {
                    value.push('\n');
                }
            }
            KeyCode::Char(character)
                if (key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT)
                    && !(kind == FormKind::TaxonomyMetadata
                        && self.editor_mode == EditorMode::Edit
                        && form.selected == 0) =>
            {
                if let Some(value) = form.fields.get_mut(form.selected) {
                    value.push(character);
                }
            }
            _ => {}
        }
        false
    }

    fn fork_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::PageUp => {
                self.fork_scroll = self
                    .fork_scroll
                    .saturating_sub(self.fork_viewport_height.max(1));
            }
            KeyCode::PageDown => {
                self.fork_scroll = self
                    .fork_scroll
                    .saturating_add(self.fork_viewport_height.max(1));
            }
            KeyCode::Backspace => {
                self.fork_target.pop();
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let result = self
                    .selected_taxonomy()
                    .map(|source| source.id().clone())
                    .ok_or("select a taxonomy".to_owned())
                    .and_then(|source| {
                        TaxonomyId::new(&self.fork_target)
                            .map_err(|error| error.to_string())
                            .and_then(|target| {
                                self.global
                                    .as_mut()
                                    .ok_or("global configuration unavailable".to_owned())
                                    .and_then(|session| session.fork_taxonomy(&source, target))
                            })
                    });
                match result {
                    Ok(()) => {
                        self.screen = Screen::Home;
                        self.status = Some(Notice::Info(
                            "Fork staged. Ctrl+S reviews before applying.".into(),
                        ));
                    }
                    Err(error) => self.status = Some(Notice::Error(error)),
                }
            }
            KeyCode::Esc => self.screen = Screen::Home,
            KeyCode::Char(character)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.fork_target.push(character)
            }
            _ => {}
        }
        false
    }

    fn delete_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('y') => {
                let result = self
                    .selected_taxonomy()
                    .map(|taxonomy| taxonomy.id().clone())
                    .ok_or("select a taxonomy".to_owned())
                    .and_then(|id| {
                        self.global
                            .as_mut()
                            .ok_or("global configuration unavailable".to_owned())
                            .and_then(|session| session.delete_taxonomy(&id))
                    });
                match result {
                    Ok(()) => {
                        self.library.browse_selected =
                            self.library.browse_selected.saturating_sub(1);
                        self.library.type_selected = 0;
                        self.library.type_offset = 0;
                        self.library.property_scroll = 0;
                        self.screen = Screen::Home;
                        self.status = Some(Notice::Info(
                            "Deletion staged. Ctrl+S reviews before applying.".into(),
                        ));
                    }
                    Err(error) => {
                        self.screen = Screen::Home;
                        self.status = Some(Notice::Error(error));
                    }
                }
            }
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('n') => self.screen = Screen::Home,
            _ => {}
        }
        false
    }

    fn review_key(
        &mut self,
        key: KeyEvent,
        destination: ConfigurationDestination,
        workspace: &dyn ConfigurationWorkspace,
    ) -> bool {
        match key.code {
            KeyCode::Enter => {
                let Some(session) = self.session(destination).cloned() else {
                    return false;
                };
                match workspace.save(&session) {
                    Ok(saved) => {
                        match destination {
                            ConfigurationDestination::Global => self.global = Some(saved),
                            ConfigurationDestination::Project => self.project = Some(saved),
                        }
                        if destination == ConfigurationDestination::Global {
                            let (project, error) =
                                split_result(workspace.load(ConfigurationDestination::Project));
                            self.project = project;
                            self.project_error = error;
                        }
                        self.screen = Screen::Home;
                        self.status = Some(Notice::Info("Configuration applied.".into()));
                    }
                    Err(error) => {
                        self.screen = Screen::Home;
                        self.status = Some(Notice::Error(error));
                    }
                }
            }
            KeyCode::Esc => self.screen = Screen::Home,
            KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
            _ => {}
        }
        false
    }

    fn scope_change_key(
        &mut self,
        key: KeyEvent,
        action: PendingAction,
        workspace: &dyn ConfigurationWorkspace,
    ) -> bool {
        match key.code {
            KeyCode::Char('a') => {
                let destination = self.active_scope;
                let Some(session) = self.session(destination).cloned() else {
                    return false;
                };
                match workspace.save(&session) {
                    Ok(saved) => {
                        match destination {
                            ConfigurationDestination::Global => self.global = Some(saved),
                            ConfigurationDestination::Project => self.project = Some(saved),
                        };
                        self.screen = Screen::Home;
                        self.perform_pending_action(action);
                    }
                    Err(error) => {
                        self.screen = Screen::Home;
                        self.status = Some(Notice::Error(error));
                    }
                }
            }
            KeyCode::Char('d') => {
                let destination = self.active_scope;
                let (session, error) = split_result(workspace.load(destination));
                match destination {
                    ConfigurationDestination::Global => {
                        self.global = session;
                        self.global_error = error;
                    }
                    ConfigurationDestination::Project => {
                        self.project = session;
                        self.project_error = error;
                    }
                }
                self.screen = Screen::Home;
                self.perform_pending_action(action);
            }
            KeyCode::Esc | KeyCode::Enter => self.screen = Screen::Home,
            _ => {}
        }
        false
    }

    fn request_global_action(&mut self, action: PendingAction) {
        if let Some(error) = self.catalog_error() {
            self.status = Some(Notice::Error(error));
            return;
        }
        if !matches!(action, PendingAction::Create | PendingAction::ReviewGlobal)
            && self.selected_taxonomy().is_none()
        {
            self.status = Some(Notice::Error("select a taxonomy".into()));
            return;
        }
        if matches!(action, PendingAction::Edit | PendingAction::Delete)
            && self.selected_taxonomy().is_some_and(|taxonomy| {
                self.global
                    .as_ref()
                    .and_then(|session| session.catalog().ok())
                    .and_then(|catalog| catalog.origin(taxonomy.id()))
                    != Some(TaxonomyOrigin::Custom)
            })
        {
            self.status = Some(Notice::Error(
                "Built-in taxonomies are read-only; fork one to customize it.".into(),
            ));
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
                    self.status = Some(Notice::Error(self.project_error.clone()));
                    return;
                };
                match session
                    .initialize_project()
                    .and_then(|()| session.review().map(|_| ()))
                {
                    Ok(()) => self.screen = Screen::Review(ConfigurationDestination::Project),
                    Err(error) => self.status = Some(Notice::Error(error)),
                }
            }
            PendingAction::ReviewProject => {
                self.active_scope = ConfigurationDestination::Project;
                self.open_review();
            }
            PendingAction::Create => {
                self.active_scope = ConfigurationDestination::Global;
                self.editor = Some(TaxonomyDraft::empty());
                self.editor_mode = EditorMode::Create;
                self.child_selected = 0;
                self.screen = Screen::TaxonomyEditor(EditorMode::Create);
            }
            PendingAction::Edit => {
                self.active_scope = ConfigurationDestination::Global;
                self.open_edit();
            }
            PendingAction::Fork => {
                self.active_scope = ConfigurationDestination::Global;
                self.fork_target.clear();
                self.fork_scroll = 0;
                self.screen = Screen::Fork;
            }
            PendingAction::Delete => {
                self.active_scope = ConfigurationDestination::Global;
                self.screen = Screen::Delete;
            }
            PendingAction::ReviewGlobal => {
                self.active_scope = ConfigurationDestination::Global;
                self.open_review();
            }
        }
    }

    fn open_review(&mut self) {
        let destination = self.active_scope;
        let should_review = self
            .session(destination)
            .is_some_and(|session| session.is_dirty() || session.update_available());
        if should_review {
            if let Some(error) = self
                .session(destination)
                .and_then(|session| session.review().err())
            {
                self.status = Some(Notice::Error(error));
            } else {
                self.scroll = 0;
                self.screen = Screen::Review(destination);
            }
        } else {
            self.status = Some(Notice::Info(
                "No changes or taxonomy updates to apply.".into(),
            ));
        }
    }

    fn open_edit(&mut self) {
        let Some(taxonomy) = self.selected_taxonomy() else {
            return;
        };
        if self
            .global
            .as_ref()
            .and_then(|session| session.catalog().ok())
            .and_then(|catalog| catalog.origin(taxonomy.id()))
            != Some(TaxonomyOrigin::Custom)
        {
            self.status = Some(Notice::Error(
                "Built-in taxonomies are read-only; fork one to customize it.".into(),
            ));
            return;
        }
        self.editor = Some(TaxonomyDraft::from_taxonomy(&taxonomy));
        self.editor_mode = EditorMode::Edit;
        self.child_selected = 0;
        self.screen = Screen::TaxonomyEditor(EditorMode::Edit);
    }

    fn open_taxonomy_metadata(&mut self, mode: EditorMode) {
        if let Some(editor) = &self.editor {
            let mut form = FormState::metadata(editor.id.clone(), editor.description.clone());
            if matches!(mode, EditorMode::Edit) {
                form.selected = 1;
            }
            self.form = Some(form);
            self.form_scroll = 0;
            self.screen = Screen::Form(FormKind::TaxonomyMetadata);
        }
    }

    fn open_type_form(&mut self, index: usize) {
        if let Some(kind) = self
            .editor
            .as_ref()
            .and_then(|draft| draft.types.get(index))
        {
            self.form = Some(FormState::metadata(
                kind.id.clone(),
                kind.description.clone(),
            ));
            self.form_scroll = 0;
            self.screen = Screen::Form(FormKind::EditType(index));
        }
    }

    fn open_property_form(&mut self, type_index: usize, property_index: usize) {
        if let Some(property) = self
            .editor
            .as_ref()
            .and_then(|draft| draft.types.get(type_index))
            .and_then(|draft| draft.properties.get(property_index))
        {
            self.form = Some(FormState::property(property.clone()));
            self.form_scroll = 0;
            self.screen = Screen::Form(FormKind::EditProperty(type_index, property_index));
        }
    }

    fn submit_form(&mut self, kind: FormKind) {
        let Some(form) = self.form.clone() else {
            return;
        };
        let result: Result<(), String> = match kind {
            FormKind::TaxonomyMetadata => {
                if form.fields[0].trim().is_empty() || form.fields[1].trim().is_empty() {
                    Err("Taxonomy id and description are required.".into())
                } else if let Some(editor) = &mut self.editor {
                    editor.id.clone_from(&form.fields[0]);
                    editor.description.clone_from(&form.fields[1]);
                    Ok(())
                } else {
                    Err("taxonomy draft is missing".into())
                }
            }
            FormKind::NewType | FormKind::EditType(_) => {
                let candidate = TypeDraft {
                    id: form.fields[0].clone(),
                    description: form.fields[1].clone(),
                    schema_version: 1,
                    properties: match kind {
                        FormKind::EditType(index) => self
                            .editor
                            .as_ref()
                            .and_then(|draft| draft.types.get(index))
                            .map_or_else(Vec::new, |draft| draft.properties.clone()),
                        _ => Vec::new(),
                    },
                };
                candidate.build().map(|_| ()).and_then(|()| {
                    let editor = self.editor.as_mut().ok_or("taxonomy draft is missing")?;
                    match kind {
                        FormKind::NewType => editor.types.push(candidate),
                        FormKind::EditType(index) => editor.types[index] = candidate,
                        _ => {}
                    };
                    Ok(())
                })
            }
            FormKind::NewProperty(_) | FormKind::EditProperty(_, _) => {
                let requirement = if matches!(form.requirement, PropertyRequirement::Conditional(_))
                {
                    let id = ConditionId::new(&form.fields[2]).map_err(|error| error.to_string());
                    id.and_then(|id| {
                        PropertyCondition::new(id, form.fields[3].clone())
                            .map(PropertyRequirement::Conditional)
                            .map_err(|error| error.to_string())
                    })
                } else {
                    Ok(form.requirement.clone())
                };
                requirement
                    .and_then(|requirement| {
                        let candidate = PropertyDraft {
                            key: form.fields[0].clone(),
                            description: form.fields[1].clone(),
                            requirement,
                            multiplicity: form.multiplicity,
                        };
                        candidate.build().map(|_| candidate)
                    })
                    .and_then(|candidate| {
                        let editor = self.editor.as_mut().ok_or("taxonomy draft is missing")?;
                        match kind {
                            FormKind::NewProperty(index) => {
                                editor.types[index].properties.push(candidate)
                            }
                            FormKind::EditProperty(type_index, property_index) => {
                                editor.types[type_index].properties[property_index] = candidate
                            }
                            _ => {}
                        }
                        Ok(())
                    })
            }
        };
        match result {
            Ok(()) => {
                self.form = None;
                self.screen = parent_screen(kind, self.editor_mode);
            }
            Err(error) => self.status = Some(Notice::Error(error)),
        }
    }

    fn stage_editor(&mut self, mode: EditorMode) {
        let Some(draft) = &self.editor else {
            return;
        };
        let result = draft.build(mode).and_then(|taxonomy| {
            let session = self
                .global
                .as_mut()
                .ok_or("global configuration unavailable".to_owned())?;
            match mode {
                EditorMode::Create => session.create_taxonomy(taxonomy),
                EditorMode::Edit => {
                    let original = session
                        .catalog()?
                        .find(taxonomy.id())
                        .cloned()
                        .ok_or("taxonomy is missing")?;
                    revised_taxonomy(
                        &original,
                        taxonomy.description().clone(),
                        taxonomy.commit_types().to_vec(),
                    )
                    .and_then(|revised| session.update_taxonomy(revised))
                }
            }
        });
        match result {
            Ok(()) => {
                self.editor = None;
                self.screen = Screen::Home;
                self.status = Some(Notice::Info(
                    "Taxonomy change staged. Ctrl+S reviews before applying.".into(),
                ));
            }
            Err(error) => self.status = Some(Notice::Error(error)),
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

    fn catalog_error(&self) -> Option<String> {
        self.global.as_ref().map_or_else(
            || {
                Some(if self.global_error.is_empty() {
                    "global configuration unavailable".into()
                } else {
                    self.global_error.clone()
                })
            },
            |session| session.catalog().err(),
        )
    }

    fn selected_taxonomy(&self) -> Option<Taxonomy> {
        if self.library.create_selected {
            return None;
        }
        self.catalog()?.get(self.library.browse_selected).cloned()
    }

    fn show_create_row(&self) -> bool {
        self.global
            .as_ref()
            .and_then(|session| session.catalog().ok())
            .is_some_and(|catalog| {
                catalog
                    .taxonomies()
                    .iter()
                    .all(|taxonomy| catalog.origin(taxonomy.id()) != Some(TaxonomyOrigin::Custom))
            })
    }

    fn scope_dirty(&self, destination: ConfigurationDestination) -> bool {
        self.session(destination)
            .is_some_and(ConfigurationSession::is_dirty)
    }

    fn any_dirty(&self) -> bool {
        self.scope_dirty(ConfigurationDestination::Global)
            || self.scope_dirty(ConfigurationDestination::Project)
    }

    fn render(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        frame.render_widget(
            Block::default().style(Style::default().bg(JET_BLACK).fg(Color::White)),
            area,
        );
        self.too_small = area.width < MINIMUM_WIDTH || area.height < MINIMUM_HEIGHT;
        if self.too_small {
            frame.render_widget(
                Paragraph::new("Terminal too small\n\nResize or press esc/q to cancel").centered(),
                centered_rect(54, 3, area),
            );
            normalize_background(frame, area);
            return;
        }
        let [options, content, footer] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(area);
        frame.render_widget(
            Block::bordered()
                .border_style(frame_style())
                .title("Config Options"),
            options,
        );
        self.render_tabs(
            frame,
            Rect::new(
                options.x.saturating_add(1),
                options.y.saturating_add(1),
                options.width.saturating_sub(2),
                1,
            ),
        );
        let body = content;
        let hints = match self.screen.clone() {
            Screen::Home => {
                if self.category == Category::Project {
                    self.render_project(frame, body);
                    vec![
                        ("tab", "switch"),
                        ("↑/↓", "move"),
                        ("/", "commands"),
                        ("?", "help"),
                    ]
                } else {
                    self.render_library(frame, body);
                    vec![
                        ("tab", "switch"),
                        ("←/→", "focus"),
                        ("↑/↓", "move"),
                        ("/", "commands"),
                        ("?", "help"),
                    ]
                }
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
            Screen::TaxonomyEditor(mode) => {
                self.render_taxonomy_editor(frame, body, mode);
                vec![
                    ("enter", "open"),
                    ("n/d", "add/remove"),
                    ("alt+↑/↓", "reorder"),
                    ("ctrl+s", "complete"),
                    ("esc", "back"),
                ]
            }
            Screen::TypeEditor(index) => {
                self.render_type_editor(frame, body, index);
                vec![
                    ("enter", "edit"),
                    ("n/d", "add/remove"),
                    ("alt+↑/↓", "reorder"),
                    ("esc", "back"),
                ]
            }
            Screen::Form(kind) => {
                self.render_form(frame, body, kind);
                vec![
                    ("tab", "field"),
                    ("←/→", "choice"),
                    ("alt+enter", "newline"),
                    ("ctrl+s", "complete"),
                    ("esc", "back"),
                ]
            }
            Screen::Fork => {
                self.render_fork(frame, body);
                vec![("ctrl+s", "fork"), ("esc", "back")]
            }
            Screen::Delete => {
                self.render_confirmation(
                    frame,
                    body,
                    "Delete this custom taxonomy?",
                    "y: stage deletion    enter/esc/n: keep",
                );
                vec![("y", "delete"), ("esc/n", "keep")]
            }
            Screen::Review(destination) => {
                self.render_review(frame, body, destination);
                vec![("enter", "apply"), ("esc", "back"), ("↑/↓", "scroll")]
            }
            Screen::Leave => {
                self.render_confirmation(
                    frame,
                    body,
                    "Discard all unapplied configuration changes?",
                    "y: discard and quit    enter/esc/n: keep",
                );
                vec![("y", "discard"), ("esc/n", "keep")]
            }
            Screen::ScopeChange(_) => {
                self.render_confirmation(
                    frame,
                    body,
                    "This scope has unapplied changes.",
                    "a: apply    d: discard    enter/esc: cancel",
                );
                vec![("a", "apply"), ("d", "discard"), ("esc", "cancel")]
            }
        };
        render_navigation_row(frame, footer, &hints);
        if let Some(overlay) = &mut self.home_overlay {
            overlay.render(frame, body, self.category);
        } else {
            self.render_notice(frame, body);
        }
        normalize_background(frame, area);
    }

    fn render_tabs(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.tab_areas = [Rect::default(); 2];
        let mut x = area.x.saturating_add(1);
        for category in Category::ALL {
            let label = category.label();
            let width = u16::try_from(Line::from(label).width())
                .unwrap_or(u16::MAX)
                .saturating_add(2);
            let tab = Rect::new(x, area.y, width.min(area.right().saturating_sub(x)), 1);
            self.tab_areas[category.index()] = tab;
            let style = if category == self.category {
                navigation_key_style()
            } else {
                Style::default()
            };
            frame.render_widget(Paragraph::new(format!(" {label} ")).style(style), tab);
            x = x.saturating_add(width).saturating_add(1);
        }
        let dirty = if self.any_dirty() {
            "unapplied changes"
        } else {
            ""
        };
        if !dirty.is_empty() {
            let width = u16::try_from(Line::from(dirty).width()).unwrap_or(u16::MAX);
            if x.saturating_add(width) < area.right() {
                let indicator = Rect::new(area.right().saturating_sub(width), area.y, width, 1);
                frame.render_widget(
                    Paragraph::new(dirty).style(section_heading_style()),
                    indicator,
                );
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
                Category::Library => self.catalog_error(),
            }
        } else {
            None
        };
        let (message, style) = match (&self.status, load_error) {
            (Some(Notice::Error(message)), _) => (message.clone(), Style::default().fg(Color::Red)),
            (Some(Notice::Info(message)), _) => {
                (message.clone(), Style::default().fg(Color::White))
            }
            (None, Some(message)) => (message, Style::default().fg(Color::Red)),
            (None, None) => return,
        };
        let width = area.width.min(56);
        let lines = Paragraph::new(message.as_str()).wrap(Wrap { trim: false });
        let height = u16::try_from(lines.line_count(width.saturating_sub(2)))
            .unwrap_or(u16::MAX)
            .min(3)
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
        frame.render_widget(
            Block::bordered()
                .border_style(frame_style())
                .title("Project"),
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
            List::new(rows).highlight_style(navigation_key_style()),
            nav,
            &mut state,
        );

        frame.render_widget(
            Block::bordered()
                .border_style(frame_style())
                .title(self.project_section.label()),
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
        render_overflow_scrollbar(
            frame,
            columns[1],
            total_rows,
            detail.height,
            self.project_scroll,
        );
    }

    fn project_detail(&self) -> String {
        let Some(session) = &self.project else {
            return self.project_error.clone();
        };
        let inherited = ProjectConfig::inherited();
        let config = session.project_config().unwrap_or(&inherited);
        let baseline = session.global();
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

    fn render_library(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let catalog = self
            .global
            .as_ref()
            .and_then(|session| session.catalog().ok());
        let error = self.catalog_error();
        self.library
            .render(frame, area, catalog.as_ref(), error.as_deref());
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
                .style(Style::default().bg(if index % 2 == 0 {
                    JET_BLACK
                } else {
                    ZEBRA_BACKGROUND
                }))
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
        let total_rows = u16::try_from(rows.len()).unwrap_or(u16::MAX);
        frame.render_stateful_widget(
            List::new(rows)
                .block(
                    Block::bordered()
                        .border_style(frame_style())
                        .title(match destination {
                            ConfigurationDestination::Global => "Global availability",
                            ConfigurationDestination::Project => "Project availability",
                        }),
                )
                .highlight_style(navigation_key_style()),
            parts[1],
            &mut state,
        );
        render_overflow_scrollbar(
            frame,
            parts[1],
            total_rows,
            parts[1].height.saturating_sub(2),
            u16::try_from(state.offset()).unwrap_or(u16::MAX),
        );
    }

    fn render_taxonomy_editor(&self, frame: &mut Frame<'_>, area: Rect, mode: EditorMode) {
        let Some(editor) = &self.editor else {
            return;
        };
        let mut rows = vec![ListItem::new(format!(
            "Metadata  {}  {}",
            editor.id, editor.description
        ))];
        rows.extend(editor.types.iter().map(|kind| {
            ListItem::new(format!(
                "{}  {}  ({} properties)",
                kind.id,
                kind.description,
                kind.properties.len()
            ))
        }));
        let mut state = ListState::default();
        state.select(Some(self.child_selected));
        let total_rows = u16::try_from(rows.len()).unwrap_or(u16::MAX);
        frame.render_stateful_widget(
            List::new(rows)
                .block(
                    Block::bordered()
                        .border_style(frame_style())
                        .title(match mode {
                            EditorMode::Create => "Create taxonomy",
                            EditorMode::Edit => "Edit taxonomy",
                        }),
                )
                .highlight_style(navigation_key_style()),
            area,
            &mut state,
        );
        render_overflow_scrollbar(
            frame,
            area,
            total_rows,
            area.height.saturating_sub(2),
            u16::try_from(state.offset()).unwrap_or(u16::MAX),
        );
    }

    fn render_type_editor(&self, frame: &mut Frame<'_>, area: Rect, index: usize) {
        let Some(kind) = self
            .editor
            .as_ref()
            .and_then(|draft| draft.types.get(index))
        else {
            return;
        };
        let mut rows = vec![ListItem::new(format!(
            "Metadata  {}  {}",
            kind.id, kind.description
        ))];
        rows.extend(kind.properties.iter().map(|property| {
            ListItem::new(format!(
                "{}  {:?}  {:?}",
                property.key, property.requirement, property.multiplicity
            ))
        }));
        let mut state = ListState::default();
        state.select(Some(self.child_selected));
        let total_rows = u16::try_from(rows.len()).unwrap_or(u16::MAX);
        frame.render_stateful_widget(
            List::new(rows)
                .block(
                    Block::bordered()
                        .border_style(frame_style())
                        .title("Commit type properties"),
                )
                .highlight_style(navigation_key_style()),
            area,
            &mut state,
        );
        render_overflow_scrollbar(
            frame,
            area,
            total_rows,
            area.height.saturating_sub(2),
            u16::try_from(state.offset()).unwrap_or(u16::MAX),
        );
    }

    fn render_form(&mut self, frame: &mut Frame<'_>, area: Rect, kind: FormKind) {
        let Some(form) = &self.form else {
            return;
        };
        let labels: &[&str] = if matches!(
            kind,
            FormKind::NewProperty(_) | FormKind::EditProperty(_, _)
        ) {
            &["Key", "Description", "Condition id", "Condition rationale"]
        } else {
            &["Id", "Description"]
        };
        let mut lines = Vec::new();
        for (index, label) in labels.iter().enumerate() {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{label}\n"),
                    if index == form.selected {
                        navigation_key_style()
                    } else {
                        section_heading_style()
                    },
                ),
                Span::raw(form.fields.get(index).map_or("", String::as_str)),
                Span::raw("\n"),
            ]));
        }
        if matches!(
            kind,
            FormKind::NewProperty(_) | FormKind::EditProperty(_, _)
        ) {
            lines.push(Line::from(format!("Requirement  {:?}", form.requirement)));
            lines.push(Line::from(format!("Multiplicity {:?}", form.multiplicity)));
        }
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        let width = area.width.saturating_sub(2);
        self.form_viewport_height = area.height.saturating_sub(2);
        let total_rows = u16::try_from(paragraph.line_count(width)).unwrap_or(u16::MAX);
        self.form_scroll = self
            .form_scroll
            .min(total_rows.saturating_sub(self.form_viewport_height));
        frame.render_widget(
            paragraph.scroll((self.form_scroll, 0)).block(
                Block::bordered()
                    .border_style(frame_style())
                    .title("Focused editor"),
            ),
            area,
        );
        render_overflow_scrollbar(
            frame,
            area,
            total_rows,
            self.form_viewport_height,
            self.form_scroll,
        );
    }

    fn render_fork(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let source = self
            .selected_taxonomy()
            .map_or_else(String::new, |taxonomy| taxonomy.id().to_string());
        let paragraph = Paragraph::new(format!("Fork taxonomy\n\nSource\n  {source}\n\nNew taxonomy id\n  {}\n\nThe fork is a snapshot; later source changes never propagate.", self.fork_target))
            .wrap(Wrap { trim: false });
        self.fork_viewport_height = area.height.saturating_sub(2);
        let total_rows =
            u16::try_from(paragraph.line_count(area.width.saturating_sub(2))).unwrap_or(u16::MAX);
        self.fork_scroll = self
            .fork_scroll
            .min(total_rows.saturating_sub(self.fork_viewport_height));
        frame.render_widget(
            paragraph
                .scroll((self.fork_scroll, 0))
                .block(Block::bordered().border_style(frame_style())),
            area,
        );
        render_overflow_scrollbar(
            frame,
            area,
            total_rows,
            self.fork_viewport_height,
            self.fork_scroll,
        );
    }

    fn render_review(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        destination: ConfigurationDestination,
    ) {
        let text = self
            .session(destination)
            .and_then(|session| session.review().ok())
            .unwrap_or_else(|| "Configuration cannot be reviewed.".into());
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
        let viewport_height = area.height.saturating_sub(2);
        let total_rows =
            u16::try_from(paragraph.line_count(area.width.saturating_sub(2))).unwrap_or(u16::MAX);
        self.scroll = self.scroll.min(total_rows.saturating_sub(viewport_height));
        frame.render_widget(
            paragraph.scroll((self.scroll, 0)).block(
                Block::bordered()
                    .border_style(frame_style())
                    .title("Review changes — Enter to apply"),
            ),
            area,
        );
        render_overflow_scrollbar(frame, area, total_rows, viewport_height, self.scroll);
    }

    fn render_confirmation(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        message: &str,
        controls: &str,
    ) {
        let popup = centered_rect(58, 9, area);
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Block::bordered()
                .border_type(BorderType::Double)
                .border_style(frame_style())
                .style(Style::default().bg(JET_BLACK)),
            popup,
        );
        frame.render_widget(
            Paragraph::new(format!("Configuration\n\n{message}\n\n{controls}")).centered(),
            Rect::new(
                popup.x + 2,
                popup.y + 2,
                popup.width.saturating_sub(4),
                popup.height.saturating_sub(4),
            ),
        );
    }
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

const fn requirement_label(value: &PropertyRequirement) -> &'static str {
    match value {
        PropertyRequirement::Required => "required",
        PropertyRequirement::Recommended => "recommended",
        PropertyRequirement::Optional => "optional",
        PropertyRequirement::Conditional(_) => "conditional",
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

fn parent_screen(kind: FormKind, mode: EditorMode) -> Screen {
    match kind {
        FormKind::TaxonomyMetadata | FormKind::NewType => Screen::TaxonomyEditor(mode),
        FormKind::EditType(index)
        | FormKind::NewProperty(index)
        | FormKind::EditProperty(index, _) => Screen::TypeEditor(index),
    }
}

fn next_requirement(value: &PropertyRequirement) -> PropertyRequirement {
    match value {
        PropertyRequirement::Required => PropertyRequirement::Recommended,
        PropertyRequirement::Recommended => PropertyRequirement::Optional,
        PropertyRequirement::Optional | PropertyRequirement::Conditional(_) => {
            PropertyRequirement::Conditional(
                PropertyCondition::new(
                    ConditionId::new("condition").unwrap_or_else(|_| unreachable!()),
                    "Explain when this property applies.",
                )
                .unwrap_or_else(|_| unreachable!()),
            )
        }
    }
}

fn previous_requirement(value: &PropertyRequirement) -> PropertyRequirement {
    match value {
        PropertyRequirement::Required => PropertyRequirement::Conditional(
            PropertyCondition::new(
                ConditionId::new("condition").unwrap_or_else(|_| unreachable!()),
                "Explain when this property applies.",
            )
            .unwrap_or_else(|_| unreachable!()),
        ),
        PropertyRequirement::Recommended => PropertyRequirement::Required,
        PropertyRequirement::Optional => PropertyRequirement::Recommended,
        PropertyRequirement::Conditional(_) => PropertyRequirement::Optional,
    }
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
    use std::cell::{Cell, RefCell};

    use gitserious_app::{GlobalConfiguration, ProjectLock, ProjectState, RepositoryRoot};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    struct Workspace {
        global: RefCell<ConfigurationSession>,
        project: RefCell<ConfigurationSession>,
        saves: Cell<usize>,
    }

    impl Workspace {
        fn new() -> Self {
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
            match session.destination() {
                ConfigurationDestination::Global => *self.global.borrow_mut() = session.clone(),
                ConfigurationDestination::Project => *self.project.borrow_mut() = session.clone(),
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

    fn scrollbar_rows(terminal: &Terminal<TestBackend>, pane: Rect) -> Vec<u16> {
        (pane.y + 1..pane.bottom().saturating_sub(1))
            .filter(|&y| terminal.backend().buffer()[(pane.right() - 1, y)].symbol() == "█")
            .collect()
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
        assert!(row_text(&terminal, 0, 0, 100).contains("Config Options"));
        assert_eq!(buffer[(0, 0)].symbol(), "┌");
        assert_eq!(buffer[(99, 0)].symbol(), "┐");
        assert_eq!(buffer[(0, 2)].symbol(), "└");
        assert_eq!(buffer[(1, 2)].symbol(), "─");
        assert_eq!(buffer[(99, 2)].symbol(), "┘");
        assert_eq!(buffer[(0, 3)].symbol(), "┌");
        assert_eq!(buffer[(99, 3)].symbol(), "┐");
        assert_ne!(buffer[(1, 4)].symbol(), " ");
        assert_eq!(buffer[(1, 4)].fg, Color::Yellow);
        let project_columns = pane_columns(Rect::new(0, 3, 100, 26));
        assert!(
            row_text(
                &terminal,
                project_columns[1].x + 1,
                project_columns[1].y + 1,
                project_columns[1].width - 2,
            )
            .starts_with("Commit taxonomy")
        );
        assert_eq!(buffer[(2, 1)].bg, Color::Yellow);
        assert_eq!(buffer[(0, 28)].symbol(), "└");
        assert_eq!(buffer[(0, 29)].symbol(), "t");
        assert_eq!(buffer[(3, 29)].symbol(), ":");
        assert!((0..100).all(|x| buffer[(x, 29)].bg == Color::Yellow));

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
        let team = TaxonomyId::new("team")?;
        authored.fork_taxonomy(&TaxonomyId::new("conventional")?, team.clone())?;
        let config = ProjectConfig::new(1, Some(team.clone()), Some(vec![team]))?;
        let lock = ProjectLock::capture(
            &config,
            &resolve_effective_configuration(authored.global(), &config)?,
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

    #[test]
    fn library_uses_three_panes_and_retains_selection() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        let mut terminal = Terminal::new(TestBackend::new(120, 40))?;
        terminal.draw(|frame| state.render(frame))?;
        let library_tab = state.tab_areas[Category::Library.index()];
        state.handle_event(
            Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: library_tab.x,
                row: library_tab.y,
                modifiers: KeyModifiers::NONE,
            }),
            &workspace,
        );
        terminal.draw(|frame| state.render(frame))?;
        let [taxonomies, types, metadata] = state.library.pane_areas;
        assert_eq!(taxonomies.x, 0);
        assert_eq!(taxonomies.right() + 1, types.x);
        assert_eq!(types.right() + 1, metadata.x);
        assert_eq!(metadata.right(), 120);
        assert!(taxonomies.width.abs_diff(types.width) <= 1);
        assert!(types.width.abs_diff(metadata.width) <= 1);
        assert!(taxonomies.width.abs_diff(metadata.width) <= 1);
        let titles = row_text(&terminal, 0, taxonomies.y, 120);
        for title in ["Taxonomies", "Types", "Type Details"] {
            assert!(titles.contains(title));
        }
        let taxonomy_description = state.library.taxonomy_description_area;
        assert_eq!(taxonomies.bottom(), taxonomy_description.y);
        assert!(
            row_text(
                &terminal,
                taxonomy_description.x,
                taxonomy_description.y,
                taxonomy_description.width,
            )
            .contains("Taxonomy Description")
        );
        let taxonomy_area = state.library.browser_area;
        let type_area = state.library.type_area;
        let built_in = row_text(
            &terminal,
            taxonomy_area.x,
            taxonomy_area.y,
            taxonomy_area.width,
        );
        let spacer = row_text(
            &terminal,
            taxonomy_area.x,
            taxonomy_area.y + 4,
            taxonomy_area.width,
        );
        let custom = row_text(
            &terminal,
            taxonomy_area.x,
            taxonomy_area.y + 5,
            taxonomy_area.width,
        );
        assert!(built_in.contains("Built-in") && built_in.contains('⠒'));
        assert!(spacer.trim().is_empty());
        assert!(custom.contains("Custom") && custom.contains('⠒'));
        assert_eq!(
            terminal.backend().buffer()[(taxonomy_area.right() - 1, taxonomy_area.y)].symbol(),
            "⠒"
        );
        assert!(
            row_text(
                &terminal,
                taxonomy_description.x + 1,
                taxonomy_description.y + 1,
                taxonomy_description.width - 2,
            )
            .contains("The Conventional")
        );
        let first_type = row_text(&terminal, type_area.x, type_area.y, type_area.width);
        assert!(first_type.contains("› feat"));
        assert!(!first_type.contains("feat  3"));
        let first_metadata = row_text(
            &terminal,
            state.library.metadata_area.x,
            state.library.metadata_area.y,
            state.library.metadata_area.width,
        );
        assert!(first_metadata.contains("Description") && first_metadata.contains('⠒'));
        assert_eq!(state.library.metadata_area.x, metadata.x + 1);
        assert_eq!(
            terminal.backend().buffer()[(taxonomy_description.x + 1, taxonomy_description.y + 1)]
                .symbol(),
            "T"
        );
        assert_eq!(
            terminal.backend().buffer()[(
                state.library.metadata_area.right() - 1,
                state.library.metadata_area.y
            )]
                .symbol(),
            "⠒"
        );
        assert!(
            row_text(
                &terminal,
                state.library.metadata_area.x,
                state.library.metadata_area.y + 1,
                state.library.metadata_area.width,
            )
            .starts_with("An addition")
        );
        let properties_y = (state.library.metadata_area.y..state.library.metadata_area.bottom())
            .find(|&y| {
                row_text(
                    &terminal,
                    state.library.metadata_area.x,
                    y,
                    state.library.metadata_area.width,
                )
                .contains("Properties")
            })
            .ok_or("missing properties heading")?;
        assert_eq!(
            terminal.backend().buffer()[(state.library.metadata_area.right() - 1, properties_y)]
                .symbol(),
            "⠒"
        );
        let required = row_text(
            &terminal,
            state.library.metadata_area.x,
            properties_y + 1,
            state.library.metadata_area.width,
        );
        let conditional = row_text(
            &terminal,
            state.library.metadata_area.x,
            properties_y + 3,
            state.library.metadata_area.width,
        );
        assert_eq!(
            required.find("required").ok_or("missing requirement")?,
            conditional.find("conditional").ok_or("missing condition")?
        );
        assert_eq!(
            terminal.backend().buffer()[(taxonomy_area.x + 1, taxonomy_area.y + 1)].bg,
            Color::Yellow
        );
        assert_ne!(
            terminal.backend().buffer()[(type_area.x + 1, type_area.y)].bg,
            Color::Yellow
        );

        state.handle_event(key(KeyCode::Right), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        assert_ne!(
            terminal.backend().buffer()[(taxonomy_area.x + 1, taxonomy_area.y + 1)].bg,
            Color::Yellow
        );
        assert_eq!(
            terminal.backend().buffer()[(type_area.x + 1, type_area.y)].bg,
            Color::Yellow
        );
        state.handle_event(
            Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: type_area.x,
                row: type_area.y + 1,
                modifiers: KeyModifiers::NONE,
            }),
            &workspace,
        );
        assert_eq!(state.library.type_selected, 1);
        assert_eq!(state.library.focus, LibraryFocus::Types);
        state.handle_event(
            Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: taxonomy_area.x,
                row: taxonomy_area.y + 2,
                modifiers: KeyModifiers::NONE,
            }),
            &workspace,
        );
        assert_eq!(state.library.browse_selected, 1);
        state.handle_event(
            Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: taxonomy_area.x,
                row: taxonomy_area.y + 5,
                modifiers: KeyModifiers::NONE,
            }),
            &workspace,
        );
        assert_eq!(state.library.browse_selected, 1);
        assert_eq!(state.library.type_selected, 0);
        assert_eq!(state.library.focus, LibraryFocus::Taxonomies);
        terminal.draw(|frame| state.render(frame))?;
        state.handle_event(
            Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: taxonomy_description.x + 2,
                row: taxonomy_description.y + 1,
                modifiers: KeyModifiers::NONE,
            }),
            &workspace,
        );
        assert_eq!(state.library.browse_selected, 1);
        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(state.library.browse_selected, 2);
        state.handle_event(key(KeyCode::Tab), &workspace);
        state.handle_event(key(KeyCode::Tab), &workspace);
        assert_eq!(state.category, Category::Library);
        assert_eq!(state.library.browse_selected, 2);
        let project_tab = state.tab_areas[Category::Project.index()];
        state.screen = Screen::Fork;
        state.handle_event(
            Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: project_tab.x,
                row: project_tab.y,
                modifiers: KeyModifiers::NONE,
            }),
            &workspace,
        );
        assert_eq!(state.category, Category::Library);
        Ok(())
    }

    #[test]
    fn compact_panels_scroll_without_changing_selection() -> Result<(), Box<dyn std::error::Error>>
    {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        state.project_section = ProjectSection::Files;
        terminal.draw(|frame| state.render(frame))?;
        let project_columns = pane_columns(Rect::new(
            0,
            3,
            MINIMUM_WIDTH,
            MINIMUM_HEIGHT.saturating_sub(4),
        ));
        assert!(!scrollbar_rows(&terminal, project_columns[1]).is_empty());
        state.handle_event(key(KeyCode::PageDown), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        assert!(state.project_scroll > 0);
        assert_eq!(state.project_section, ProjectSection::Files);

        state.category = Category::Library;
        state.library.focus = LibraryFocus::Types;
        terminal.draw(|frame| state.render(frame))?;
        assert!(state.library.type_area.height > 0);
        let [_, types, details] = state.library.pane_areas;
        assert!(!scrollbar_rows(&terminal, types).is_empty());
        assert!(!scrollbar_rows(&terminal, details).is_empty());
        assert_eq!(types.bottom(), details.y);
        assert_eq!(
            state.library.pane_areas[0].bottom(),
            state.library.taxonomy_description_area.y
        );
        let type_area = state.library.type_area;
        let taxonomy_area = state.library.browser_area;
        assert_eq!(
            terminal.backend().buffer()[(type_area.x + 1, type_area.y)].bg,
            Color::Yellow
        );
        assert_ne!(
            terminal.backend().buffer()[(taxonomy_area.x + 1, taxonomy_area.y + 1)].bg,
            Color::Yellow
        );
        let detail_heading = row_text(&terminal, details.x, details.y, details.width);
        assert!(detail_heading.contains("Type Details"));
        assert!(
            row_text(
                &terminal,
                state.library.metadata_area.x,
                state.library.metadata_area.y,
                state.library.metadata_area.width
            )
            .contains("Description")
        );
        assert!(
            row_text(
                &terminal,
                state.library.metadata_area.x,
                state.library.metadata_area.y + 1,
                state.library.metadata_area.width,
            )
            .contains("An addition")
        );
        state.handle_event(key(KeyCode::PageDown), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        assert!(state.library.property_scroll > 0);
        assert!(text(&terminal).contains("Properties"));
        assert!(
            row_text(
                &terminal,
                state.library.metadata_area.x,
                state.library.metadata_area.y + 2,
                state.library.metadata_area.width,
            )
            .contains("intent")
        );
        assert_eq!(state.library.browse_selected, 0);
        assert_eq!(state.library.type_selected, 0);
        Ok(())
    }

    #[test]
    fn library_switches_layout_at_the_content_width_boundary()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Library;
        let mut compact = Terminal::new(TestBackend::new(95, 30))?;
        compact.draw(|frame| state.render(frame))?;
        assert_eq!(
            state.library.pane_areas[1].bottom(),
            state.library.pane_areas[2].y
        );
        assert!(text(&compact).contains("Type Details"));

        let mut wide = Terminal::new(TestBackend::new(96, 30))?;
        wide.draw(|frame| state.render(frame))?;
        let [taxonomies, types, metadata] = state.library.pane_areas;
        assert_eq!(taxonomies.right() + 1, types.x);
        assert_eq!(types.right() + 1, metadata.x);
        assert_eq!(metadata.right(), 96);
        assert!(taxonomies.width.abs_diff(types.width) <= 1);
        assert!(types.width.abs_diff(metadata.width) <= 1);
        assert!(text(&wide).contains("Type Details"));
        Ok(())
    }

    #[test]
    fn compact_details_align_property_columns() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Library;
        let mut terminal = Terminal::new(TestBackend::new(80, 35))?;
        terminal.draw(|frame| state.render(frame))?;
        let details = state.library.pane_areas[2];
        assert_eq!(state.library.pane_areas[1].bottom(), details.y);
        assert!(row_text(&terminal, details.x, details.y, details.width).contains("Type Details"));
        let area = state.library.metadata_area;
        let required = row_text(&terminal, area.x, area.y + 4, area.width);
        let conditional = row_text(&terminal, area.x, area.y + 6, area.width);
        assert_eq!(
            required.find("required").ok_or("missing requirement")?,
            conditional.find("conditional").ok_or("missing condition")?
        );
        Ok(())
    }

    #[test]
    fn taxonomy_description_scrolls_to_fork_lineage() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Library;
        let target = TaxonomyId::new("team")?;
        let session = state.global.as_mut().ok_or("missing global")?;
        session.fork_taxonomy(&TaxonomyId::new("conventional")?, target.clone())?;
        let original = session
            .catalog()?
            .find(&target)
            .cloned()
            .ok_or("missing fork")?;
        let description = Description::new("A lengthy taxonomy description. ".repeat(40))?;
        session.update_taxonomy(revised_taxonomy(
            &original,
            description,
            original.commit_types().to_vec(),
        )?)?;
        state.library.browse_selected = 3;
        let mut terminal = Terminal::new(TestBackend::new(120, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        let pane = state.library.taxonomy_description_area;
        assert_eq!(
            terminal.backend().buffer()[(pane.right() - 1, pane.y + 1)].symbol(),
            "█"
        );
        for _ in 0..3 {
            state.handle_event(key(KeyCode::PageDown), &workspace);
            terminal.draw(|frame| state.render(frame))?;
        }
        assert!(state.library.taxonomy_scroll > 0);
        assert!(text(&terminal).contains("Forked from conventional@1"));
        Ok(())
    }

    #[test]
    fn library_mouse_targets_follow_the_scrolled_taxonomy_list()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Library;
        let session = state.global.as_mut().ok_or("missing global")?;
        for index in 0..8 {
            session.fork_taxonomy(
                &TaxonomyId::new("conventional")?,
                TaxonomyId::new(format!("team-{index}"))?,
            )?;
        }
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        for _ in 0..8 {
            state.handle_event(key(KeyCode::Down), &workspace);
        }
        terminal.draw(|frame| state.render(frame))?;
        let list = state.library.browser_area;
        let offset = state.library.browser_offset;
        assert!(offset > 0);
        assert!(!scrollbar_rows(&terminal, state.library.pane_areas[0]).is_empty());
        let (row, expected) = (list.y..list.bottom())
            .find_map(|row| {
                state
                    .library
                    .taxonomy_at(list.x, row, state.catalog().map_or(0, |items| items.len()))
                    .map(|index| (row, index))
            })
            .ok_or("no visible taxonomy")?;
        state.handle_event(
            Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: list.x,
                row,
                modifiers: KeyModifiers::NONE,
            }),
            &workspace,
        );
        assert_eq!(state.library.browse_selected, expected);
        assert_eq!(state.library.focus, LibraryFocus::Taxonomies);
        Ok(())
    }

    #[test]
    fn taxonomy_selection_skips_section_spacer_and_headings()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Library;
        state
            .global
            .as_mut()
            .ok_or("missing global")?
            .fork_taxonomy(&TaxonomyId::new("conventional")?, TaxonomyId::new("team")?)?;
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        let list = state.library.browser_area;
        assert!(
            row_text(&terminal, list.x, list.y + 4, list.width)
                .trim()
                .is_empty()
        );
        assert!(row_text(&terminal, list.x, list.y + 5, list.width).contains("Custom"));
        assert!(row_text(&terminal, list.x, list.y + 6, list.width).contains("team"));
        state.library.select_taxonomy(2);
        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(state.library.browse_selected, 3);
        state.handle_event(key(KeyCode::Up), &workspace);
        assert_eq!(state.library.browse_selected, 2);
        for row in [list.y + 4, list.y + 5] {
            state.handle_event(
                Event::Mouse(ratatui::crossterm::event::MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: list.x,
                    row,
                    modifiers: KeyModifiers::NONE,
                }),
                &workspace,
            );
            assert_eq!(state.library.browse_selected, 2);
        }
        state.handle_event(
            Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: list.x,
                row: list.y + 6,
                modifiers: KeyModifiers::NONE,
            }),
            &workspace,
        );
        assert_eq!(state.library.browse_selected, 3);
        Ok(())
    }

    #[test]
    fn empty_custom_action_is_selectable_and_disappears_after_creation()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Library;
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        let list = state.library.browser_area;
        assert!(row_text(&terminal, list.x, list.y + 6, list.width).contains("+ Create New"));
        state.library.select_taxonomy(2);
        state.handle_event(key(KeyCode::Down), &workspace);
        assert!(state.library.create_selected);
        state.handle_event(key(KeyCode::Right), &workspace);
        assert_eq!(state.library.focus, LibraryFocus::Taxonomies);
        state.handle_event(key(KeyCode::Left), &workspace);
        assert_eq!(state.library.focus, LibraryFocus::Taxonomies);
        terminal.draw(|frame| state.render(frame))?;
        assert!(!row_text(&terminal, list.x, list.y + 3, list.width).contains('›'));
        assert!(row_text(&terminal, list.x, list.y + 6, list.width).contains("› + Create New"));
        assert!(text(&terminal).contains("n/a"));
        state.handle_event(key(KeyCode::Up), &workspace);
        assert!(!state.library.create_selected);
        assert_eq!(state.library.browse_selected, 2);
        state.handle_event(key(KeyCode::Right), &workspace);
        assert_eq!(state.library.focus, LibraryFocus::Types);
        state.handle_event(key(KeyCode::Left), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(
            state.screen,
            Screen::TaxonomyEditor(EditorMode::Create)
        ));
        assert_eq!(workspace.saves.get(), 0);
        state.handle_event(key(KeyCode::Esc), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(state.library.create_selected);
        state
            .global
            .as_mut()
            .ok_or("missing global")?
            .fork_taxonomy(&TaxonomyId::new("conventional")?, TaxonomyId::new("team")?)?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(!state.library.create_selected);
        assert_eq!(state.library.browse_selected, 3);
        assert!(!text(&terminal).contains("+ Create New"));
        Ok(())
    }

    #[test]
    fn empty_custom_action_click_uses_dirty_scope_guard() -> Result<(), Box<dyn std::error::Error>>
    {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Library;
        state
            .project
            .as_mut()
            .ok_or("missing project")?
            .initialize_project()?;
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        let list = state.library.browser_area;
        state.handle_event(
            Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: list.x,
                row: list.y + 6,
                modifiers: KeyModifiers::NONE,
            }),
            &workspace,
        );
        assert!(state.library.create_selected);
        assert!(matches!(
            state.screen,
            Screen::ScopeChange(PendingAction::Create)
        ));
        assert_eq!(workspace.saves.get(), 0);
        state.handle_event(key(KeyCode::Esc), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(state.library.create_selected);
        Ok(())
    }

    #[test]
    fn home_palette_filters_commands_and_routes_through_existing_actions()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Library;
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(
            row_text(&terminal, 0, 29, 100)
                .contains("tab: switch | ←/→: focus | ↑/↓: move | /: commands | ?: help")
        );
        state.handle_event(key(KeyCode::Char('/')), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        assert!(text(&terminal).contains("Commands"));
        assert!(text(&terminal).contains("╔"));
        let Some(HomeOverlay::Palette(palette)) = &state.home_overlay else {
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
        let Some(HomeOverlay::Palette(palette)) = &state.home_overlay else {
            return Err("palette missing".into());
        };
        assert_eq!(palette.visible(state.category), vec![HomeCommand::Fork]);
        assert_eq!(
            terminal.backend().buffer()[(palette.list_area.right() - 1, palette.list_area.y)]
                .symbol(),
            "f"
        );
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(state.screen, Screen::Fork));
        assert!(state.home_overlay.is_none());
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }

    #[test]
    fn palette_blocks_underlying_keys_and_reports_unavailable_actions()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Library;
        state.handle_event(key(KeyCode::Char('/')), &workspace);
        state.handle_event(Event::Paste("edit".into()), &workspace);
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        let Some(HomeOverlay::Palette(palette)) = &state.home_overlay else {
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
    fn project_palette_and_help_keep_the_review_boundary() -> Result<(), Box<dyn std::error::Error>>
    {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        state.handle_event(key(KeyCode::Char('?')), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        assert!(text(&terminal).contains("Initialize project"));
        let help = centered_rect(58, 12, Rect::new(0, 3, 100, 26));
        let content_x = help.x + 2;
        let content_width = help.width - 4;
        let switch_row = row_text(&terminal, content_x, help.y + 2, content_width);
        let review_row = row_text(&terminal, content_x, help.y + 6, content_width);
        assert!(switch_row.starts_with("Switch Project / Library"));
        assert!(switch_row.ends_with("tab"));
        assert!(review_row.starts_with("Review changes"));
        assert!(review_row.ends_with("ctrl+s"));
        assert!(text(&terminal).contains("[esc] close"));
        assert!(row_text(&terminal, 0, 29, 100).contains("/: commands | ?: help"));
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        assert!(matches!(state.home_overlay, Some(HomeOverlay::Help)));
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(state.home_overlay, Some(HomeOverlay::Help)));
        state.handle_event(key(KeyCode::Char('?')), &workspace);
        assert!(matches!(state.home_overlay, Some(HomeOverlay::Help)));
        state.handle_event(key(KeyCode::Esc), &workspace);
        assert!(state.home_overlay.is_none());
        state.handle_event(key(KeyCode::Char('/')), &workspace);
        state.handle_event(Event::Paste("initialize".into()), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(
            state.screen,
            Screen::Review(ConfigurationDestination::Project)
        ));
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }

    #[test]
    fn library_help_aligns_shortcuts_and_closes_only_with_escape()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Library;
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        state.handle_event(key(KeyCode::Char('?')), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        let help = centered_rect(58, 12, Rect::new(0, 3, MINIMUM_WIDTH, MINIMUM_HEIGHT - 4));
        let content_x = help.x + 2;
        let content_width = help.width - 4;
        let switch_row = row_text(&terminal, content_x, help.y + 2, content_width);
        let edit_row = row_text(&terminal, content_x, help.y + 6, content_width);
        assert!(switch_row.starts_with("Switch Project / Library"));
        assert!(switch_row.ends_with("tab"));
        assert!(edit_row.starts_with("New / edit / fork / delete"));
        assert!(edit_row.ends_with("n/e/f/d"));
        state.handle_event(key(KeyCode::Enter), &workspace);
        state.handle_event(
            Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            }),
            &workspace,
        );
        assert!(matches!(state.home_overlay, Some(HomeOverlay::Help)));
        state.handle_event(key(KeyCode::Esc), &workspace);
        assert!(state.home_overlay.is_none());
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
        state.category = Category::Library;
        state.handle_event(key(KeyCode::Char('/')), &workspace);
        state.handle_event(Event::Paste("new".into()), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(
            state.screen,
            Screen::ScopeChange(PendingAction::Create)
        ));
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }

    #[test]
    fn command_popup_fits_the_minimum_and_blocks_underlying_mouse_targets()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Library;
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(row_text(&terminal, 0, 17, MINIMUM_WIDTH).ends_with("?: help"));
        let project_tab = state.tab_areas[Category::Project.index()];
        state.handle_event(key(KeyCode::Char('/')), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        assert!(text(&terminal).contains("Commands"));
        assert!(text(&terminal).contains("╔"));
        assert!(text(&terminal).contains("Search"));
        assert!(text(&terminal).contains("▌"));
        let Some(HomeOverlay::Palette(palette)) = &state.home_overlay else {
            return Err("palette missing".into());
        };
        assert_eq!(palette.list_area.height, 7);
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
        let Some(HomeOverlay::Palette(palette)) = &state.home_overlay else {
            return Err("palette missing".into());
        };
        assert_eq!(palette.selected, 7);
        assert_eq!(palette.list_offset, 1);
        assert!(
            row_text(
                &terminal,
                palette.list_area.x,
                palette.list_area.bottom() - 1,
                palette.list_area.width
            )
            .contains("Quit")
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
        let Some(HomeOverlay::Palette(palette)) = &state.home_overlay else {
            return Err("palette missing".into());
        };
        assert_eq!(palette.selected, 1);
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
        assert_eq!(state.category, Category::Library);
        Ok(())
    }

    #[test]
    fn child_lists_show_scrollbars_when_selection_overflows()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        for index in 0..12 {
            state
                .global
                .as_mut()
                .ok_or("missing global")?
                .fork_taxonomy(
                    &TaxonomyId::new("conventional")?,
                    TaxonomyId::new(format!("team-{index}"))?,
                )?;
        }
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        let body = Rect::new(0, 3, MINIMUM_WIDTH, MINIMUM_HEIGHT - 4);
        state.open_settings(ConfigurationDestination::Global);
        terminal.draw(|frame| state.render(frame))?;
        let settings_area = Rect::new(body.x, body.y + 2, body.width, body.height - 2);
        assert!(!scrollbar_rows(&terminal, settings_area).is_empty());

        let original = state.selected_taxonomy().ok_or("missing taxonomy")?;
        let mut draft = TaxonomyDraft::from_taxonomy(&original);
        let extra_type = draft.types[0].clone();
        draft.types.extend([extra_type.clone(), extra_type]);
        state.editor = Some(draft);
        state.child_selected = 13;
        state.screen = Screen::TaxonomyEditor(EditorMode::Edit);
        terminal.draw(|frame| state.render(frame))?;
        assert!(!scrollbar_rows(&terminal, body).is_empty());

        let draft = state.editor.as_mut().ok_or("missing editor")?;
        let extra_property = draft.types[0].properties[0].clone();
        draft.types[0]
            .properties
            .extend((0..13).map(|_| extra_property.clone()));
        state.child_selected = 14;
        state.screen = Screen::TypeEditor(0);
        terminal.draw(|frame| state.render(frame))?;
        assert!(!scrollbar_rows(&terminal, body).is_empty());
        Ok(())
    }

    #[test]
    fn form_fork_and_review_text_scroll_with_visible_thumbs()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        let body = Rect::new(0, 3, MINIMUM_WIDTH, MINIMUM_HEIGHT - 4);
        state.form = Some(FormState::metadata(
            "team".into(),
            "Long description. ".repeat(90),
        ));
        state.screen = Screen::Form(FormKind::TaxonomyMetadata);
        terminal.draw(|frame| state.render(frame))?;
        assert!(!scrollbar_rows(&terminal, body).is_empty());
        state.handle_event(key(KeyCode::PageDown), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        assert!(state.form_scroll > 0);

        state.category = Category::Library;
        state.screen = Screen::Fork;
        state.fork_target = "very-long-name-".repeat(70);
        terminal.draw(|frame| state.render(frame))?;
        assert!(!scrollbar_rows(&terminal, body).is_empty());
        state.handle_event(key(KeyCode::PageDown), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        assert!(state.fork_scroll > 0);

        state
            .global
            .as_mut()
            .ok_or("missing global")?
            .fork_taxonomy(&TaxonomyId::new("conventional")?, TaxonomyId::new("team")?)?;
        for index in 0..40 {
            state
                .global
                .as_mut()
                .ok_or("missing global")?
                .fork_taxonomy(
                    &TaxonomyId::new("conventional")?,
                    TaxonomyId::new(format!("more-{index}"))?,
                )?;
        }
        let session = state.global.as_mut().ok_or("missing global")?;
        let available = session
            .catalog()?
            .taxonomies()
            .iter()
            .map(|taxonomy| taxonomy.id().clone())
            .collect();
        session.configure_global(TaxonomyId::new("conventional")?, available)?;
        state.active_scope = ConfigurationDestination::Global;
        state.open_review();
        terminal.draw(|frame| state.render(frame))?;
        assert!(!scrollbar_rows(&terminal, body).is_empty());
        state.handle_event(key(KeyCode::Down), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        assert!(state.scroll > 0);
        Ok(())
    }

    #[test]
    fn overlay_notices_replace_the_message_row() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        state.status = Some(Notice::Error("Invalid selection".into()));
        terminal.draw(|frame| state.render(frame))?;
        assert_eq!(terminal.backend().buffer()[(0, 26)].symbol(), "│");
        assert_eq!(terminal.backend().buffer()[(45, 27)].fg, Color::Red);
        assert!(text(&terminal).contains("Invalid selection"));

        state.status = Some(Notice::Info("Configuration applied.".into()));
        terminal.draw(|frame| state.render(frame))?;
        assert_eq!(terminal.backend().buffer()[(45, 27)].fg, Color::White);
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
        assert_eq!(terminal.backend().buffer()[(45, 27)].fg, Color::Red);

        state.status = Some(Notice::Info("Review pending".into()));
        terminal.draw(|frame| state.render(frame))?;
        assert_eq!(terminal.backend().buffer()[(45, 27)].fg, Color::White);
        assert!(text(&terminal).contains("Review pending"));

        state.status = None;
        state.category = Category::Library;
        state.global = None;
        state.global_error = "global load failed".into();
        terminal.draw(|frame| state.render(frame))?;
        assert_eq!(terminal.backend().buffer()[(45, 27)].fg, Color::Red);
        assert!(text(&terminal).contains("global load failed"));
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
        assert!(text(&terminal).contains("unapplied changes"));
        assert_eq!(terminal.backend().buffer()[(83, 1)].fg, Color::Yellow);
        state.handle_event(key(KeyCode::Tab), &workspace);
        assert_eq!(state.category, Category::Library);
        terminal.draw(|frame| state.render(frame))?;
        assert!(text(&terminal).contains("Library"));
        assert!(state.scope_dirty(ConfigurationDestination::Project));
        assert_eq!(workspace.saves.get(), 0);
        state.handle_event(key(KeyCode::Char('e')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(matches!(state.status, Some(Notice::Error(_))));
        assert_eq!(workspace.saves.get(), 0);
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        assert!(matches!(
            state.screen,
            Screen::ScopeChange(PendingAction::Create)
        ));
        assert_eq!(workspace.saves.get(), 0);
        state.handle_event(key(KeyCode::Esc), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert_eq!(state.category, Category::Library);
        assert!(state.scope_dirty(ConfigurationDestination::Project));
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        state.handle_event(key(KeyCode::Char('a')), &workspace);
        assert_eq!(workspace.saves.get(), 1);
        assert!(matches!(
            state.screen,
            Screen::TaxonomyEditor(EditorMode::Create)
        ));
        assert_eq!(state.active_scope, ConfigurationDestination::Global);
        Ok(())
    }

    #[test]
    fn taxonomy_commands_target_selected_global_definition()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Library;
        state.handle_event(key(KeyCode::Char('e')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(matches!(state.status, Some(Notice::Error(_))));
        state.handle_event(key(KeyCode::Char('f')), &workspace);
        assert!(matches!(state.screen, Screen::Fork));
        state.handle_event(key(KeyCode::Esc), &workspace);
        state.handle_event(key(KeyCode::Char('d')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        state
            .global
            .as_mut()
            .ok_or("missing global")?
            .fork_taxonomy(&TaxonomyId::new("conventional")?, TaxonomyId::new("team")?)?;
        state.library.browse_selected = 3;
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(text(&terminal).contains("Forked from"));
        assert!(text(&terminal).contains("conventional@1"));
        let list = state.library.browser_area;
        assert!(
            row_text(&terminal, list.x, list.y + 4, list.width)
                .trim()
                .is_empty()
        );
        assert!(row_text(&terminal, list.x, list.y + 5, list.width).contains("Custom"));
        assert!(row_text(&terminal, list.x, list.y + 6, list.width).contains("team"));
        state.handle_event(key(KeyCode::Char('e')), &workspace);
        assert!(matches!(
            state.screen,
            Screen::TaxonomyEditor(EditorMode::Edit)
        ));
        state.handle_event(key(KeyCode::Esc), &workspace);
        state.handle_event(key(KeyCode::Char('d')), &workspace);
        assert!(matches!(state.screen, Screen::Delete));
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        assert!(matches!(
            state.screen,
            Screen::Review(ConfigurationDestination::Global)
        ));
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }

    #[test]
    fn unavailable_library_keeps_the_frame_and_reports_the_error()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Library;
        state.global = None;
        state.global_error = "global load failed".into();
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        let rendered = text(&terminal);
        assert!(rendered.contains("Library"));
        assert!(rendered.contains("Taxonomies"));
        assert!(rendered.contains("Details"));
        assert!(rendered.contains("Taxonomy library"));
        assert!(rendered.contains("No commit types."));
        assert!(rendered.contains("No type selected."));
        assert!(rendered.contains("/: commands | ?: help"));
        assert!(rendered.contains("global load failed"));
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(matches!(state.status, Some(Notice::Error(_))));
        Ok(())
    }

    #[test]
    fn global_review_resumes_after_discarding_project_draft()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state
            .project
            .as_mut()
            .ok_or("missing project")?
            .initialize_project()?;
        state.category = Category::Library;
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        assert!(matches!(
            state.screen,
            Screen::ScopeChange(PendingAction::ReviewGlobal)
        ));
        state.handle_event(key(KeyCode::Char('d')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert_eq!(state.active_scope, ConfigurationDestination::Global);
        assert!(!state.scope_dirty(ConfigurationDestination::Project));
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }

    #[test]
    fn review_is_the_only_save_boundary() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.active_scope = ConfigurationDestination::Global;
        state
            .global
            .as_mut()
            .ok_or("missing global")?
            .fork_taxonomy(&TaxonomyId::new("conventional")?, TaxonomyId::new("team")?)?;
        state.open_review();
        assert!(matches!(
            state.screen,
            Screen::Review(ConfigurationDestination::Global)
        ));
        assert_eq!(workspace.saves.get(), 0);
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert_eq!(workspace.saves.get(), 1);
        Ok(())
    }

    #[test]
    fn too_small_view_blocks_hidden_actions() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        let mut terminal = Terminal::new(TestBackend::new(59, 17))?;
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
        assert_eq!(buffer[(0, 2)].symbol(), "└");
        assert_eq!(buffer[(0, 16)].symbol(), "└");
        Ok(())
    }
}
