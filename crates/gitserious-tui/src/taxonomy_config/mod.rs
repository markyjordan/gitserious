use std::io::{self, IsTerminal};

use gitserious_app::{
    ConfigurationDestination, ConfigurationEditor, ConfigurationSession, ConfigurationWorkspace,
    TaxonomyOrigin, revised_taxonomy,
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
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::theme::{
    JET_BLACK, MINIMUM_HEIGHT, MINIMUM_WIDTH, ZEBRA_BACKGROUND, centered_rect, frame_style,
    navigation_key_style, normalize_background, render_navigation_row, section_heading_style,
};

/// Taxonomy-first terminal configuration adapter.
#[derive(Clone, Copy, Debug, Default)]
pub struct RatatuiTaxonomyConfigurationEditor;

impl ConfigurationEditor for RatatuiTaxonomyConfigurationEditor {
    fn edit(&self, workspace: &dyn ConfigurationWorkspace) -> Result<(), String> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err("configuration editing requires an interactive terminal".into());
        }
        ratatui::run(|terminal| {
            let _guard = TerminalGuard::enable()?;
            let mut state = State::new(workspace);
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
enum HomeItem {
    Project,
    Global,
    Browse,
    Create,
}

impl HomeItem {
    const ALL: [Self; 4] = [Self::Project, Self::Global, Self::Browse, Self::Create];

    const fn label(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Global => "Global",
            Self::Browse => "Browse",
            Self::Create => "Create",
        }
    }

    const fn scope(self) -> ConfigurationDestination {
        match self {
            Self::Project => ConfigurationDestination::Project,
            Self::Global | Self::Browse | Self::Create => ConfigurationDestination::Global,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditorMode {
    Create,
    Edit,
}

#[derive(Clone, Debug)]
enum Screen {
    Home,
    Browse,
    TaxonomyDetail,
    CommitTypeDetail(usize),
    Settings(ConfigurationDestination),
    TaxonomyEditor(EditorMode),
    TypeEditor(usize),
    Form(FormKind),
    Fork,
    Delete,
    Review(ConfigurationDestination),
    Leave,
    ScopeChange(HomeItem),
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
    global: Option<ConfigurationSession>,
    global_error: String,
    project: Option<ConfigurationSession>,
    project_error: String,
    active_scope: ConfigurationDestination,
    screen: Screen,
    home_selected: usize,
    browse_selected: usize,
    child_selected: usize,
    scroll: u16,
    status: String,
    editor: Option<TaxonomyDraft>,
    editor_mode: EditorMode,
    form: Option<FormState>,
    settings: Option<SettingsDraft>,
    fork_target: String,
    too_small: bool,
    home_area: Rect,
    browser_area: Rect,
}

impl State {
    fn new(workspace: &dyn ConfigurationWorkspace) -> Self {
        let global_result = workspace.load(ConfigurationDestination::Global);
        let project_result = workspace.load(ConfigurationDestination::Project);
        let (global, global_error) = split_result(global_result);
        let (project, project_error) = split_result(project_result);
        Self {
            global,
            global_error,
            project,
            project_error,
            active_scope: ConfigurationDestination::Project,
            screen: Screen::Home,
            home_selected: 0,
            browse_selected: 0,
            child_selected: 0,
            scroll: 0,
            status: String::new(),
            editor: None,
            editor_mode: EditorMode::Create,
            form: None,
            settings: None,
            fork_target: String::new(),
            too_small: false,
            home_area: Rect::default(),
            browser_area: Rect::default(),
        }
    }

    fn handle_event(&mut self, event: Event, workspace: &dyn ConfigurationWorkspace) -> bool {
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
                    && contains(self.home_area, mouse.column, mouse.row)
                {
                    self.home_selected = usize::from(mouse.row.saturating_sub(self.home_area.y))
                        .min(HomeItem::ALL.len() - 1);
                    self.open_home_item(workspace);
                } else if matches!(self.screen, Screen::Browse)
                    && contains(self.browser_area, mouse.column, mouse.row)
                {
                    self.browse_selected =
                        usize::from(mouse.row.saturating_sub(self.browser_area.y));
                }
            }
            return false;
        }
        let Event::Key(key) = event else {
            return false;
        };
        if key.kind == KeyEventKind::Release {
            return false;
        }
        if self.too_small && !matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
            return false;
        }
        match self.screen.clone() {
            Screen::Home => self.home_key(key, workspace),
            Screen::Browse => self.browse_key(key, workspace),
            Screen::TaxonomyDetail => self.taxonomy_detail_key(key),
            Screen::CommitTypeDetail(_) => self.detail_key(key),
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

    fn home_key(&mut self, key: KeyEvent, workspace: &dyn ConfigurationWorkspace) -> bool {
        match key.code {
            KeyCode::Up => self.home_selected = self.home_selected.saturating_sub(1),
            KeyCode::Down => {
                self.home_selected = (self.home_selected + 1).min(HomeItem::ALL.len() - 1)
            }
            KeyCode::Enter => self.open_home_item(workspace),
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.open_review()
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

    fn open_home_item(&mut self, _workspace: &dyn ConfigurationWorkspace) {
        let item = HomeItem::ALL[self.home_selected];
        if item.scope() != self.active_scope && self.scope_dirty(self.active_scope) {
            self.screen = Screen::ScopeChange(item);
            return;
        }
        self.active_scope = item.scope();
        match item {
            HomeItem::Project => {
                let Some(session) = &mut self.project else {
                    self.status = self.project_error.clone();
                    return;
                };
                if session.project_config().is_none() {
                    match session
                        .initialize_project()
                        .and_then(|()| session.review().map(|_| ()))
                    {
                        Ok(()) => self.screen = Screen::Review(ConfigurationDestination::Project),
                        Err(error) => self.status = error,
                    }
                } else {
                    self.open_settings(ConfigurationDestination::Project);
                }
            }
            HomeItem::Global => self.open_settings(ConfigurationDestination::Global),
            HomeItem::Browse => self.screen = Screen::Browse,
            HomeItem::Create => {
                self.editor = Some(TaxonomyDraft::empty());
                self.editor_mode = EditorMode::Create;
                self.screen = Screen::TaxonomyEditor(EditorMode::Create);
            }
        }
    }

    fn browse_key(&mut self, key: KeyEvent, _workspace: &dyn ConfigurationWorkspace) -> bool {
        let count = self.catalog().map_or(0, |items| items.len());
        match key.code {
            KeyCode::Up => self.browse_selected = self.browse_selected.saturating_sub(1),
            KeyCode::Down => {
                self.browse_selected = (self.browse_selected + 1).min(count.saturating_sub(1))
            }
            KeyCode::Enter => {
                self.child_selected = 0;
                self.screen = Screen::TaxonomyDetail;
            }
            KeyCode::Char('e') => self.open_edit(),
            KeyCode::Char('f') => {
                self.fork_target.clear();
                self.screen = Screen::Fork;
            }
            KeyCode::Char('d') => self.screen = Screen::Delete,
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.open_review()
            }
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Home,
            _ => {}
        }
        false
    }

    fn taxonomy_detail_key(&mut self, key: KeyEvent) -> bool {
        let count = self
            .selected_taxonomy()
            .map_or(0, |taxonomy| taxonomy.commit_types().len());
        match key.code {
            KeyCode::Up => self.child_selected = self.child_selected.saturating_sub(1),
            KeyCode::Down => {
                self.child_selected = (self.child_selected + 1).min(count.saturating_sub(1))
            }
            KeyCode::Enter if count > 0 => {
                self.screen = Screen::CommitTypeDetail(self.child_selected)
            }
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Browse,
            _ => {}
        }
        false
    }

    fn detail_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(10),
            KeyCode::Esc | KeyCode::Char('q') => {
                self.scroll = 0;
                self.screen = Screen::TaxonomyDetail;
            }
            _ => {}
        }
        false
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
                        self.status =
                            "Choose another default before removing this taxonomy.".into();
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
                        self.status =
                            "Configuration change staged. Ctrl+S reviews before applying.".into();
                    }
                    Err(error) => self.status = error,
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
                self.screen = if matches!(mode, EditorMode::Edit) {
                    Screen::Browse
                } else {
                    Screen::Home
                };
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
                        self.screen = Screen::Browse;
                        self.status = "Fork staged. Ctrl+S reviews before applying.".into();
                    }
                    Err(error) => self.status = error,
                }
            }
            KeyCode::Esc => self.screen = Screen::Browse,
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
                        self.browse_selected = self.browse_selected.saturating_sub(1);
                        self.screen = Screen::Browse;
                        self.status = "Deletion staged. Ctrl+S reviews before applying.".into();
                    }
                    Err(error) => {
                        self.screen = Screen::Browse;
                        self.status = error;
                    }
                }
            }
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('n') => self.screen = Screen::Browse,
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
                        self.status = "Configuration applied.".into();
                    }
                    Err(error) => {
                        self.screen = Screen::Home;
                        self.status = error;
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
        target: HomeItem,
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
                        self.active_scope = target.scope();
                        self.screen = Screen::Home;
                        self.open_home_item(workspace);
                    }
                    Err(error) => {
                        self.screen = Screen::Home;
                        self.status = error;
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
                self.active_scope = target.scope();
                self.screen = Screen::Home;
                self.open_home_item(workspace);
            }
            KeyCode::Esc | KeyCode::Enter => self.screen = Screen::Home,
            _ => {}
        }
        false
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
                self.status = error;
            } else {
                self.scroll = 0;
                self.screen = Screen::Review(destination);
            }
        } else {
            self.status = "No changes or taxonomy updates to apply.".into();
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
            self.status = "Built-in taxonomies are read-only; fork one to customize it.".into();
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
            Err(error) => self.status = error,
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
                self.screen = if matches!(mode, EditorMode::Edit) {
                    Screen::Browse
                } else {
                    Screen::Home
                };
                self.status = "Taxonomy change staged. Ctrl+S reviews before applying.".into();
            }
            Err(error) => self.status = error,
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

    fn selected_taxonomy(&self) -> Option<Taxonomy> {
        self.catalog()?.get(self.browse_selected).cloned()
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
        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);
        self.render_header(frame, rows[0]);
        let hints = match self.screen.clone() {
            Screen::Home => {
                self.render_home(frame, rows[1]);
                vec![
                    ("↑/↓", "move"),
                    ("enter", "open"),
                    ("ctrl+s", "review"),
                    ("q", "quit"),
                ]
            }
            Screen::Browse => {
                self.render_browser(frame, rows[1]);
                vec![
                    ("↑/↓", "move"),
                    ("enter", "inspect"),
                    ("e/f/d", "edit/fork/delete"),
                    ("esc", "home"),
                ]
            }
            Screen::TaxonomyDetail => {
                self.render_taxonomy_detail(frame, rows[1]);
                vec![("↑/↓", "move"), ("enter", "properties"), ("esc", "back")]
            }
            Screen::CommitTypeDetail(index) => {
                self.render_commit_type_detail(frame, rows[1], index);
                vec![("↑/↓", "scroll"), ("esc", "back")]
            }
            Screen::Settings(destination) => {
                self.render_settings(frame, rows[1], destination);
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
                self.render_taxonomy_editor(frame, rows[1], mode);
                vec![
                    ("enter", "open"),
                    ("n/d", "add/remove"),
                    ("alt+↑/↓", "reorder"),
                    ("ctrl+s", "complete"),
                    ("esc", "back"),
                ]
            }
            Screen::TypeEditor(index) => {
                self.render_type_editor(frame, rows[1], index);
                vec![
                    ("enter", "edit"),
                    ("n/d", "add/remove"),
                    ("alt+↑/↓", "reorder"),
                    ("esc", "back"),
                ]
            }
            Screen::Form(kind) => {
                self.render_form(frame, rows[1], kind);
                vec![
                    ("tab", "field"),
                    ("←/→", "choice"),
                    ("alt+enter", "newline"),
                    ("ctrl+s", "complete"),
                    ("esc", "back"),
                ]
            }
            Screen::Fork => {
                self.render_fork(frame, rows[1]);
                vec![("ctrl+s", "fork"), ("esc", "back")]
            }
            Screen::Delete => {
                self.render_confirmation(
                    frame,
                    rows[1],
                    "Delete this custom taxonomy?",
                    "y: stage deletion    enter/esc/n: keep",
                );
                vec![("y", "delete"), ("esc/n", "keep")]
            }
            Screen::Review(destination) => {
                self.render_review(frame, rows[1], destination);
                vec![("enter", "apply"), ("esc", "back"), ("↑/↓", "scroll")]
            }
            Screen::Leave => {
                self.render_confirmation(
                    frame,
                    rows[1],
                    "Discard all unapplied configuration changes?",
                    "y: discard and quit    enter/esc/n: keep",
                );
                vec![("y", "discard"), ("esc/n", "keep")]
            }
            Screen::ScopeChange(_) => {
                self.render_confirmation(
                    frame,
                    rows[1],
                    "This scope has unapplied changes.",
                    "a: apply    d: discard    enter/esc: cancel",
                );
                vec![("a", "apply"), ("d", "discard"), ("esc", "cancel")]
            }
        };
        frame.render_widget(
            Paragraph::new(self.status.as_str()).style(Style::default().fg(Color::Yellow)),
            rows[2],
        );
        render_navigation_row(frame, rows[3], &hints);
        normalize_background(frame, area);
    }

    fn render_header(&self, frame: &mut Frame<'_>, area: Rect) {
        let dirty = if self.any_dirty() {
            " • unapplied changes"
        } else {
            ""
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "gitserious config",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(dirty, Style::default().fg(Color::Yellow)),
            ])),
            area,
        );
    }

    fn render_home(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let columns = Layout::horizontal([
            Constraint::Length(20),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(area);
        frame.render_widget(Block::bordered().border_style(frame_style()), area);
        let nav = Rect::new(
            columns[0].x + 1,
            columns[0].y + 1,
            columns[0].width.saturating_sub(2),
            columns[0].height.saturating_sub(2),
        );
        self.home_area = nav;
        let rows = HomeItem::ALL
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let section = match item {
                    HomeItem::Project | HomeItem::Global => "Config",
                    HomeItem::Browse | HomeItem::Create => "Taxonomies",
                };
                ListItem::new(format!(
                    "{}{:11} {}",
                    if index == self.home_selected {
                        "› "
                    } else {
                        "  "
                    },
                    section,
                    item.label()
                ))
            })
            .collect::<Vec<_>>();
        let mut state = ListState::default();
        state.select(Some(self.home_selected));
        frame.render_stateful_widget(
            List::new(rows).highlight_style(navigation_key_style()),
            nav,
            &mut state,
        );
        let detail = Rect::new(
            columns[2].x + 1,
            columns[2].y + 1,
            columns[2].width.saturating_sub(2),
            columns[2].height.saturating_sub(2),
        );
        let text = self.home_detail(HomeItem::ALL[self.home_selected]);
        frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), detail);
    }

    fn home_detail(&self, item: HomeItem) -> String {
        match item {
            HomeItem::Global => self.global.as_ref().map_or_else(|| self.global_error.clone(), |session| format!("Global configuration\n\nDefault taxonomy\n  {}\n\nAvailable\n  {}\n\nSource\n  user configuration", session.global().default_taxonomy(), session.global().available_taxonomies().iter().map(ToString::to_string).collect::<Vec<_>>().join(", "))),
            HomeItem::Project => self.project.as_ref().map_or_else(|| self.project_error.clone(), |session| match session.project_config() {
                None => "Project configuration\n\nNot initialized\n\nEnter to review initialization from the current global environment.".into(),
                Some(config) => {
                    let default = config.default_taxonomy().map_or_else(|| format!("{}  [inherited]", session.global().default_taxonomy()), |id| format!("{id}  [project override]"));
                    let available = config.available_taxonomies().map_or_else(|| format!("{}  [inherited]", session.global().available_taxonomies().iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")), |ids| format!("{}  [project override]", ids.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")));
                    let lock = if session.source_error().is_some() { "pinned source unavailable" } else if session.update_available() { "update available" } else if session.lock().is_some_and(|lock| lock.matches(config)) { "current" } else { "stale — review Apply/Refresh" };
                    format!("Project configuration\n\nDefault taxonomy\n  {default}\n\nAvailable\n  {available}\n\nLock\n  {lock}\n\nSource\n  {}", session.root().map_or_else(String::new, |root| root.as_path().join("gitserious.toml").display().to_string()))
                }
            }),
            HomeItem::Browse => format!("Taxonomy library\n\n{} built-in and custom taxonomies.\n\nBrowse state here; configuration screens decide which taxonomies are available.", self.catalog().map_or(0, |items| items.len())),
            HomeItem::Create => "Create taxonomy\n\nAuthor one taxonomy containing ordered commit types and their durable properties. Completion adds it to the library but does not make it available automatically.".into(),
        }
    }

    fn render_browser(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let columns = Layout::horizontal([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(area);
        let items = self.catalog().unwrap_or_default();
        self.browse_selected = self.browse_selected.min(items.len().saturating_sub(1));
        let list_area = Rect::new(
            columns[0].x + 1,
            columns[0].y + 1,
            columns[0].width.saturating_sub(2),
            columns[0].height.saturating_sub(2),
        );
        self.browser_area = list_area;
        let rows = items
            .iter()
            .enumerate()
            .map(|(index, taxonomy)| {
                ListItem::new(format!(
                    "{}{}",
                    if index == self.browse_selected {
                        "› "
                    } else {
                        "  "
                    },
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
        if !items.is_empty() {
            state.select(Some(self.browse_selected));
        }
        frame.render_stateful_widget(
            List::new(rows)
                .block(
                    Block::bordered()
                        .border_style(frame_style())
                        .title("Taxonomies"),
                )
                .highlight_style(navigation_key_style()),
            columns[0],
            &mut state,
        );
        let detail = items.get(self.browse_selected).map_or_else(|| "No taxonomies.".into(), |taxonomy| {
            let origin = self.global.as_ref().and_then(|session| session.catalog().ok()).and_then(|catalog| catalog.origin(taxonomy.id())).map_or("unknown", TaxonomyOrigin::as_str);
            let lineage = taxonomy.derived_from().map_or_else(|| "—".into(), |source| format!("{}@{}", source.id(), source.version()));
            format!("{}\n\n{}\n\nOrigin        {origin}\nVersion       {}\nForked from   {lineage}\nCommit types  {}\n\n[ Inspect ]  [ Edit ]  [ Fork ]  [ Delete ]", taxonomy.id(), taxonomy.description(), taxonomy.version(), taxonomy.commit_types().len())
        });
        frame.render_widget(
            Paragraph::new(detail).wrap(Wrap { trim: false }).block(
                Block::bordered()
                    .border_style(frame_style())
                    .title("Selected taxonomy"),
            ),
            columns[1],
        );
    }

    fn render_taxonomy_detail(&self, frame: &mut Frame<'_>, area: Rect) {
        let Some(taxonomy) = self.selected_taxonomy() else {
            return;
        };
        let rows = taxonomy
            .commit_types()
            .iter()
            .enumerate()
            .map(|(index, definition)| {
                ListItem::new(format!(
                    "{}{}  {}",
                    if index == self.child_selected {
                        "› "
                    } else {
                        "  "
                    },
                    definition.id(),
                    definition.description()
                ))
                .style(Style::default().bg(if index % 2 == 0 {
                    JET_BLACK
                } else {
                    ZEBRA_BACKGROUND
                }))
            })
            .collect::<Vec<_>>();
        let mut state = ListState::default();
        state.select(Some(self.child_selected));
        frame.render_stateful_widget(
            List::new(rows)
                .block(
                    Block::bordered()
                        .border_style(frame_style())
                        .title(format!("{} — commit types", taxonomy.id())),
                )
                .highlight_style(navigation_key_style()),
            area,
            &mut state,
        );
    }

    fn render_commit_type_detail(&self, frame: &mut Frame<'_>, area: Rect, index: usize) {
        let Some(taxonomy) = self.selected_taxonomy() else {
            return;
        };
        let Some(definition) = taxonomy.commit_types().get(index) else {
            return;
        };
        let mut text = format!(
            "{}\n\n{}\n\nProperties\n──────────\n",
            definition.id(),
            definition.description()
        );
        if definition.properties().is_empty() {
            text.push_str("\nNo durable properties.\n");
        }
        for property in definition.properties() {
            text.push_str(&format!(
                "\n{}  [{:?}, {:?}]\n  {}\n",
                property.key(),
                property.requirement(),
                property.multiplicity(),
                property.description()
            ));
        }
        frame.render_widget(
            Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0))
                .block(
                    Block::bordered()
                        .border_style(frame_style())
                        .title("Commit type"),
                ),
            area,
        );
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
    }

    fn render_form(&self, frame: &mut Frame<'_>, area: Rect, kind: FormKind) {
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
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).block(
                Block::bordered()
                    .border_style(frame_style())
                    .title("Focused editor"),
            ),
            area,
        );
    }

    fn render_fork(&self, frame: &mut Frame<'_>, area: Rect) {
        let source = self
            .selected_taxonomy()
            .map_or_else(String::new, |taxonomy| taxonomy.id().to_string());
        frame.render_widget(Paragraph::new(format!("Fork taxonomy\n\nSource\n  {source}\n\nNew taxonomy id\n  {}\n\nThe fork is a snapshot; later source changes never propagate.", self.fork_target)).block(Block::bordered().border_style(frame_style())), area);
    }

    fn render_review(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        destination: ConfigurationDestination,
    ) {
        let text = self
            .session(destination)
            .and_then(|session| session.review().ok())
            .unwrap_or_else(|| "Configuration cannot be reviewed.".into());
        frame.render_widget(
            Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0))
                .block(
                    Block::bordered()
                        .border_style(frame_style())
                        .title("Review changes — Enter to apply"),
                ),
            area,
        );
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

    use gitserious_app::{GlobalConfiguration, ProjectState, RepositoryRoot};
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

    fn text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    #[test]
    fn home_and_browser_use_progressive_taxonomy_navigation()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        let home = text(&terminal);
        assert!(home.contains("gitserious config"));
        assert!(home.contains("Project"));
        assert!(home.contains("Taxonomies"));

        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(state.screen, Screen::Browse));
        terminal.draw(|frame| state.render(frame))?;
        let browser = text(&terminal);
        assert!(browser.contains("conventional"));
        assert!(browser.contains("research"));
        assert!(!browser.contains("ml-research"));
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
        let before = state.home_selected;
        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(state.home_selected, before);
        Ok(())
    }
}
