use std::io::{self, IsTerminal};
use std::path::PathBuf;

use crate::confirmation::{
    self, APPLY_OR_DISCARD, Answer, ConfirmDialog, Confirmation, DISCARD, DISCARD_AND_QUIT, REMOVE,
    STAGE_DELETION,
};
use gitserious_app::{
    ConfigurationDestination, ConfigurationEditor, ConfigurationSession, ConfigurationWorkspace,
    LibraryDraft, ProjectConfig, RepositoryRoot, TaxonomyOrigin, resolve_effective_configuration,
};
use gitserious_core::{
    CommitTypeDefinition, CommitTypeDefinitionError, CommitTypeId, ConditionId, Description,
    PropertyCondition, PropertyDefinition, PropertyKey, PropertyRequirement, SchemaVersion,
    Taxonomy, TaxonomyError, TaxonomyId, TaxonomyLineage, TaxonomyVersion,
};
use ratatui::Frame;
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState, Wrap,
};
use tui_textarea::{CursorMove, CursorRenderMode, TextArea, WrapMode};
use unicode_segmentation::UnicodeSegmentation;

use crate::theme::{
    JET_BLACK, MINIMUM_HEIGHT, MINIMUM_WIDTH, centered_rect, focus_frame_style, frame_style,
    navigation_key_style, normalize_background, render_navigation_row, row_style,
    section_heading_style, selected_style,
};

mod child_editor;
mod home_overlay;
#[cfg(test)]
mod model_check;
mod naming;
mod taxonomies;
use child_editor::{ChildAction, PropertyEditorState, TypeEditorState};
use home_overlay::{CommandPaletteState, CreateCommand, HomeCommand, HomeOverlay};
use naming::{
    NameField, description_required, edited_prose, name_error, name_heading_line,
    noted_heading_line, sentence, single_line,
};
use taxonomies::TaxonomiesView;

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
        if matches!(self.state.screen, Screen::Home) && self.state.home_overlay.is_none() {
            ShellKeys::Navigation
        } else {
            ShellKeys::None
        }
    }

    pub(crate) fn modal(&self) -> bool {
        self.state.home_overlay.is_some() || self.confirming_discard()
    }

    pub(crate) fn editing_taxonomy(&self) -> bool {
        self.state.creating_taxonomy()
    }

    pub(crate) fn confirming_discard(&self) -> bool {
        matches!(
            self.state.screen,
            Screen::ConfirmDiscardCreate(_) | Screen::ConfirmDiscardChild(_)
        )
    }

    pub(crate) fn dirty(&self) -> bool {
        self.state.any_dirty() || self.state.editor.is_some() || self.state.settings.is_some()
    }

    /// Leaves a taxonomy editor the way its Library breadcrumb does. Returns false while
    /// the discard confirmation is open.
    pub(crate) fn request_leave_editor(&mut self) -> bool {
        self.state.request_leave_editor();
        !self.state.creating_taxonomy()
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
            Self::Taxonomies => "Taxonomies",
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

impl Notice {
    fn error(message: String) -> Self {
        Self::Error(sentence(&message))
    }

    fn info(message: String) -> Self {
        Self::Info(sentence(&message))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditorMode {
    Create,
    Edit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CreateFocus {
    Name,
    Description,
    Types,
}

#[derive(Clone, Debug)]
enum Screen {
    Home,
    Settings(ConfigurationDestination),
    TaxonomyEditor,
    TypeEditor,
    PropertyEditor,
    Delete,
    Review(ConfigurationDestination),
    Leave,
    ScopeChange(PendingAction),
    ConfirmDiscardCreate(Box<Screen>),
    ConfirmDiscardChild(Box<Screen>),
    ConfirmRemoveType(usize),
    ConfirmRemoveProperty(usize),
}

const MENU_WIDTH: u16 = 16;
const BREADCRUMB_HEIGHT: u16 = 3;
const CREATE_TEXT_LIMIT: usize = 80;
const CREATE_TEXT_LIMIT_NOTICE: &str =
    "Taxonomy name and description are limited to 80 characters.";
const CREATE_TYPES_MARKER_WIDTH: u16 = 1;
const CREATE_TYPES_COLUMN_SPACING: u16 = 2;
const CREATE_TYPES_ADD_LABEL: &str = "+ add type";

struct CreateTextEditors {
    name: TextArea<'static>,
    description: TextArea<'static>,
    limit: usize,
}

impl CreateTextEditors {
    fn from_draft(draft: &TaxonomyDraft) -> Self {
        Self::from_values(&draft.id, &draft.description, CREATE_TEXT_LIMIT)
    }

    fn from_values(name: &str, description: &str, limit: usize) -> Self {
        let mut name = TextArea::new(vec![name.to_owned()]);
        let mut description = TextArea::new(description.split('\n').map(str::to_owned).collect());
        for area in [&mut name, &mut description] {
            area.set_style(Style::default().fg(Color::White).bg(JET_BLACK));
            area.set_cursor_line_style(Style::default());
            area.set_cursor_render_mode(CursorRenderMode::Hidden);
            area.move_cursor(CursorMove::Bottom);
            area.move_cursor(CursorMove::End);
        }
        description.set_wrap_mode(WrapMode::WordOrGlyph);
        Self {
            name,
            description,
            limit,
        }
    }

    fn sync_draft(&self, draft: &mut TaxonomyDraft) {
        draft.id = self.name.lines().join("\n");
        draft.description = self.description.lines().join("\n");
    }

    fn used(&self) -> usize {
        Self::length(&self.name) + Self::length(&self.description)
    }

    fn length(area: &TextArea<'_>) -> usize {
        area.lines().join("\n").graphemes(true).count()
    }

    /// Applies `edit` unless it would leave the entry over its budget and longer than before, so
    /// an entry loaded over budget can still shrink.
    fn edit(&mut self, focus: CreateFocus, edit: impl FnOnce(&mut TextArea<'static>)) -> bool {
        let (target, other) = match focus {
            CreateFocus::Name => (&mut self.name, &self.description),
            CreateFocus::Description => (&mut self.description, &self.name),
            CreateFocus::Types => return false,
        };
        let mut candidate = target.clone();
        edit(&mut candidate);
        let before = Self::length(target) + Self::length(other);
        let after = Self::length(&candidate) + Self::length(other);
        if after > self.limit && after > before {
            return false;
        }
        *target = candidate;
        true
    }

    fn paste(&mut self, focus: CreateFocus, text: &str) -> bool {
        for grapheme in text.graphemes(true) {
            if !self.edit(focus, |area| {
                area.insert_str(grapheme);
            }) {
                return true;
            }
        }
        false
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PropertyDraft {
    key: String,
    description: String,
    requirement: PropertyRequirement,
}

impl From<&PropertyDefinition> for PropertyDraft {
    fn from(value: &PropertyDefinition) -> Self {
        Self {
            key: value.key().to_string(),
            description: value.description().to_owned(),
            requirement: value.requirement().clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
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

    fn build(&self) -> Result<Taxonomy, String> {
        let id = TaxonomyId::from_label(&self.id)
            .map_err(|error| name_error(NameField::Taxonomy, &self.id, &error))?;
        let description =
            Description::new(&self.description).map_err(|_| description_required("Taxonomy"))?;
        let version = TaxonomyVersion::new(self.version).map_err(|error| error.to_string())?;
        let types = self
            .types
            .iter()
            .map(TypeDraft::build)
            .collect::<Result<Vec<_>, _>>()?;
        Taxonomy::new(id, version, description, self.derived_from.clone(), types).map_err(|error| {
            match error {
                TaxonomyError::EmptyCommitTypes => "Add a type before staging the taxonomy.".into(),
                TaxonomyError::DuplicateCommitType(_) => sentence(&error.to_string()),
            }
        })
    }
}

impl TypeDraft {
    fn build(&self) -> Result<CommitTypeDefinition, String> {
        let id = CommitTypeId::from_label(&self.id)
            .map_err(|error| name_error(NameField::Type, &self.id, &error))?;
        let version = SchemaVersion::new(self.schema_version).map_err(|error| error.to_string())?;
        let properties = self
            .properties
            .iter()
            .map(PropertyDraft::build)
            .collect::<Result<Vec<_>, _>>()?;
        CommitTypeDefinition::new(version, id, self.description.clone(), properties).map_err(
            |error| match error {
                CommitTypeDefinitionError::EmptyDescription => description_required("Type"),
                CommitTypeDefinitionError::DuplicateProperty(_) => sentence(&error.to_string()),
            },
        )
    }
}

impl PropertyDraft {
    fn empty() -> Self {
        Self {
            key: String::new(),
            description: String::new(),
            requirement: PropertyRequirement::Required,
        }
    }

    fn build(&self) -> Result<PropertyDefinition, String> {
        let key = PropertyKey::from_label(&self.key)
            .map_err(|error| name_error(NameField::Property, &self.key, &error))?;
        PropertyDefinition::new(key, self.description.clone(), self.requirement.clone())
            .map_err(|_| description_required("Property"))
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
    breadcrumb_area: Rect,
    home_overlay: Option<HomeOverlay>,
    child_selected: usize,
    create_focus: CreateFocus,
    create_areas: [Rect; 2],
    create_text: Option<CreateTextEditors>,
    fork_source: Option<TaxonomyLineage>,
    create_type_offset: usize,
    scroll: u16,
    project_scroll: u16,
    status: Option<Notice>,
    editor: Option<TaxonomyDraft>,
    initial_editor: Option<TaxonomyDraft>,
    dialog: ConfirmDialog,
    editor_mode: EditorMode,
    type_editor: Option<TypeEditorState>,
    property_editor: Option<PropertyEditorState>,
    settings: Option<SettingsDraft>,
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
            breadcrumb_area: Rect::default(),
            home_overlay: None,
            child_selected: 0,
            create_focus: CreateFocus::Name,
            create_areas: [Rect::default(); 2],
            create_text: None,
            fork_source: None,
            create_type_offset: 0,
            scroll: 0,
            project_scroll: 0,
            status: None,
            editor: None,
            initial_editor: None,
            dialog: ConfirmDialog::default(),
            editor_mode: EditorMode::Create,
            type_editor: None,
            property_editor: None,
            settings: None,
            too_small: false,
            project_area: Rect::default(),
            tab_areas: [Rect::default(); 2],
        }
    }

    fn creating_taxonomy(&self) -> bool {
        self.editor.is_some()
            && matches!(
                self.screen,
                Screen::TaxonomyEditor
                    | Screen::TypeEditor
                    | Screen::PropertyEditor
                    | Screen::ConfirmDiscardCreate(_)
                    | Screen::ConfirmDiscardChild(_)
                    | Screen::ConfirmRemoveType(_)
                    | Screen::ConfirmRemoveProperty(_)
            )
    }

    fn editor_breadcrumb(&self) -> String {
        let mut path = if self.editor_mode == EditorMode::Edit {
            " / edit taxonomy"
        } else if self.fork_source.is_some() {
            " / fork taxonomy"
        } else {
            " / create taxonomy"
        }
        .to_owned();
        if let Some(editor) = &self.type_editor {
            path.push_str(if editor.target.is_none() {
                " / add type"
            } else {
                " / edit type"
            });
        }
        if let Some(editor) = &self.property_editor {
            path.push_str(if editor.target.is_none() {
                " / add property"
            } else {
                " / edit property"
            });
        }
        path
    }

    fn leave_create_taxonomy(&mut self) {
        self.editor = None;
        self.initial_editor = None;
        self.dialog.invalidate();
        self.create_text = None;
        self.fork_source = None;
        self.type_editor = None;
        self.property_editor = None;
        self.screen = Screen::Home;
        self.category = Category::Taxonomies;
        self.create_areas = [Rect::default(); 2];
        self.breadcrumb_area = Rect::default();
    }

    fn taxonomy_is_dirty(&self) -> bool {
        let (Some(editor), Some(initial)) = (&self.editor, &self.initial_editor) else {
            return false;
        };
        let mut current = editor.clone();
        if let Some(text) = &self.create_text {
            text.sync_draft(&mut current);
        }
        current != *initial
    }

    fn open_discard(&mut self, screen: Screen) {
        self.dialog.invalidate();
        self.screen = screen;
    }

    fn request_taxonomy_back(&mut self) {
        if self.taxonomy_is_dirty() {
            self.open_discard(Screen::ConfirmDiscardCreate(Box::new(self.screen.clone())));
        } else {
            self.leave_create_taxonomy();
        }
    }

    fn request_leave_editor(&mut self) {
        if matches!(self.screen, Screen::TaxonomyEditor) {
            self.request_taxonomy_back();
        } else {
            self.open_discard(Screen::ConfirmDiscardCreate(Box::new(self.screen.clone())));
        }
    }

    /// The question the current screen asks, if it is a confirmation. Its choices drive the
    /// dialog's keys and buttons and the footer's hints.
    fn confirmation(&self) -> Option<Confirmation<'static>> {
        let (title, message, choices) = match &self.screen {
            Screen::ConfirmDiscardCreate(_) => {
                let message = match (&self.editor_mode, &self.fork_source, &self.initial_editor) {
                    (EditorMode::Edit, _, Some(initial)) => {
                        format!(
                            "Discard changes to \"{}\" and return to Library?",
                            initial.id
                        )
                    }
                    (_, Some(source), _) => format!(
                        "Discard this fork of \"{}\" and return to Library?",
                        source.id()
                    ),
                    _ => "Discard new taxonomy and return to Library?".to_owned(),
                };
                ("Discard Taxonomy", message, DISCARD)
            }
            Screen::ConfirmDiscardChild(return_to)
                if matches!(**return_to, Screen::PropertyEditor) =>
            {
                let existing = self
                    .property_editor
                    .as_ref()
                    .and_then(|editor| editor.target)
                    .zip(self.type_editor.as_ref())
                    .and_then(|(index, kind)| kind.draft.properties.get(index));
                let message = existing.map_or_else(
                    || "Discard the new property?".to_owned(),
                    |property| format!("Discard changes to property \"{}\"?", property.key),
                );
                ("Discard Property", message, DISCARD)
            }
            Screen::ConfirmDiscardChild(_) => {
                let message = match self.type_editor.as_ref() {
                    Some(editor) if editor.target.is_some() => {
                        format!("Discard changes to type \"{}\"?", editor.draft.id)
                    }
                    _ => "Discard the new type?".to_owned(),
                };
                ("Discard Type", message, DISCARD)
            }
            Screen::ConfirmRemoveType(index) => {
                let id = self
                    .editor
                    .as_ref()
                    .and_then(|draft| draft.types.get(*index))
                    .map(|kind| kind.id.clone())
                    .unwrap_or_default();
                (
                    "Remove Type",
                    format!("Remove type \"{id}\" from this taxonomy?"),
                    REMOVE,
                )
            }
            Screen::ConfirmRemoveProperty(index) => {
                let (kind, key) = self
                    .type_editor
                    .as_ref()
                    .map(|editor| {
                        let key = editor
                            .draft
                            .properties
                            .get(*index)
                            .map(|property| property.key.clone())
                            .unwrap_or_default();
                        (editor.draft.id.clone(), key)
                    })
                    .unwrap_or_default();
                (
                    "Remove Property",
                    format!("Remove property \"{key}\" from type \"{kind}\"?"),
                    REMOVE,
                )
            }
            Screen::Delete => {
                let id = self
                    .selected_taxonomy()
                    .map(|taxonomy| taxonomy.id().to_string())
                    .unwrap_or_default();
                (
                    "Delete Taxonomy",
                    format!("Stage the deletion of \"{id}\"? Ctrl+S reviews it before applying."),
                    STAGE_DELETION,
                )
            }
            Screen::Leave => (
                "Quit",
                "Discard unapplied changes and quit?".to_owned(),
                DISCARD_AND_QUIT,
            ),
            Screen::ScopeChange(_) => {
                let scope = match self.active_scope {
                    ConfigurationDestination::Global => "The Library",
                    ConfigurationDestination::Project => "This project",
                };
                (
                    "Unapplied Changes",
                    format!("{scope} has unapplied changes. Apply or discard them first?"),
                    APPLY_OR_DISCARD,
                )
            }
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
            (Screen::ConfirmDiscardCreate(_), Answer::Confirm) => self.leave_create_taxonomy(),
            (Screen::ConfirmDiscardChild(return_to), Answer::Confirm) => {
                self.close_child(matches!(*return_to, Screen::PropertyEditor));
            }
            (
                Screen::ConfirmDiscardCreate(return_to) | Screen::ConfirmDiscardChild(return_to),
                Answer::Keep | Answer::Alternate,
            ) => self.screen = *return_to,
            (Screen::ConfirmRemoveType(index), Answer::Confirm) => {
                if let Some(draft) = &mut self.editor
                    && index < draft.types.len()
                {
                    draft.types.remove(index);
                    self.child_selected = index;
                }
                self.screen = Screen::TaxonomyEditor;
            }
            (Screen::ConfirmRemoveType(_), _) => self.screen = Screen::TaxonomyEditor,
            (Screen::ConfirmRemoveProperty(index), Answer::Confirm) => {
                if let Some(editor) = &mut self.type_editor {
                    editor.remove_property(index);
                }
                self.screen = Screen::TypeEditor;
            }
            (Screen::ConfirmRemoveProperty(_), _) => self.screen = Screen::TypeEditor,
            (Screen::Delete, Answer::Confirm) => self.stage_deletion(),
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

    fn handle_event(&mut self, event: Event, workspace: &dyn ConfigurationWorkspace) -> bool {
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
            let child_action = match self.screen {
                Screen::TypeEditor => self.type_editor.as_mut().map(|editor| editor.paste(text)),
                Screen::PropertyEditor => self
                    .property_editor
                    .as_mut()
                    .map(|editor| editor.paste(text)),
                _ => None,
            };
            if let Some(action) = child_action {
                self.child_action(action, matches!(self.screen, Screen::PropertyEditor));
                return false;
            }
            if matches!(self.screen, Screen::TaxonomyEditor)
                && let Some(editors) = &mut self.create_text
            {
                let text = match self.create_focus {
                    CreateFocus::Name => single_line(text),
                    CreateFocus::Description => text.into(),
                    CreateFocus::Types => return false,
                };
                if editors.paste(self.create_focus, &text) {
                    self.status = Some(Notice::info(CREATE_TEXT_LIMIT_NOTICE.into()));
                }
            }
            return false;
        }
        if let Event::Mouse(mouse) = event {
            if self.too_small {
                return false;
            }
            if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                if self.creating_taxonomy()
                    && contains(self.breadcrumb_area, mouse.column, mouse.row)
                {
                    self.request_leave_editor();
                } else if matches!(self.screen, Screen::Home)
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
                    && let Some(library) = self.library()
                    && self.taxonomies.click(mouse.column, mouse.row, &library)
                {
                    self.home_focus = HomeFocus::Content;
                } else if matches!(self.screen, Screen::TypeEditor)
                    && let Some(editor) = &mut self.type_editor
                {
                    let pane = editor.areas[1];
                    let list = Rect::new(
                        pane.x + 1,
                        pane.y + 1,
                        pane.width.saturating_sub(2),
                        pane.height.saturating_sub(2),
                    );
                    if contains(list, mouse.column, mouse.row) {
                        let index = editor.offset + usize::from(mouse.row - list.y);
                        if index <= editor.draft.properties.len() {
                            editor.focus = CreateFocus::Types;
                            editor.selected = index;
                        }
                    }
                } else if matches!(self.screen, Screen::TaxonomyEditor) {
                    let pane = self.create_areas[1];
                    let list = Rect::new(
                        pane.x.saturating_add(1),
                        pane.y.saturating_add(1),
                        pane.width.saturating_sub(2),
                        pane.height.saturating_sub(2),
                    );
                    if contains(list, mouse.column, mouse.row) {
                        self.create_focus = CreateFocus::Types;
                        let index =
                            self.create_type_offset + usize::from(mouse.row.saturating_sub(list.y));
                        let count = self
                            .editor
                            .as_ref()
                            .map_or(1, |draft| draft.types.len() + 1);
                        if index < count {
                            self.child_selected = index;
                        }
                    }
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
            Screen::TaxonomyEditor => self.taxonomy_editor_key(key),
            Screen::TypeEditor => self.type_editor_key(key),
            Screen::PropertyEditor => self.property_editor_key(key),
            Screen::Review(destination) => self.review_key(key, destination, workspace),
            Screen::Delete
            | Screen::Leave
            | Screen::ScopeChange(_)
            | Screen::ConfirmDiscardCreate(_)
            | Screen::ConfirmDiscardChild(_)
            | Screen::ConfirmRemoveType(_)
            | Screen::ConfirmRemoveProperty(_) => false,
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
            HomeOverlay::Palette(palette) => {
                self.handle_palette_event(event, workspace, palette, false)
            }
            HomeOverlay::CreatePalette(palette) => {
                self.handle_palette_event(event, workspace, palette, true)
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

    fn handle_palette_event(
        &mut self,
        event: Event,
        workspace: &dyn ConfigurationWorkspace,
        mut palette: CommandPaletteState,
        create: bool,
    ) -> bool {
        match event {
            Event::Key(key) => match key.code {
                KeyCode::Esc => return false,
                KeyCode::Up | KeyCode::Down => {
                    if create {
                        palette.move_create_selection(key.code == KeyCode::Down);
                    } else {
                        palette.move_selection(self.category, key.code == KeyCode::Down);
                    }
                }
                KeyCode::Backspace => {
                    let mut query = palette.query.clone();
                    query.pop();
                    palette.set_query(query);
                }
                KeyCode::Enter => {
                    if create {
                        if let Some(command) = palette.selected_create_command() {
                            self.status = None;
                            self.dispatch_create_command(command);
                            return false;
                        }
                    } else if let Some(command) = palette.selected_command(self.category) {
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
                    let count = if create {
                        palette.visible_create().len()
                    } else {
                        palette.visible(self.category).len()
                    };
                    if index < count {
                        palette.selected = index;
                    }
                } else {
                    return false;
                }
            }
            _ => {}
        }
        self.home_overlay = Some(if create {
            HomeOverlay::CreatePalette(palette)
        } else {
            HomeOverlay::Palette(palette)
        });
        false
    }

    fn dispatch_create_command(&mut self, command: CreateCommand) {
        match command {
            CreateCommand::AddType => self.open_new_type_form(),
            CreateCommand::ValidateAndStage => self.stage_editor(self.editor_mode),
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
        match key.code {
            KeyCode::Char('/') if key.modifiers.is_empty() => {
                self.home_overlay = Some(HomeOverlay::Palette(CommandPaletteState::default()));
            }
            KeyCode::Char('?')
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.home_overlay = Some(HomeOverlay::Help);
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
            KeyCode::Up
            | KeyCode::Down
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Enter
            | KeyCode::PageUp
            | KeyCode::PageDown
                if self.category == Category::Taxonomies
                    && self.home_focus == HomeFocus::Content =>
            {
                if let Some(library) = self.library()
                    && self.taxonomies.key(key.code, &library)
                {
                    self.home_focus = HomeFocus::Menu;
                }
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
            KeyCode::Char('n')
                if self.category == Category::Taxonomies && key.modifiers.is_empty() =>
            {
                self.request_global_action(PendingAction::Create);
            }
            KeyCode::Char('e')
                if self.category == Category::Taxonomies && key.modifiers.is_empty() =>
            {
                self.request_global_action(PendingAction::Edit);
            }
            KeyCode::Char('f')
                if self.category == Category::Taxonomies && key.modifiers.is_empty() =>
            {
                self.request_global_action(PendingAction::Fork);
            }
            KeyCode::Char('d')
                if self.category == Category::Taxonomies && key.modifiers.is_empty() =>
            {
                self.request_global_action(PendingAction::Delete);
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.category == Category::Taxonomies {
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

    fn taxonomy_editor_key(&mut self, key: KeyEvent) -> bool {
        let count = self
            .editor
            .as_ref()
            .map_or(1, |draft| draft.types.len() + 1);
        match key.code {
            KeyCode::Char('/') if key.modifiers.is_empty() => {
                self.home_overlay =
                    Some(HomeOverlay::CreatePalette(CommandPaletteState::default()));
            }
            KeyCode::Char('/')
                if key.modifiers.contains(KeyModifiers::ALT)
                    && self.create_focus == CreateFocus::Description =>
            {
                if let Some(editors) = &mut self.create_text
                    && !editors.edit(CreateFocus::Description, |area| {
                        area.insert_char('/');
                    })
                {
                    self.status = Some(Notice::info(CREATE_TEXT_LIMIT_NOTICE.into()));
                }
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.open_new_type_form();
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.stage_editor(self.editor_mode);
            }
            KeyCode::Up
                if self.create_focus == CreateFocus::Types
                    && key.modifiers.contains(KeyModifiers::ALT)
                    && self.child_selected > 1 =>
            {
                if let Some(editor) = &mut self.editor {
                    editor
                        .types
                        .swap(self.child_selected - 1, self.child_selected - 2);
                    self.child_selected -= 1;
                }
            }
            KeyCode::Down
                if self.create_focus == CreateFocus::Types
                    && key.modifiers.contains(KeyModifiers::ALT)
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
            KeyCode::Up if self.create_focus == CreateFocus::Types => {
                if self.child_selected == 0 {
                    self.create_focus = CreateFocus::Description;
                } else {
                    self.child_selected -= 1;
                }
            }
            KeyCode::Down if self.create_focus == CreateFocus::Types => {
                self.child_selected = (self.child_selected + 1).min(count.saturating_sub(1));
            }
            KeyCode::Up if self.create_focus == CreateFocus::Description => {
                if let Some(editors) = &mut self.create_text {
                    let before = editors.description.cursor();
                    editors.description.move_cursor(CursorMove::Up);
                    if editors.description.cursor() == before
                        && self.editor_mode == EditorMode::Create
                    {
                        self.create_focus = CreateFocus::Name;
                    }
                }
            }
            KeyCode::Down if self.create_focus == CreateFocus::Description => {
                if let Some(editors) = &mut self.create_text {
                    let before = editors.description.cursor();
                    editors.description.move_cursor(CursorMove::Down);
                    if editors.description.cursor() == before {
                        self.create_focus = CreateFocus::Types;
                    }
                }
            }
            KeyCode::Down if self.create_focus == CreateFocus::Name => {
                self.create_focus = CreateFocus::Description;
            }
            KeyCode::PageUp | KeyCode::PageDown
                if self.create_focus == CreateFocus::Description =>
            {
                if let Some(editors) = &mut self.create_text {
                    editors.description.input(key);
                }
            }
            KeyCode::Enter
                if self.create_focus == CreateFocus::Description
                    && key.modifiers.contains(KeyModifiers::ALT) =>
            {
                if let Some(editors) = &mut self.create_text
                    && !editors.edit(CreateFocus::Description, |area| {
                        area.insert_newline();
                    })
                {
                    self.status = Some(Notice::info(CREATE_TEXT_LIMIT_NOTICE.into()));
                }
            }
            KeyCode::Enter if key.modifiers.is_empty() => match self.create_focus {
                CreateFocus::Name => {
                    self.create_focus = CreateFocus::Description;
                }
                CreateFocus::Description => self.create_focus = CreateFocus::Types,
                CreateFocus::Types if self.child_selected == 0 => self.open_new_type_form(),
                CreateFocus::Types => {
                    let index = self.child_selected - 1;
                    self.open_type_editor(Some(index));
                }
            },
            KeyCode::Char('n')
                if self.create_focus == CreateFocus::Types && key.modifiers.is_empty() =>
            {
                self.open_new_type_form();
            }
            KeyCode::Char('d')
                if self.create_focus == CreateFocus::Types
                    && key.modifiers.is_empty()
                    && self.child_selected > 0 =>
            {
                self.screen = Screen::ConfirmRemoveType(self.child_selected - 1);
            }
            KeyCode::Backspace
            | KeyCode::Delete
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Home
            | KeyCode::End
                if self.create_focus != CreateFocus::Types =>
            {
                self.create_text_input(key)
            }
            KeyCode::Char(_)
                if (key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT)
                    && self.create_focus != CreateFocus::Types =>
            {
                self.create_text_input(key)
            }
            KeyCode::Esc => {
                self.request_taxonomy_back();
            }
            _ => {}
        }
        false
    }

    fn create_text_input(&mut self, key: KeyEvent) {
        if let Some(editors) = &mut self.create_text
            && !editors.edit(self.create_focus, |area| {
                area.input(key);
            })
        {
            self.status = Some(Notice::info(CREATE_TEXT_LIMIT_NOTICE.into()));
        }
    }

    fn type_editor_key(&mut self, key: KeyEvent) -> bool {
        let action = self
            .type_editor
            .as_mut()
            .map_or(ChildAction::Continue, |editor| editor.key(key));
        self.child_action(action, false);
        false
    }

    fn property_editor_key(&mut self, key: KeyEvent) -> bool {
        let action = self
            .property_editor
            .as_mut()
            .map_or(ChildAction::Continue, |editor| editor.key(key));
        self.child_action(action, true);
        false
    }

    fn child_action(&mut self, action: ChildAction, property: bool) {
        match action {
            ChildAction::Continue => {}
            ChildAction::Error(message) => self.status = Some(Notice::error(message)),
            ChildAction::Complete => {
                let result = if property {
                    self.complete_property()
                } else {
                    self.complete_type()
                };
                if let Err(message) = result {
                    self.status = Some(Notice::error(message));
                }
            }
            ChildAction::Back => {
                let dirty = if property {
                    self.property_editor
                        .as_ref()
                        .is_some_and(PropertyEditorState::is_dirty)
                } else {
                    self.type_editor
                        .as_ref()
                        .is_some_and(TypeEditorState::is_dirty)
                };
                if dirty {
                    self.open_discard(Screen::ConfirmDiscardChild(Box::new(self.screen.clone())));
                } else {
                    self.close_child(property);
                }
            }
            ChildAction::AddProperty => self.open_property_editor(None),
            ChildAction::EditProperty(index) => self.open_property_editor(Some(index)),
            ChildAction::RemoveProperty(index) => {
                self.screen = Screen::ConfirmRemoveProperty(index)
            }
        }
    }

    fn close_child(&mut self, property: bool) {
        self.dialog.invalidate();
        if property {
            self.property_editor = None;
            self.screen = Screen::TypeEditor;
        } else {
            self.child_selected = self
                .type_editor
                .as_ref()
                .and_then(|editor| editor.target)
                .map_or(0, |index| index + 1);
            self.type_editor = None;
            self.screen = Screen::TaxonomyEditor;
        }
    }

    fn stage_deletion(&mut self) {
        let previous = self.library().and_then(|library| {
            let selected = self.taxonomies.selected(&library)?.taxonomy().id();
            let index = library
                .taxonomies()
                .iter()
                .position(|taxonomy| taxonomy.id() == selected)?;
            Some(library.taxonomies()[index.saturating_sub(1)].id().clone())
        });
        let result = self
            .selected_taxonomy()
            .map(|taxonomy| taxonomy.id().clone())
            .ok_or("Select a taxonomy first.".to_owned())
            .and_then(|id| {
                self.global
                    .as_mut()
                    .ok_or("The Library is unavailable.".to_owned())
                    .and_then(|session| session.delete_taxonomy(&id))
            });
        match result {
            Ok(()) => {
                if let Some(previous) = previous {
                    self.taxonomies.select(previous);
                }
                self.screen = Screen::Home;
                self.status = Some(Notice::info(
                    "Deletion staged. Ctrl+S reviews before applying.".into(),
                ));
            }
            Err(error) => {
                self.screen = Screen::Home;
                self.status = Some(Notice::error(error));
            }
        }
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
                        self.status = Some(Notice::info("Configuration applied.".into()));
                    }
                    Err(error) => {
                        self.screen = Screen::Home;
                        self.status = Some(Notice::error(error));
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

    fn apply_scope_then(&mut self, action: PendingAction, workspace: &dyn ConfigurationWorkspace) {
        let destination = self.active_scope;
        let Some(session) = self.session(destination).cloned() else {
            return;
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
                self.status = Some(Notice::error(error));
            }
        }
    }

    fn discard_scope_then(
        &mut self,
        action: PendingAction,
        workspace: &dyn ConfigurationWorkspace,
    ) {
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

    fn request_global_action(&mut self, action: PendingAction) {
        if let Some(error) = self.catalog_error() {
            self.status = Some(Notice::error(error));
            return;
        }
        if !matches!(action, PendingAction::Create | PendingAction::ReviewGlobal)
            && self.selected_taxonomy().is_none()
        {
            self.status = Some(Notice::error("Select a taxonomy first.".into()));
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
            self.status = Some(Notice::error(
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
            PendingAction::Create => {
                self.open_creation(TaxonomyDraft::empty(), None);
            }
            PendingAction::Edit => {
                self.active_scope = ConfigurationDestination::Global;
                self.open_edit();
            }
            PendingAction::Fork => {
                if let Some(source) = self.selected_taxonomy() {
                    let lineage = TaxonomyLineage::new(source.id().clone(), source.version());
                    let mut draft = TaxonomyDraft::from_taxonomy(&source);
                    draft.version = 1;
                    draft.derived_from = Some(lineage.clone());
                    self.open_creation(draft, Some(lineage));
                }
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

    fn open_creation(&mut self, draft: TaxonomyDraft, source: Option<TaxonomyLineage>) {
        self.active_scope = ConfigurationDestination::Global;
        self.create_text = Some(CreateTextEditors::from_draft(&draft));
        self.initial_editor = Some(draft.clone());
        self.dialog.invalidate();
        self.editor = Some(draft);
        self.fork_source = source;
        self.type_editor = None;
        self.property_editor = None;
        self.editor_mode = EditorMode::Create;
        self.child_selected = 0;
        self.create_focus = CreateFocus::Name;
        self.create_type_offset = 0;
        self.screen = Screen::TaxonomyEditor;
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
                self.status = Some(Notice::error(error));
            } else {
                self.scroll = 0;
                self.screen = Screen::Review(destination);
            }
        } else {
            self.status = Some(Notice::info(
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
            self.status = Some(Notice::error(
                "Built-in taxonomies are read-only; fork one to customize it.".into(),
            ));
            return;
        }
        let draft = TaxonomyDraft::from_taxonomy(&taxonomy);
        self.create_text = Some(CreateTextEditors::from_draft(&draft));
        self.initial_editor = Some(draft.clone());
        self.editor = Some(draft);
        self.fork_source = None;
        self.type_editor = None;
        self.property_editor = None;
        self.dialog.invalidate();
        self.editor_mode = EditorMode::Edit;
        self.child_selected = 0;
        self.create_focus = CreateFocus::Types;
        self.create_type_offset = 0;
        self.screen = Screen::TaxonomyEditor;
    }

    fn open_new_type_form(&mut self) {
        self.open_type_editor(None);
    }

    fn open_type_editor(&mut self, target: Option<usize>) {
        let draft = match target {
            Some(index) => self
                .editor
                .as_ref()
                .and_then(|editor| editor.types.get(index))
                .cloned(),
            None => Some(TypeDraft {
                id: String::new(),
                description: String::new(),
                schema_version: 1,
                properties: Vec::new(),
            }),
        };
        if let Some(draft) = draft {
            self.create_focus = CreateFocus::Types;
            self.type_editor = Some(TypeEditorState::new(target, draft));
            self.property_editor = None;
            self.screen = Screen::TypeEditor;
        }
    }

    fn open_property_editor(&mut self, target: Option<usize>) {
        let draft = match target {
            Some(index) => self
                .type_editor
                .as_ref()
                .and_then(|editor| editor.draft.properties.get(index))
                .cloned(),
            None => Some(PropertyDraft::empty()),
        };
        if let Some(draft) = draft {
            if let Some(editor) = &mut self.type_editor {
                editor.focus = CreateFocus::Types;
            }
            self.property_editor = Some(PropertyEditorState::new(target, draft));
            self.screen = Screen::PropertyEditor;
        }
    }

    fn complete_type(&mut self) -> Result<(), String> {
        let child = self.type_editor.as_ref().ok_or("type draft is missing")?;
        let candidate = child.candidate()?;
        let target = child.target;
        let parent = self.editor.as_mut().ok_or("taxonomy draft is missing")?;
        if parent
            .types
            .iter()
            .enumerate()
            .any(|(index, kind)| Some(index) != target && kind.id == candidate.id)
        {
            return Err(format!(
                "A type named \"{}\" already exists in this taxonomy.",
                candidate.id
            ));
        }
        let index = if let Some(index) = target {
            *parent.types.get_mut(index).ok_or("type is missing")? = candidate;
            index
        } else {
            parent.types.push(candidate);
            parent.types.len() - 1
        };
        self.child_selected = index + 1;
        self.create_focus = CreateFocus::Types;
        self.type_editor = None;
        self.screen = Screen::TaxonomyEditor;
        Ok(())
    }

    fn complete_property(&mut self) -> Result<(), String> {
        let child = self
            .property_editor
            .as_ref()
            .ok_or("property draft is missing")?;
        let candidate = child.candidate()?;
        let target = child.target;
        let parent = self.type_editor.as_mut().ok_or("type draft is missing")?;
        if parent
            .draft
            .properties
            .iter()
            .enumerate()
            .any(|(index, property)| Some(index) != target && property.key == candidate.key)
        {
            return Err(format!(
                "A property named \"{}\" already exists in this type.",
                candidate.key
            ));
        }
        let index = if let Some(index) = target {
            *parent
                .draft
                .properties
                .get_mut(index)
                .ok_or("property is missing")? = candidate;
            index
        } else {
            parent.draft.properties.push(candidate);
            parent.draft.properties.len() - 1
        };
        parent.selected = index + 1;
        parent.focus = CreateFocus::Types;
        self.property_editor = None;
        self.screen = Screen::TypeEditor;
        Ok(())
    }

    fn stage_editor(&mut self, mode: EditorMode) {
        if let (Some(editors), Some(draft)) = (&self.create_text, &mut self.editor) {
            if editors.used() > CREATE_TEXT_LIMIT {
                self.status = Some(Notice::error(CREATE_TEXT_LIMIT_NOTICE.into()));
                return;
            }
            editors.sync_draft(draft);
        }
        let Some(mut candidate) = self.editor.clone() else {
            return;
        };
        if let Some(initial) = &self.initial_editor {
            candidate.description = edited_prose(
                std::mem::take(&mut candidate.description),
                &initial.description,
            );
        }
        let result = candidate.build().and_then(|taxonomy| {
            let id = taxonomy.id().clone();
            let session = self
                .global
                .as_mut()
                .ok_or("The Library is unavailable.".to_owned())?;
            match (mode, &self.fork_source) {
                (EditorMode::Create, Some(source)) => session
                    .fork_taxonomy(
                        source.id(),
                        id.clone(),
                        taxonomy.description().clone(),
                        taxonomy.commit_types().to_vec(),
                    )
                    .map(|()| true),
                (EditorMode::Create, None) => session.create_taxonomy(taxonomy).map(|()| true),
                (EditorMode::Edit, _) => session.update_taxonomy(
                    &id,
                    taxonomy.description().clone(),
                    taxonomy.commit_types().to_vec(),
                ),
            }
            .map(|staged| staged.then_some(id))
        });
        match result {
            Ok(staged) => {
                self.leave_create_taxonomy();
                self.status = Some(Notice::info(staged.map_or_else(
                    || "No changes to stage.".into(),
                    |id| format!("Staged taxonomy \"{id}\". Ctrl+S reviews before applying."),
                )));
            }
            Err(error) => self.status = Some(Notice::error(error)),
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
                    "The Library is unavailable.".into()
                } else {
                    self.global_error.clone()
                })
            },
            |session| session.catalog().err(),
        )
    }

    /// The Library as the outline reads it, rebuilt from the Global session on each call.
    fn library(&self) -> Option<LibraryDraft> {
        self.global
            .as_ref()
            .map(|session| LibraryDraft::from_configuration(session.global()))
    }

    /// The taxonomy that owns the outline's selected row.
    fn selected_taxonomy(&self) -> Option<Taxonomy> {
        let library = self.library()?;
        let name = self
            .taxonomies
            .selected(&library)?
            .taxonomy()
            .name()
            .to_owned();
        self.catalog()?
            .into_iter()
            .find(|taxonomy| taxonomy.id().as_str() == name)
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
        self.render_at(frame, frame.area());
    }

    fn render_at(&mut self, frame: &mut Frame<'_>, area: Rect) {
        self.dialog.invalidate();
        frame.render_widget(
            Block::default().style(Style::default().bg(JET_BLACK).fg(Color::White)),
            area,
        );
        let terminal = frame.area();
        // Embedded browsing fits beneath the shell header; taxonomy editors keep their full height.
        self.too_small = terminal.width < MINIMUM_WIDTH
            || terminal.height < MINIMUM_HEIGHT
            || (self.creating_taxonomy() && area.height < MINIMUM_HEIGHT);
        if self.too_small {
            self.breadcrumb_area = Rect::default();
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
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
        let [menu, content] =
            Layout::horizontal([Constraint::Length(MENU_WIDTH), Constraint::Min(1)]).areas(main);
        self.render_menu(frame, menu);
        let body = if self.creating_taxonomy() {
            let [breadcrumb, body] =
                Layout::vertical([Constraint::Length(BREADCRUMB_HEIGHT), Constraint::Min(1)])
                    .areas(content);
            self.render_breadcrumb(frame, breadcrumb);
            body
        } else {
            self.breadcrumb_area = Rect::default();
            content
        };
        let hints = if let Some(question) = self.confirmation() {
            match self.screen.clone() {
                Screen::ConfirmDiscardCreate(return_to)
                | Screen::ConfirmDiscardChild(return_to) => {
                    self.render_editor_background(frame, body, &return_to);
                }
                Screen::ConfirmRemoveType(_) => {
                    self.render_editor_background(frame, body, &Screen::TaxonomyEditor);
                }
                Screen::ConfirmRemoveProperty(_) => {
                    self.render_editor_background(frame, body, &Screen::TypeEditor);
                }
                _ if self.category == Category::Project => self.render_project(frame, body),
                _ => self.render_taxonomies(frame, body),
            }
            self.dialog.render(frame, body, &question);
            confirmation::hints(question.choices)
        } else {
            match self.screen.clone() {
                Screen::Home if self.category == Category::Taxonomies => {
                    self.render_taxonomies(frame, body);
                    let line = self.taxonomies.status_line(
                        self.library().as_ref(),
                        self.embedded,
                        footer.width,
                    );
                    frame.render_widget(Paragraph::new(line), footer);
                    Vec::new()
                }
                Screen::Home => {
                    self.render_project(frame, body);
                    let mut hints = vec![
                        ("←/→", "focus"),
                        ("↑/↓", "move"),
                        ("/", "commands"),
                        ("?", "help"),
                    ];
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
                Screen::TaxonomyEditor => {
                    self.render_taxonomy_editor(frame, body);
                    vec![
                        ("esc", "back"),
                        ("enter", "next/open"),
                        ("ctrl+s", "stage"),
                        ("/", "commands"),
                    ]
                }
                Screen::TypeEditor => {
                    self.render_type_editor(frame, body);
                    vec![
                        ("esc", "back"),
                        ("↑/↓", "move"),
                        ("enter", "next/open"),
                        ("ctrl+s", "complete"),
                    ]
                }
                Screen::PropertyEditor => {
                    self.render_property_editor(frame, body);
                    vec![
                        ("esc", "back"),
                        ("↑/↓", "move"),
                        ("←/→", "choice"),
                        ("ctrl+s", "complete"),
                    ]
                }
                Screen::Review(destination) => {
                    self.render_review(frame, body, destination);
                    vec![("enter", "apply"), ("esc", "back"), ("↑/↓", "scroll")]
                }
                Screen::Delete
                | Screen::Leave
                | Screen::ScopeChange(_)
                | Screen::ConfirmDiscardCreate(_)
                | Screen::ConfirmDiscardChild(_)
                | Screen::ConfirmRemoveType(_)
                | Screen::ConfirmRemoveProperty(_) => Vec::new(),
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
                .title("Config"),
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
                        .fg(Color::Yellow)
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
        if self.any_dirty() && area.height > 5 {
            frame.render_widget(
                Paragraph::new(" unapplied\n changes").style(section_heading_style()),
                Rect::new(
                    area.x.saturating_add(1),
                    area.bottom().saturating_sub(3),
                    area.width.saturating_sub(2),
                    2,
                ),
            );
        }
    }

    fn render_breadcrumb(&mut self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Block::bordered().border_style(frame_style()), area);
        let row = Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(2),
            1,
        );
        let label = "Library";
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!(" {label}"), Style::default().fg(Color::Yellow)),
                Span::raw(self.editor_breadcrumb()),
            ])),
            row,
        );
        render_header_underline(frame, row, i32::from(row.x), label);
        let width = u16::try_from(Line::from(label).width())
            .unwrap_or(u16::MAX)
            .saturating_add(2);
        self.breadcrumb_area = Rect::new(row.x, row.y, width.min(row.width), 2);
    }

    fn render_editor_background(&mut self, frame: &mut Frame<'_>, area: Rect, screen: &Screen) {
        match screen {
            Screen::TypeEditor => self.render_type_editor(frame, area),
            Screen::PropertyEditor => self.render_property_editor(frame, area),
            _ => self.render_taxonomy_editor(frame, area),
        }
    }

    fn render_notice(&self, frame: &mut Frame<'_>, area: Rect) {
        let load_error = if matches!(self.screen, Screen::Home) {
            match self.category {
                Category::Project => self.project.as_ref().map_or_else(
                    || Some(self.project_error.clone()),
                    ConfigurationSession::source_error,
                ),
                Category::Taxonomies => self.catalog_error(),
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
            List::new(rows).highlight_style(selected_style(focused)),
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

    fn render_taxonomies(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let focused = matches!(self.screen, Screen::Home) && self.home_focus == HomeFocus::Content;
        let library = self.library();
        self.taxonomies
            .render(frame, area, library.as_ref(), focused);
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

    fn render_taxonomy_editor(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let Some(editor) = &self.editor else {
            return;
        };
        let [taxonomy, types] =
            Layout::vertical([Constraint::Length(8), Constraint::Min(4)]).areas(area);
        self.create_areas = [taxonomy, types];
        if let Some(text) = &mut self.create_text {
            render_metadata_frame(
                frame,
                taxonomy,
                "Taxonomy",
                text,
                self.create_focus,
                self.editor_mode == EditorMode::Edit,
            );
        }

        let types_focused = self.create_focus == CreateFocus::Types;
        let types_style = focus_frame_style(types_focused);
        let content_width = types.width.saturating_sub(2);
        let id_width = create_types_id_width(&editor.types, content_width);
        let mut rows = vec![
            Row::new(vec![
                Cell::from(if self.child_selected == 0 { "›" } else { " " }),
                Cell::from(CREATE_TYPES_ADD_LABEL),
                Cell::from(""),
            ])
            .style(row_style(0)),
        ];
        rows.extend(editor.types.iter().enumerate().map(|(index, kind)| {
            Row::new(vec![
                Cell::from(if self.child_selected == index + 1 {
                    "›"
                } else {
                    " "
                }),
                Cell::from(kind.id.as_str()),
                Cell::from(kind.description.as_str()),
            ])
            .style(row_style(index + 1))
        }));
        let mut state = TableState::default();
        state.select(Some(self.child_selected.min(rows.len().saturating_sub(1))));
        frame.render_stateful_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(CREATE_TYPES_MARKER_WIDTH),
                    Constraint::Length(id_width),
                    Constraint::Min(1),
                ],
            )
            .column_spacing(CREATE_TYPES_COLUMN_SPACING)
            .block(
                Block::bordered()
                    .border_style(types_style)
                    .title_style(types_style)
                    .title("Types"),
            )
            .row_highlight_style(selected_style(types_focused)),
            types,
            &mut state,
        );
        self.create_type_offset = state.offset();
    }

    fn render_type_editor(&mut self, frame: &mut Frame<'_>, area: Rect) {
        if let Some(editor) = &mut self.type_editor {
            editor.render(frame, area);
        }
    }

    fn render_property_editor(&mut self, frame: &mut Frame<'_>, area: Rect) {
        if let Some(editor) = &mut self.property_editor {
            editor.render(frame, area);
        }
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
        let paragraph = Paragraph::new(hanging_wrap(
            &text,
            usize::from(area.width.saturating_sub(2)),
        ))
        .wrap(Wrap { trim: false });
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

const fn policy_label(requirement: &PropertyRequirement) -> &'static str {
    match requirement {
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

fn render_header_underline(frame: &mut Frame<'_>, area: Rect, label_x: i32, label: &str) {
    for offset in 0..label.chars().count() {
        let x = label_x
            .saturating_add(1)
            .saturating_add(i32::try_from(offset).unwrap_or(i32::MAX));
        if let Ok(column) = u16::try_from(x)
            && column >= area.x
            && column < area.right()
        {
            frame.buffer_mut()[(column, area.y + 1)]
                .set_symbol("─")
                .set_fg(Color::Yellow);
        }
    }
}

fn render_taxonomy_heading(frame: &mut Frame<'_>, area: Rect, title: &str) {
    frame.render_widget(
        Paragraph::new(taxonomy_heading_line(title, area.width)),
        area,
    );
}

fn taxonomy_heading_line(title: &str, width: u16) -> Line<'static> {
    let rule_width = usize::from(width).saturating_sub(title.len() + 1);
    Line::from(vec![
        Span::styled(title.to_owned(), section_heading_style().fg(Color::White)),
        Span::styled(format!(" {}", "⠒".repeat(rule_width)), frame_style()),
    ])
}

fn render_text_area(frame: &mut Frame<'_>, text: &TextArea<'_>, area: Rect, focused: bool) {
    frame.render_widget(text, area);
    if focused
        && let Some(cursor) = text.rendered_cursor_position()
        && contains(area, cursor.x, cursor.y)
    {
        frame.buffer_mut()[(cursor.x, cursor.y)]
            .set_style(Style::default().add_modifier(Modifier::REVERSED));
    }
}

fn render_counter(frame: &mut Frame<'_>, area: Rect, used: usize, limit: usize) {
    let counter = format!(" {used}/{limit} ");
    let width = u16::try_from(counter.len())
        .unwrap_or(u16::MAX)
        .min(area.width.saturating_sub(2));
    frame.render_widget(
        Paragraph::new(counter).style(Style::default().fg(Color::White).bg(JET_BLACK)),
        Rect::new(
            area.right().saturating_sub(width + 1),
            area.bottom() - 1,
            width,
            1,
        ),
    );
}

fn render_metadata_frame(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    text: &mut CreateTextEditors,
    focus: CreateFocus,
    name_locked: bool,
) {
    let style = focus_frame_style(focus != CreateFocus::Types);
    frame.render_widget(
        Block::bordered()
            .title(title)
            .border_style(style)
            .title_style(style),
        area,
    );
    let x = area.x + 1;
    let width = area.width.saturating_sub(2);
    let heading = if name_locked {
        noted_heading_line("Name", Some("fixed after creation".into()), width)
    } else {
        name_heading_line("Name", &text.name.lines().join("\n"), width)
    };
    frame.render_widget(Paragraph::new(heading), Rect::new(x, area.y + 1, width, 1));
    render_text_area(
        frame,
        &text.name,
        Rect::new(x, area.y + 2, width, 1),
        focus == CreateFocus::Name,
    );
    render_taxonomy_heading(frame, Rect::new(x, area.y + 4, width, 1), "Description");
    render_text_area(
        frame,
        &text.description,
        Rect::new(x, area.y + 5, width, 2),
        focus == CreateFocus::Description,
    );
    render_counter(frame, area, text.used(), text.limit);
}

fn create_types_id_width(types: &[TypeDraft], content_width: u16) -> u16 {
    let measured = types
        .iter()
        .map(|kind| u16::try_from(Line::from(kind.id.as_str()).width()).unwrap_or(u16::MAX))
        .fold(
            u16::try_from(Line::from(CREATE_TYPES_ADD_LABEL).width()).unwrap_or(u16::MAX),
            u16::max,
        )
        .max(1);
    let maximum = content_width
        .saturating_sub(CREATE_TYPES_MARKER_WIDTH)
        .saturating_sub(CREATE_TYPES_COLUMN_SPACING.saturating_mul(2))
        .saturating_sub(1)
        .max(1);
    measured.min(maximum)
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
    }

    /// Stages a fork without edits, as staging the fork editor unchanged does.
    trait ForkUnchanged {
        fn fork_unchanged(&mut self, source: &TaxonomyId, target: TaxonomyId)
        -> Result<(), String>;
    }

    impl ForkUnchanged for ConfigurationSession {
        fn fork_unchanged(
            &mut self,
            source: &TaxonomyId,
            target: TaxonomyId,
        ) -> Result<(), String> {
            let source_taxonomy = self
                .catalog()?
                .find(source)
                .cloned()
                .ok_or("the fork source is not installed")?;
            self.fork_taxonomy(
                source,
                target,
                source_taxonomy.description().clone(),
                source_taxonomy.commit_types().to_vec(),
            )
        }
    }

    fn child_editor_workspace() -> Result<(Workspace, Taxonomy), Box<dyn std::error::Error>> {
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
                vec![source.clone()],
            )?);
        Ok((workspace, source))
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

    fn click(area: Rect) -> Event {
        Event::Mouse(ratatui::crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.x + 1,
            row: area.y,
            modifiers: KeyModifiers::NONE,
        })
    }

    fn stage_creation(state: &mut State, workspace: &Workspace) {
        state.handle_event(key(KeyCode::Char('/')), workspace);
        state.handle_event(Event::Paste("stage".into()), workspace);
        state.handle_event(key(KeyCode::Enter), workspace);
    }

    fn rename_creation(
        state: &mut State,
        workspace: &Workspace,
        name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        state.create_focus = CreateFocus::Name;
        let length = state
            .create_text
            .as_ref()
            .ok_or("missing text")?
            .name
            .lines()[0]
            .chars()
            .count();
        for _ in 0..length {
            state.handle_event(key(KeyCode::Left), workspace);
        }
        for _ in 0..length {
            state.handle_event(key(KeyCode::Delete), workspace);
        }
        state.handle_event(Event::Paste(name.into()), workspace);
        Ok(())
    }

    fn select_taxonomy(state: &mut State, index: usize) {
        let library = state
            .library()
            .unwrap_or_else(|| unreachable!("the Library loads"));
        state
            .taxonomies
            .select(library.taxonomies()[index].id().clone());
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
        assert!(row_text(&terminal, 0, 0, 100).contains("Config"));
        assert_eq!(buffer[(0, 0)].symbol(), "┌");
        assert_eq!(buffer[(MENU_WIDTH - 1, 0)].symbol(), "┐");
        assert_eq!(buffer[(MENU_WIDTH - 1, 28)].symbol(), "┘");
        assert_eq!(buffer[(MENU_WIDTH, 0)].symbol(), "┌");
        assert_eq!(buffer[(99, 0)].symbol(), "┐");
        assert_ne!(buffer[(MENU_WIDTH + 1, 1)].symbol(), " ");
        assert_eq!(buffer[(MENU_WIDTH + 1, 1)].fg, Color::Yellow);
        let project_columns = pane_columns(Rect::new(MENU_WIDTH, 0, 100 - MENU_WIDTH, 29));
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
        assert_eq!(buffer[(2, 1)].bg, Color::Yellow);
        assert_eq!(buffer[(5, 1)].fg, Color::Yellow);
        assert!(buffer[(5, 1)].modifier.contains(Modifier::UNDERLINED));
        assert_eq!(buffer[(0, 28)].symbol(), "└");
        assert_eq!(buffer[(0, 29)].symbol(), "←");
        assert_eq!(buffer[(3, 29)].symbol(), ":");
        assert!((0..100).all(|x| buffer[(x, 29)].bg == Color::Yellow));

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
        let team = TaxonomyId::new("team")?;
        authored.fork_unchanged(&TaxonomyId::new("conventional")?, team.clone())?;
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
    fn create_taxonomy_edits_fields_and_stages_through_commands()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        assert!(
            matches!(state.screen, Screen::TaxonomyEditor)
                && state.editor_mode == EditorMode::Create
        );
        assert_eq!(state.create_focus, CreateFocus::Name);

        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        let rendered = text(&terminal);
        assert!(row_text(&terminal, 0, 0, MINIMUM_WIDTH).contains("Config"));
        assert!(rendered.contains("Name"));
        assert!(rendered.contains("Description"));
        assert!(rendered.contains("+ add type"));
        assert!(!rendered.contains("Metadata"));
        assert!(rendered.contains("/: commands"));
        assert!(
            row_text(
                &terminal,
                0,
                state.create_areas[0].bottom() - 1,
                MINIMUM_WIDTH
            )
            .contains(" 0/80 ")
        );
        assert!(
            row_text(&terminal, 0, MINIMUM_HEIGHT - 1, MINIMUM_WIDTH)
                .starts_with("esc: back  enter: next/open  ctrl+s: stage  /: commands")
        );
        terminal.draw(|frame| state.render(frame))?;
        assert!(row_text(&terminal, 0, 0, MINIMUM_WIDTH).contains("Config"));

        state.handle_event(Event::Paste("team".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(Event::Paste("Team changes".into()), &workspace);
        assert_eq!(state.create_focus, CreateFocus::Description);
        assert_eq!(
            state
                .create_text
                .as_ref()
                .ok_or("missing text")?
                .name
                .lines(),
            &["team"]
        );
        assert_eq!(
            state
                .create_text
                .as_ref()
                .ok_or("missing text")?
                .description
                .lines()
                .join("\n"),
            "Team changes"
        );
        state.handle_event(
            Event::Key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::ALT)),
            &workspace,
        );
        assert_eq!(
            state
                .create_text
                .as_ref()
                .ok_or("missing text")?
                .description
                .lines()
                .join("\n"),
            "Team changes/"
        );
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        assert!(matches!(state.status, Some(Notice::Error(_))));
        assert!(
            matches!(state.screen, Screen::TaxonomyEditor)
                && state.editor_mode == EditorMode::Create
        );
        assert!(!state.scope_dirty(ConfigurationDestination::Global));

        state.handle_event(key(KeyCode::Char('/')), &workspace);
        assert!(matches!(
            state.home_overlay,
            Some(HomeOverlay::CreatePalette(_))
        ));
        state.handle_event(Event::Paste("stage".into()), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(state.status, Some(Notice::Error(_))));
        assert!(
            matches!(state.screen, Screen::TaxonomyEditor)
                && state.editor_mode == EditorMode::Create
        );

        state.handle_event(key(KeyCode::Char('/')), &workspace);
        state.handle_event(Event::Paste("add".into()), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(state.screen, Screen::TypeEditor));
        state.handle_event(Event::Paste("feat".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(Event::Paste("New feature".into()), &workspace);
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        assert!(
            matches!(state.screen, Screen::TaxonomyEditor)
                && state.editor_mode == EditorMode::Create
        );
        assert_eq!(state.create_focus, CreateFocus::Types);
        assert_eq!(state.child_selected, 1);
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(state.screen, Screen::TypeEditor));
        terminal.draw(|frame| state.render(frame))?;
        assert!(!row_text(&terminal, 0, 0, MINIMUM_WIDTH).contains("Create Taxonomy"));
        state.handle_event(key(KeyCode::Esc), &workspace);

        state.handle_event(key(KeyCode::Char('/')), &workspace);
        state.handle_event(Event::Paste("stage".into()), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(state.scope_dirty(ConfigurationDestination::Global));
        let staged = state
            .global
            .as_ref()
            .ok_or("missing global")?
            .catalog()?
            .find(&TaxonomyId::new("team")?)
            .cloned()
            .ok_or("missing staged taxonomy")?;
        assert_eq!(staged.description().to_string(), "Team changes/");
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }

    #[test]
    fn create_taxonomy_mouse_focus_and_cancel_leave_no_draft()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        let taxonomy = state.create_areas[0];
        assert!(
            row_text(&terminal, 0, MINIMUM_HEIGHT - 1, MINIMUM_WIDTH)
                .starts_with("esc: back  enter: next/open  ctrl+s: stage  /: commands")
        );
        assert!(!row_text(&terminal, 0, MINIMUM_HEIGHT - 1, MINIMUM_WIDTH).contains('|'));
        state.handle_event(key(KeyCode::Tab), &workspace);
        assert_eq!(state.create_focus, CreateFocus::Name);
        state.handle_event(
            Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: taxonomy.x + 1,
                row: taxonomy.y + 4,
                modifiers: KeyModifiers::NONE,
            }),
            &workspace,
        );
        assert_eq!(state.create_focus, CreateFocus::Name);
        state.handle_event(Event::Paste("A description".into()), &workspace);
        assert_eq!(
            state
                .create_text
                .as_ref()
                .ok_or("missing text")?
                .name
                .lines(),
            &["A description"]
        );
        state.handle_event(key(KeyCode::Esc), &workspace);
        assert!(matches!(state.screen, Screen::ConfirmDiscardCreate(_)));
        state.handle_event(key(KeyCode::Char('y')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(state.editor.is_none());
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }

    #[test]
    fn create_taxonomy_cursor_edits_and_crosses_sections() -> Result<(), Box<dyn std::error::Error>>
    {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        let taxonomy = state.create_areas[0];
        assert_eq!(
            terminal.backend().buffer()[(taxonomy.x, taxonomy.y + 1)].fg,
            Color::Yellow
        );
        assert!(
            (taxonomy.x + 1..taxonomy.right() - 1)
                .all(|x| terminal.backend().buffer()[(x, taxonomy.y + 2)].bg == JET_BLACK)
        );
        assert!(
            terminal.backend().buffer()[(taxonomy.x + 1, taxonomy.y + 2)]
                .modifier
                .contains(Modifier::REVERSED)
        );
        assert!(
            row_text(
                &terminal,
                taxonomy.x + 1,
                taxonomy.y + 1,
                taxonomy.width - 2
            )
            .contains("Name ⠒")
        );
        assert!(
            row_text(
                &terminal,
                taxonomy.x + 1,
                taxonomy.y + 4,
                taxonomy.width - 2
            )
            .contains("Description ⠒")
        );

        state.handle_event(Event::Paste("team".into()), &workspace);
        state.handle_event(key(KeyCode::Left), &workspace);
        state.handle_event(key(KeyCode::Left), &workspace);
        state.handle_event(Event::Paste("X".into()), &workspace);
        assert_eq!(
            state
                .create_text
                .as_ref()
                .ok_or("missing text")?
                .name
                .lines(),
            &["teXam"]
        );
        state.handle_event(key(KeyCode::Backspace), &workspace);
        state.handle_event(key(KeyCode::Delete), &workspace);
        assert_eq!(
            state
                .create_text
                .as_ref()
                .ok_or("missing text")?
                .name
                .lines(),
            &["tem"]
        );
        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(state.create_focus, CreateFocus::Description);
        state.handle_event(
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT)),
            &workspace,
        );
        state.handle_event(Event::Paste("second".into()), &workspace);
        state.handle_event(key(KeyCode::Up), &workspace);
        state.handle_event(Event::Paste("first".into()), &workspace);
        assert_eq!(
            state
                .create_text
                .as_ref()
                .ok_or("missing text")?
                .description
                .lines()
                .join("\n"),
            "first\nsecond"
        );
        state.handle_event(key(KeyCode::Down), &workspace);
        let cursor = state
            .create_text
            .as_ref()
            .ok_or("missing text")?
            .description
            .cursor();
        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(state.create_focus, CreateFocus::Types);
        state.handle_event(key(KeyCode::Up), &workspace);
        assert_eq!(state.create_focus, CreateFocus::Description);
        assert_eq!(
            state
                .create_text
                .as_ref()
                .ok_or("missing text")?
                .description
                .cursor(),
            cursor
        );
        state.handle_event(key(KeyCode::BackTab), &workspace);
        assert_eq!(state.create_focus, CreateFocus::Description);
        state.handle_event(key(KeyCode::Up), &workspace);
        state.handle_event(key(KeyCode::Up), &workspace);
        assert_eq!(state.create_focus, CreateFocus::Name);
        assert_eq!(
            state
                .create_text
                .as_ref()
                .ok_or("missing text")?
                .name
                .cursor()
                .1,
            2
        );
        terminal.draw(|frame| state.render(frame))?;
        let reversed = (taxonomy.y..taxonomy.bottom())
            .flat_map(|y| (taxonomy.x..taxonomy.right()).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                terminal.backend().buffer()[(x, y)]
                    .modifier
                    .contains(Modifier::REVERSED)
            })
            .count();
        assert_eq!(reversed, 1);
        Ok(())
    }

    #[test]
    fn create_description_scrolls_with_cursor_at_minimum_and_wide_sizes()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(
            Event::Paste(
                (0..20)
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            &workspace,
        );
        for (width, height) in [(MINIMUM_WIDTH, MINIMUM_HEIGHT), (120, 30)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| state.render(frame))?;
            let taxonomy = state.create_areas[0];
            assert!(
                (taxonomy.y + 4..taxonomy.bottom() - 1)
                    .any(|y| row_text(&terminal, 0, y, width).contains("│19"))
            );
            assert!(row_text(&terminal, 0, taxonomy.y + 4, width).contains("Description ⠒"));
            assert_eq!(
                state
                    .create_text
                    .as_ref()
                    .ok_or("missing text")?
                    .description
                    .cursor()
                    .0,
                19
            );
        }
        Ok(())
    }

    #[test]
    fn create_taxonomy_counts_visible_characters_and_caps_all_text_edits()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        state.handle_event(Event::Paste("team".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(Event::Paste("e\u{301}👩‍💻".into()), &workspace);
        state.handle_event(Event::Paste("x".repeat(73)), &workspace);
        assert_eq!(state.create_text.as_ref().ok_or("missing text")?.used(), 79);
        state.handle_event(
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT)),
            &workspace,
        );
        assert_eq!(state.create_text.as_ref().ok_or("missing text")?.used(), 80);

        for (width, height, types_height) in [(MINIMUM_WIDTH, MINIMUM_HEIGHT, 6), (120, 30, 18)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| state.render(frame))?;
            let [taxonomy, types] = state.create_areas;
            assert_eq!(taxonomy.height, 8);
            assert_eq!(types.height, types_height);
            assert_eq!(types.y, taxonomy.bottom());
            let counter = " 80/80 ";
            let counter_x = taxonomy.right() - u16::try_from(counter.len())? - 1;
            assert_eq!(
                row_text(
                    &terminal,
                    counter_x,
                    taxonomy.bottom() - 1,
                    u16::try_from(counter.len())?
                ),
                counter
            );
        }

        state.handle_event(key(KeyCode::Char('z')), &workspace);
        state.handle_event(
            Event::Key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::ALT)),
            &workspace,
        );
        state.handle_event(
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT)),
            &workspace,
        );
        assert_eq!(state.create_text.as_ref().ok_or("missing text")?.used(), 80);
        state.handle_event(key(KeyCode::Backspace), &workspace);
        assert_eq!(state.create_text.as_ref().ok_or("missing text")?.used(), 79);
        state.handle_event(key(KeyCode::Left), &workspace);
        state.handle_event(Event::Paste("AB".into()), &workspace);
        let editors = state.create_text.as_ref().ok_or("missing text")?;
        assert_eq!(editors.used(), 80);
        assert!(editors.description.lines().join("\n").ends_with("Ax"));
        assert!(matches!(state.status, Some(Notice::Info(_))));

        state
            .create_text
            .as_mut()
            .ok_or("missing text")?
            .description
            .insert_str("overflow");
        state.stage_editor(EditorMode::Create);
        assert!(matches!(state.status, Some(Notice::Error(ref message)) if message.contains("80")));
        assert!(
            matches!(state.screen, Screen::TaxonomyEditor)
                && state.editor_mode == EditorMode::Create
        );
        assert!(!state.scope_dirty(ConfigurationDestination::Global));
        Ok(())
    }

    #[test]
    fn create_taxonomy_stages_from_wide_layout_and_types_remain_mouse_selectable()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        let mut terminal = Terminal::new(TestBackend::new(120, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        state.handle_event(Event::Paste("team".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(Event::Paste("Team changes".into()), &workspace);
        state.handle_event(ctrl(KeyCode::Char('n')), &workspace);
        state.handle_event(Event::Paste("feat".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(Event::Paste("Feature".into()), &workspace);
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        let types = state.create_areas[1];
        state.child_selected = 0;
        state.handle_event(
            Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: types.x + 2,
                row: types.y + 2,
                modifiers: KeyModifiers::NONE,
            }),
            &workspace,
        );
        assert_eq!(state.create_focus, CreateFocus::Types);
        assert_eq!(state.child_selected, 1);
        state.handle_event(key(KeyCode::Char('/')), &workspace);
        state.handle_event(Event::Paste("stage".into()), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(state.scope_dirty(ConfigurationDestination::Global));
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }

    #[test]
    fn creation_headings_stay_white_while_the_active_widget_frame_is_yellow()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        for (width, height) in [(MINIMUM_WIDTH, MINIMUM_HEIGHT), (120, 30)] {
            let mut state = State::new(&workspace);
            state.category = Category::Taxonomies;
            state.home_focus = HomeFocus::Content;
            state.handle_event(key(KeyCode::Char('n')), &workspace);
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            for focus in [
                CreateFocus::Name,
                CreateFocus::Description,
                CreateFocus::Types,
            ] {
                terminal.draw(|frame| state.render(frame))?;
                assert_eq!(state.create_focus, focus);
                let [taxonomy, types] = state.create_areas;
                let buffer = terminal.backend().buffer();
                for y in [taxonomy.y + 1, taxonomy.y + 4] {
                    assert_eq!(buffer[(taxonomy.x + 1, y)].fg, Color::White);
                    assert!(
                        buffer[(taxonomy.x + 1, y)]
                            .modifier
                            .contains(Modifier::BOLD)
                    );
                    assert_eq!(buffer[(taxonomy.right() - 2, y)].fg, Color::DarkGray);
                }
                let (taxonomy_color, types_color) = if focus == CreateFocus::Types {
                    (Color::DarkGray, Color::Yellow)
                } else {
                    (Color::Yellow, Color::DarkGray)
                };
                for x in [taxonomy.x, taxonomy.x + 1] {
                    assert_eq!(buffer[(x, taxonomy.y)].fg, taxonomy_color);
                }
                for x in [types.x, types.x + 1] {
                    assert_eq!(buffer[(x, types.y)].fg, types_color);
                }
                state.handle_event(key(KeyCode::Down), &workspace);
            }
        }
        Ok(())
    }

    #[test]
    fn builtin_forks_copy_every_field_require_a_new_name_and_keep_the_fork_breadcrumb()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        for index in 0..3 {
            select_taxonomy(&mut state, index);
            let source = state.selected_taxonomy().ok_or("missing source")?;
            assert!(
                source.id().as_str().graphemes(true).count()
                    + source.description().as_str().graphemes(true).count()
                    <= CREATE_TEXT_LIMIT
            );
            state.handle_event(key(KeyCode::Char('f')), &workspace);
            assert_eq!(
                state
                    .create_text
                    .as_ref()
                    .ok_or("missing text")?
                    .name
                    .cursor(),
                (0, source.id().as_str().chars().count())
            );
            assert_eq!(
                state.editor.as_ref().ok_or("missing draft")?.build()?,
                source.fork(source.id().clone())
            );
            assert_eq!(
                state
                    .create_text
                    .as_ref()
                    .ok_or("missing text")?
                    .description
                    .lines()
                    .join("\n"),
                source.description().as_str()
            );
            stage_creation(&mut state, &workspace);
            assert!(
                matches!(state.status, Some(Notice::Error(ref message)) if message.contains("new taxonomy name"))
            );
            assert!(!state.scope_dirty(ConfigurationDestination::Global));
            state.handle_event(key(KeyCode::Esc), &workspace);
            assert!(state.editor.is_none());
            assert!(state.fork_source.is_none());
        }

        select_taxonomy(&mut state, 0);
        let source = state.selected_taxonomy().ok_or("missing source")?;
        state.handle_event(key(KeyCode::Char('f')), &workspace);
        rename_creation(&mut state, &workspace, "Research")?;
        stage_creation(&mut state, &workspace);
        assert!(
            matches!(state.status, Some(Notice::Error(ref message)) if message == "A taxonomy named \"research\" already exists in the Library.")
        );
        rename_creation(&mut state, &workspace, &source.id().as_str().to_uppercase())?;
        stage_creation(&mut state, &workspace);
        assert!(
            matches!(state.status, Some(Notice::Error(ref message)) if message.contains("new taxonomy name"))
        );
        rename_creation(&mut state, &workspace, "2fa")?;
        stage_creation(&mut state, &workspace);
        assert!(
            matches!(state.status, Some(Notice::Error(ref message)) if message == "Taxonomy name must start with a letter: \"2fa\".")
        );
        rename_creation(&mut state, &workspace, "team")?;

        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(state.screen, Screen::TypeEditor));
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(state.screen, Screen::TypeEditor));
        state.handle_event(Event::Paste(" Updated.".into()), &workspace);
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(row_text(&terminal, 0, 1, MINIMUM_WIDTH).contains("/ fork taxonomy"));
        let breadcrumb = state.breadcrumb_area;
        state.handle_event(
            Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: breadcrumb.x + 1,
                row: breadcrumb.y + 1,
                modifiers: KeyModifiers::NONE,
            }),
            &workspace,
        );
        assert!(matches!(state.screen, Screen::ConfirmDiscardCreate(_)));
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        let mut expected_draft = state.editor.as_ref().ok_or("missing draft")?.clone();
        state
            .create_text
            .as_ref()
            .ok_or("missing text")?
            .sync_draft(&mut expected_draft);
        let expected = expected_draft.build()?;
        for (width, height) in [(MINIMUM_WIDTH, MINIMUM_HEIGHT), (120, 30)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| state.render(frame))?;
            assert!(row_text(&terminal, 0, 1, width).contains("/ fork taxonomy"));
            assert!(!row_text(&terminal, 0, 1, width).contains("/ create taxonomy"));
        }
        stage_creation(&mut state, &workspace);
        assert!(matches!(state.screen, Screen::Home));
        let catalog = state.global.as_ref().ok_or("missing global")?.catalog()?;
        assert_eq!(catalog.find(source.id()), Some(&source));
        assert_eq!(catalog.find(&TaxonomyId::new("team")?), Some(&expected));
        assert_eq!(expected.version(), TaxonomyVersion::V1);
        assert_eq!(
            expected.derived_from(),
            Some(&TaxonomyLineage::new(source.id().clone(), source.version()))
        );
        assert_eq!(workspace.saves.get(), 0);
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        assert!(matches!(
            state.screen,
            Screen::Review(ConfigurationDestination::Global)
        ));
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert_eq!(workspace.saves.get(), 1);
        assert_eq!(
            workspace
                .global
                .borrow()
                .catalog()?
                .find(&TaxonomyId::new("team")?),
            Some(&expected)
        );
        Ok(())
    }

    #[test]
    fn custom_forks_preserve_long_text_and_record_the_direct_source_snapshot()
    -> Result<(), Box<dyn std::error::Error>> {
        let source = Taxonomy::new(
            TaxonomyId::new("original-team")?,
            TaxonomyVersion::new(5)?,
            Description::new("Long source description.\n".repeat(10))?,
            Some(TaxonomyLineage::new(
                TaxonomyId::new("conventional")?,
                TaxonomyVersion::V1,
            )),
            vec![
                CommitTypeDefinition::new(
                    SchemaVersion::new(3)?,
                    CommitTypeId::new("custom")?,
                    "Custom purpose",
                    vec![
                        PropertyDefinition::new(
                            PropertyKey::new("evidence")?,
                            "All supporting evidence",
                            PropertyRequirement::Conditional(PropertyCondition::new(
                                ConditionId::new("changed")?,
                                "Required when behavior changes",
                            )?),
                        )?,
                        PropertyDefinition::new(
                            PropertyKey::new("notes")?,
                            "Additional notes",
                            PropertyRequirement::Recommended,
                        )?,
                    ],
                )?,
                gitserious_core::built_in_taxonomies()[0].commit_types()[0].clone(),
            ],
        )?;
        let workspace = Workspace::new();
        *workspace.global.borrow_mut() =
            ConfigurationSession::open_global(GlobalConfiguration::new(
                1,
                TaxonomyId::new("conventional")?,
                vec![TaxonomyId::new("conventional")?],
                vec![source.clone()],
            )?);
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        select_taxonomy(&mut state, 3);
        state.handle_event(key(KeyCode::Char('f')), &workspace);
        assert_eq!(
            state.editor.as_ref().ok_or("missing draft")?.build()?,
            source.fork(source.id().clone())
        );
        let original_count = state.create_text.as_ref().ok_or("missing text")?.used();
        assert!(original_count > CREATE_TEXT_LIMIT);
        assert_eq!(
            state
                .create_text
                .as_ref()
                .ok_or("missing text")?
                .description
                .lines()
                .join("\n"),
            source.description().as_str()
        );
        stage_creation(&mut state, &workspace);
        assert!(matches!(state.status, Some(Notice::Error(ref message)) if message.contains("80")));
        assert!(!state.scope_dirty(ConfigurationDestination::Global));
        state.handle_event(key(KeyCode::Down), &workspace);
        let before_edit = state.create_text.as_ref().ok_or("missing text")?.used();
        state.handle_event(key(KeyCode::Char('x')), &workspace);
        assert_eq!(
            state.create_text.as_ref().ok_or("missing text")?.used(),
            before_edit
        );
        assert!(matches!(state.status, Some(Notice::Info(ref message)) if message.contains("80")));
        state
            .create_text
            .as_mut()
            .ok_or("missing text")?
            .description
            .move_cursor(CursorMove::End);
        state.handle_event(key(KeyCode::Backspace), &workspace);
        assert_eq!(
            state.create_text.as_ref().ok_or("missing text")?.used(),
            before_edit - 1
        );
        state
            .create_text
            .as_mut()
            .ok_or("missing text")?
            .description
            .select_all();
        state.handle_event(Event::Paste("Edited snapshot".into()), &workspace);
        rename_creation(&mut state, &workspace, "forked-team")?;
        select_taxonomy(&mut state, 0);
        stage_creation(&mut state, &workspace);
        assert!(matches!(state.screen, Screen::Home));
        let catalog = state.global.as_ref().ok_or("missing global")?.catalog()?;
        let fork = catalog
            .find(&TaxonomyId::new("forked-team")?)
            .ok_or("missing fork")?;
        assert_eq!(fork.description().as_str(), "Edited snapshot");
        assert_eq!(fork.commit_types(), source.commit_types());
        assert_eq!(fork.version(), TaxonomyVersion::V1);
        assert_eq!(
            fork.derived_from(),
            Some(&TaxonomyLineage::new(source.id().clone(), source.version()))
        );
        assert_eq!(catalog.find(source.id()), Some(&source));
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }

    #[test]
    fn fork_uses_the_project_guard_and_child_discard_confirmation()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        select_taxonomy(&mut state, 0);
        state
            .project
            .as_mut()
            .ok_or("missing project")?
            .initialize_project()?;
        state.handle_event(key(KeyCode::Char('f')), &workspace);
        assert!(matches!(
            state.screen,
            Screen::ScopeChange(PendingAction::Fork)
        ));
        state.handle_event(key(KeyCode::Esc), &workspace);
        assert!(state.editor.is_none());
        state.handle_event(key(KeyCode::Char('f')), &workspace);
        state.handle_event(key(KeyCode::Char('d')), &workspace);
        assert!(state.fork_source.is_some());
        state.handle_event(ctrl(KeyCode::Char('n')), &workspace);
        let mut terminal = Terminal::new(TestBackend::new(120, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(row_text(&terminal, 0, 1, 120).contains("/ fork taxonomy"));
        let breadcrumb = state.breadcrumb_area;
        state.handle_event(
            Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: breadcrumb.x + 1,
                row: breadcrumb.y,
                modifiers: KeyModifiers::NONE,
            }),
            &workspace,
        );
        assert!(matches!(state.screen, Screen::ConfirmDiscardCreate(_)));
        state.handle_event(key(KeyCode::Char('y')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(state.editor.is_none());
        assert!(state.create_text.is_none());
        assert!(state.fork_source.is_none());
        assert!(!state.scope_dirty(ConfigurationDestination::Global));
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }

    #[test]
    fn create_breadcrumb_sits_above_editor_widgets_beside_the_inert_menu()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        let content_x = MENU_WIDTH;
        let content_width = MINIMUM_WIDTH - MENU_WIDTH;
        assert!(row_text(&terminal, 0, 0, content_x).contains("Config"));
        assert!(row_text(&terminal, 0, 1, content_x).contains("Project"));
        assert!(row_text(&terminal, 0, 2, content_x).contains("Taxonomies"));
        assert!(
            row_text(&terminal, content_x, 1, content_width).contains("Library / create taxonomy")
        );
        assert_eq!(state.tab_areas, [Rect::default(); 2]);

        let [taxonomy, types] = state.create_areas;
        for pane in [taxonomy, types] {
            assert_eq!(pane.x, content_x);
            assert_eq!(pane.width, content_width);
        }
        assert_eq!(taxonomy.y, BREADCRUMB_HEIGHT);
        assert_eq!(taxonomy.height, 8);
        assert_eq!(types.height, 6);
        assert_eq!(types.y, taxonomy.bottom());
        assert_eq!(types.bottom(), MINIMUM_HEIGHT - 1);

        let breadcrumb = state.breadcrumb_area;
        assert_eq!((breadcrumb.x, breadcrumb.y), (content_x + 1, 1));
        let label = &terminal.backend().buffer()[(breadcrumb.x + 1, 1)];
        let underline = &terminal.backend().buffer()[(breadcrumb.x + 1, 2)];
        assert_eq!(label.symbol(), "L");
        assert_eq!(label.fg, Color::Yellow);
        assert_eq!(underline.symbol(), "─");
        assert_eq!(underline.fg, Color::Yellow);

        state.handle_event(click(Rect::new(1, 1, 1, 1)), &workspace);
        assert!(
            matches!(state.screen, Screen::TaxonomyEditor)
                && state.editor_mode == EditorMode::Create
        );
        assert_eq!(state.category, Category::Taxonomies);

        let mut wide = Terminal::new(TestBackend::new(120, 30))?;
        wide.draw(|frame| state.render(frame))?;
        let [taxonomy, types] = state.create_areas;
        assert!(
            [taxonomy, types]
                .into_iter()
                .all(|pane| pane.x == content_x && pane.width == 120 - content_x)
        );
        let mut small = Terminal::new(TestBackend::new(MINIMUM_WIDTH - 1, MINIMUM_HEIGHT))?;
        small.draw(|frame| state.render(frame))?;
        assert!(text(&small).contains("Terminal too small"));
        assert_eq!(state.breadcrumb_area, Rect::default());

        terminal.draw(|frame| state.render(frame))?;
        state.handle_event(click(state.breadcrumb_area), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(state.editor.is_none());
        terminal.draw(|frame| state.render(frame))?;
        assert_eq!(state.breadcrumb_area, Rect::default());
        assert!(!text(&terminal).contains("create taxonomy /"));
        assert!(row_text(&terminal, content_x, 0, 11).starts_with("┌Taxonomies"));
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
            let (workspace, _) = child_editor_workspace()?;
            let mut state = State::new(&workspace);
            state.handle_event(key(KeyCode::Char('2')), &workspace);
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| state.render(frame))?;
            let screen = rows(&terminal);
            assert!(screen[0].starts_with("┌Config"));
            assert!(row_text(&terminal, MENU_WIDTH, 0, 11).starts_with("┌Taxonomies"));
            assert!(screen[2].contains(" 2  Taxonomies"));
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
            assert!(text.contains("┌Inspector"));
            assert!(text.contains("Taxonomy conventional"));
            assert!(text.contains("Version      2"));
            assert!(
                screen[usize::from(height) - 1]
                    .starts_with(" NAV  · taxonomy ·  ◇ read-only   f fork  n new")
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
                screen[usize::from(height) - 1]
                    .starts_with(" NAV  · taxonomy  e edit  f fork  n new  d delete")
            );
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
        press(&mut state, KeyCode::Right)?;
        let screen = press(&mut state, KeyCode::Right)?;
        assert!(screen.contains("│▌  ▸ feat      intent · decision · constraints"));
        assert!(screen.contains("│   ▸ fix       cause · decision · constraints"));
        assert!(screen.contains("Type feat"));
        assert!(screen.contains("in conventional"));
        assert!(screen.contains("Properties   3"));
        assert!(screen.contains(" NAV  · type ·  ◇ read-only"));

        press(&mut state, KeyCode::Enter)?;
        for _ in 0..3 {
            press(&mut state, KeyCode::Down)?;
        }
        let screen = press(&mut state, KeyCode::PageDown)?;
        assert!(screen.contains("│▌    └ constraints  conditional"));
        assert!(screen.contains("│     ├ intent       required"));
        assert!(screen.contains("Property constraints"));
        assert!(screen.contains("in conventional › feat"));
        assert!(screen.contains("Policy       required recommended conditional optional"));
        assert!(screen.contains("Condition    non-obvious-requirements"));
        assert!(screen.contains("Rationale    Required when"));
        assert!(screen.contains(" NAV  · property ·  ◇ read-only"));
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
        assert_eq!(buffer[(policy_x, policy_y)].bg, Color::Rgb(40, 40, 40));
        assert_eq!(buffer[(policy_x - 2, policy_y)].fg, Color::Gray);

        state.handle_event(key(KeyCode::Char('e')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(matches!(
            &state.status,
            Some(Notice::Error(message)) if message.starts_with("Built-in taxonomies are read-only")
        ));
        state.handle_event(key(KeyCode::Char('f')), &workspace);
        assert_eq!(
            state
                .fork_source
                .as_ref()
                .map(|source| source.id().to_string()),
            Some("conventional".into())
        );
        state.handle_event(key(KeyCode::Esc), &workspace);
        assert!(matches!(state.screen, Screen::Home));

        let cursor = |state: &State| {
            state
                .taxonomies
                .observe(state.library().as_ref())
                .cursor_path
        };
        assert_eq!(cursor(&state), ["conventional", "feat", "constraints"]);
        state.handle_event(key(KeyCode::Left), &workspace);
        assert_eq!(cursor(&state), ["conventional", "feat"]);
        state.handle_event(key(KeyCode::Left), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(cursor(&state), ["conventional", "fix"]);
        state.handle_event(key(KeyCode::Left), &workspace);
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
        state.handle_event(key(KeyCode::Right), &workspace);
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
        let name = first.split_whitespace().nth(1).ok_or("missing first row")?;
        state.handle_event(click(Rect::new(MENU_WIDTH + 1, 1, 1, 1)), &workspace);
        assert_eq!(
            state
                .taxonomies
                .observe(state.library().as_ref())
                .cursor_path,
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
        let footer = row_text(&terminal, 0, 35, 87);
        assert!(footer.starts_with(" NAV  · taxonomy ·  ◇ read-only   f fork  n new  ↑/↓ move"));
        assert!(footer.trim_end().ends_with("│  tab tabs  / cmds  ? help"));
        Ok(())
    }

    #[test]
    fn arrows_move_focus_from_the_menu_through_each_view() {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        assert_eq!(state.home_focus, HomeFocus::Menu);
        state.handle_event(key(KeyCode::Char('m')), &workspace);
        assert_eq!(state.category, Category::Project);
        state.handle_event(key(KeyCode::Left), &workspace);
        assert_eq!(state.home_focus, HomeFocus::Menu);
        state.handle_event(key(KeyCode::Up), &workspace);
        assert_eq!(state.category, Category::Project);

        state.handle_event(key(KeyCode::Right), &workspace);
        assert_eq!(state.home_focus, HomeFocus::Content);
        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(state.project_section, ProjectSection::Resolution);
        assert_eq!(state.category, Category::Project);
        state.handle_event(key(KeyCode::Right), &workspace);
        assert_eq!(state.home_focus, HomeFocus::Content);
        state.handle_event(key(KeyCode::Left), &workspace);
        assert_eq!(state.home_focus, HomeFocus::Menu);

        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(state.category, Category::Taxonomies);
        assert_eq!(state.project_section, ProjectSection::Resolution);
        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(state.category, Category::Taxonomies);
        state.handle_event(key(KeyCode::Right), &workspace);
        assert_eq!(state.home_focus, HomeFocus::Content);
        let cursor = |state: &State| {
            state
                .taxonomies
                .observe(state.library().as_ref())
                .cursor_path
        };
        state.handle_event(key(KeyCode::Right), &workspace);
        assert_eq!(cursor(&state), ["conventional"]);
        state.handle_event(key(KeyCode::Right), &workspace);
        assert_eq!(cursor(&state), ["conventional", "feat"]);
        state.handle_event(key(KeyCode::Left), &workspace);
        assert_eq!(cursor(&state), ["conventional"]);
        state.handle_event(key(KeyCode::Left), &workspace);
        assert_eq!(state.home_focus, HomeFocus::Content);
        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(cursor(&state), ["research"]);
        state.handle_event(key(KeyCode::Up), &workspace);
        state.handle_event(key(KeyCode::Left), &workspace);
        assert_eq!(state.home_focus, HomeFocus::Menu);
        state.handle_event(key(KeyCode::Up), &workspace);
        assert_eq!(state.category, Category::Project);

        state.handle_event(key(KeyCode::Char('2')), &workspace);
        assert_eq!(state.category, Category::Taxonomies);
        assert_eq!(state.home_focus, HomeFocus::Content);
        state.handle_event(key(KeyCode::Left), &workspace);
        state.handle_event(key(KeyCode::Char('1')), &workspace);
        assert_eq!(state.category, Category::Project);
        assert_eq!(state.home_focus, HomeFocus::Content);
    }

    #[test]
    fn create_header_persists_in_child_and_breadcrumb_confirms_discard()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        state.handle_event(ctrl(KeyCode::Char('n')), &workspace);
        assert!(matches!(state.screen, Screen::TypeEditor));
        state.handle_event(Event::Paste("feat".into()), &workspace);
        let mut terminal = Terminal::new(TestBackend::new(120, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(row_text(&terminal, 0, 0, 120).contains("Config"));
        assert!(!row_text(&terminal, 0, 0, 120).contains("Create Taxonomy"));
        assert!(row_text(&terminal, 0, 1, 120).contains("/ create taxonomy"));
        assert!(
            row_text(&terminal, 0, 1, 120).contains("/ create taxonomy / add type"),
            "{}",
            row_text(&terminal, 0, 1, 120)
        );
        let breadcrumb = state.breadcrumb_area;
        let click = Event::Mouse(ratatui::crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: breadcrumb.x,
            row: breadcrumb.y,
            modifiers: KeyModifiers::NONE,
        });
        state.handle_event(click.clone(), &workspace);
        assert!(matches!(state.screen, Screen::ConfirmDiscardCreate(_)));
        terminal.draw(|frame| state.render(frame))?;
        assert!(text(&terminal).contains("Discard new taxonomy and return to Library?"));
        assert!(
            row_text(&terminal, 0, 1, 120).contains("/ create taxonomy / add type"),
            "{}",
            row_text(&terminal, 0, 1, 120)
        );
        state.handle_event(Event::Paste("ignored".into()), &workspace);
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        assert!(matches!(state.screen, Screen::TypeEditor));
        assert_eq!(
            state
                .type_editor
                .as_ref()
                .ok_or("missing type")?
                .text
                .name
                .lines()[0],
            "feat"
        );
        state.handle_event(click, &workspace);
        state.handle_event(key(KeyCode::Esc), &workspace);
        assert!(matches!(state.screen, Screen::TypeEditor));
        let breadcrumb = state.breadcrumb_area;
        state.handle_event(
            Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: breadcrumb.x,
                row: breadcrumb.y,
                modifiers: KeyModifiers::NONE,
            }),
            &workspace,
        );
        state.handle_event(key(KeyCode::Char('y')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(state.editor.is_none());
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }

    #[test]
    fn fork_add_type_breadcrumb_and_esc_pops_to_taxonomy() -> Result<(), Box<dyn std::error::Error>>
    {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        select_taxonomy(&mut state, 0);
        state.handle_event(key(KeyCode::Char('f')), &workspace);
        assert!(state.fork_source.is_some());
        let mut terminal = Terminal::new(TestBackend::new(120, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(row_text(&terminal, 0, 1, 120).contains("/ fork taxonomy"));
        assert!(!row_text(&terminal, 0, 1, 120).contains("/ add type"));
        state.handle_event(ctrl(KeyCode::Char('n')), &workspace);
        assert!(matches!(state.screen, Screen::TypeEditor));
        terminal.draw(|frame| state.render(frame))?;
        assert!(
            row_text(&terminal, 0, 1, 120).contains("/ fork taxonomy / add type"),
            "{}",
            row_text(&terminal, 0, 1, 120)
        );
        state.handle_event(key(KeyCode::Esc), &workspace);
        assert!(
            matches!(state.screen, Screen::TaxonomyEditor)
                && state.editor_mode == EditorMode::Create
        );
        terminal.draw(|frame| state.render(frame))?;
        assert!(row_text(&terminal, 0, 1, 120).contains("/ fork taxonomy"));
        assert!(!row_text(&terminal, 0, 1, 120).contains("/ add type"));
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
        assert!(row_text(&terminal, 0, 29, 100).contains("f fork  n new  ↑/↓ move"));
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
        assert!(
            matches!(state.screen, Screen::TaxonomyEditor)
                && state.editor_mode == EditorMode::Create
        );
        assert!(state.fork_source.is_some());
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
            let Some(HomeOverlay::Palette(palette)) = &state.home_overlay else {
                return Err("missing palette".into());
            };
            assert_eq!(
                row_text(
                    &terminal,
                    palette.list_area.x,
                    palette.list_area.y + 5,
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
                let Some(HomeOverlay::Palette(palette)) = &state.home_overlay else {
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
            let Some(HomeOverlay::Palette(palette)) = &state.home_overlay else {
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
                "Switch to Taxonomies"
            );
            state.handle_event(key(KeyCode::Enter), &workspace);
            assert_eq!(state.category, Category::Taxonomies);
            state.handle_event(key(KeyCode::Char('n')), &workspace);
            state.handle_event(key(KeyCode::Char('/')), &workspace);
            terminal.draw(|frame| state.render(frame))?;
            assert!(text(&terminal).contains("add type"));
            let Some(HomeOverlay::CreatePalette(palette)) = &state.home_overlay else {
                return Err("missing create palette".into());
            };
            let list = palette.list_area;
            let stage_row = (list.y..list.bottom())
                .map(|y| row_text(&terminal, list.x, y, list.width))
                .find(|row| row.starts_with("validate and stage taxonomy"))
                .ok_or("missing stage command")?;
            assert!(stage_row.ends_with("ctrl+s"), "{stage_row:?}");
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
            let footer = row_text(&terminal, 0, height - 1, width);
            assert!(footer.starts_with("←/→: focus  ↑/↓: move  /: commands  ?: help"));
            assert!(!footer.contains('|'));
            state.handle_event(key(KeyCode::Tab), &workspace);
            state.handle_event(key(KeyCode::BackTab), &workspace);
            state.handle_event(key(KeyCode::Char('m')), &workspace);
            assert_eq!(state.category, Category::Project);
            state.handle_event(key(KeyCode::Down), &workspace);
            assert_eq!(state.category, Category::Taxonomies);
            assert!(state.home_overlay.is_none());
            terminal.draw(|frame| state.render(frame))?;
            let footer = row_text(&terminal, 0, height - 1, width);
            assert!(footer.starts_with(" NAV  · taxonomy ·  ◇ read-only   f fork  n new"));
            assert!(footer.trim_end().ends_with("│  / cmds  ? help"));
            state.handle_event(key(KeyCode::Char('/')), &workspace);
            assert!(matches!(state.home_overlay, Some(HomeOverlay::Palette(_))));
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
        let help = centered_rect(58, 13, Rect::new(0, 0, 100, 29));
        let content_x = help.x + 2;
        let content_width = help.width - 4;
        let switch_row = row_text(&terminal, content_x, help.y + 2, content_width);
        let review_row = row_text(&terminal, content_x, help.y + 7, content_width);
        assert!(switch_row.starts_with("Switch Project / Taxonomies"));
        assert!(switch_row.ends_with("1/2/click"));
        assert!(review_row.starts_with("Review changes"));
        assert!(review_row.ends_with("ctrl+s"));
        assert!(text(&terminal).contains("[esc] close"));
        assert!(row_text(&terminal, 0, 29, 100).contains("/: commands  ?: help"));
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
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        state.handle_event(key(KeyCode::Char('?')), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        let help = centered_rect(58, 13, Rect::new(0, 0, MINIMUM_WIDTH, MINIMUM_HEIGHT - 1));
        let content_x = help.x + 2;
        let content_width = help.width - 4;
        let switch_row = row_text(&terminal, content_x, help.y + 2, content_width);
        let edit_row = row_text(&terminal, content_x, help.y + 6, content_width);
        assert!(switch_row.starts_with("Switch Project / Taxonomies"));
        assert!(switch_row.ends_with("1/2/click"));
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
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.handle_event(key(KeyCode::Char('/')), &workspace);
        state.handle_event(Event::Paste("create".into()), &workspace);
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
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(row_text(&terminal, 0, 17, MINIMUM_WIDTH).contains("? help"));
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
        assert_eq!(palette.list_area.height, 8);
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
        let Some(HomeOverlay::Palette(palette)) = &state.home_overlay else {
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
    fn review_text_still_scrolls() -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state
            .global
            .as_mut()
            .ok_or("missing global")?
            .fork_unchanged(&TaxonomyId::new("conventional")?, TaxonomyId::new("team")?)?;
        for index in 0..40 {
            state
                .global
                .as_mut()
                .ok_or("missing global")?
                .fork_unchanged(
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
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.global = None;
        state.global_error = "global load failed".into();
        terminal.draw(|frame| state.render(frame))?;
        assert_eq!(terminal.backend().buffer()[(45, 27)].fg, Color::Red);
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
        assert_eq!(row_text(&terminal, 0, 26, MENU_WIDTH), "│ unapplied    │");
        assert_eq!(row_text(&terminal, 0, 27, MENU_WIDTH), "│ changes      │");
        assert_eq!(terminal.backend().buffer()[(2, 26)].fg, Color::Yellow);
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
            Screen::ScopeChange(PendingAction::Create)
        ));
        assert_eq!(workspace.saves.get(), 0);
        state.handle_event(key(KeyCode::Esc), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert_eq!(state.category, Category::Taxonomies);
        assert!(state.scope_dirty(ConfigurationDestination::Project));
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        state.handle_event(key(KeyCode::Char('a')), &workspace);
        assert_eq!(workspace.saves.get(), 1);
        assert!(
            matches!(state.screen, Screen::TaxonomyEditor)
                && state.editor_mode == EditorMode::Create
        );
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
        state.handle_event(key(KeyCode::Char('f')), &workspace);
        assert!(
            matches!(state.screen, Screen::TaxonomyEditor)
                && state.editor_mode == EditorMode::Create
        );
        assert!(state.fork_source.is_some());
        state.handle_event(key(KeyCode::Esc), &workspace);
        state.handle_event(key(KeyCode::Char('d')), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        state
            .global
            .as_mut()
            .ok_or("missing global")?
            .fork_unchanged(&TaxonomyId::new("conventional")?, TaxonomyId::new("team")?)?;
        select_taxonomy(&mut state, 3);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(text(&terminal).contains("Forked from  conventional@2"));
        let team = row_text(&terminal, MENU_WIDTH, 4, 100 - MENU_WIDTH);
        assert!(team.contains("▌▸ team"));
        assert!(team.trim_end().ends_with("custom  │"));
        state.handle_event(key(KeyCode::Char('e')), &workspace);
        assert!(
            matches!(state.screen, Screen::TaxonomyEditor) && state.editor_mode == EditorMode::Edit
        );
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
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.global = None;
        state.global_error = "global load failed".into();
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        let rendered = text(&terminal);
        assert!(rendered.contains("Taxonomies"));
        assert!(rendered.contains("Taxonomy library unavailable."));
        assert!(rendered.contains("/ cmds  ? help"));
        assert!(rendered.contains("Global load failed."));
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
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
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
            .fork_unchanged(&TaxonomyId::new("conventional")?, TaxonomyId::new("team")?)?;
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
    fn wrapped_review_lines_start_under_their_subject() -> Result<(), Box<dyn std::error::Error>> {
        let types =
            |ids: &[&str]| -> Result<Vec<CommitTypeDefinition>, Box<dyn std::error::Error>> {
                ids.iter()
                    .map(|id| {
                        Ok(CommitTypeDefinition::new(
                            SchemaVersion::new(1)?,
                            CommitTypeId::new(*id)?,
                            "A change",
                            Vec::new(),
                        )?)
                    })
                    .collect()
            };
        let team = Taxonomy::new(
            TaxonomyId::new("team")?,
            TaxonomyVersion::new(1)?,
            Description::new("Team taxonomy")?,
            None,
            types(&["release", "fix", "hotfix"])?,
        )?;
        let conventional = TaxonomyId::new("conventional")?;
        let workspace = Workspace::new();
        *workspace.global.borrow_mut() = ConfigurationSession::open_global(
            GlobalConfiguration::new(1, conventional.clone(), vec![conventional], vec![team])?,
        );
        let mut state = State::new(&workspace);
        state.active_scope = ConfigurationDestination::Global;
        state
            .global
            .as_mut()
            .ok_or("missing global")?
            .update_taxonomy(
                &TaxonomyId::new("team")?,
                Description::new("Team changes")?,
                types(&["fix", "release"])?,
            )?;
        state.open_review();
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;

        let frame = text(&terminal).chars().collect::<Vec<_>>();
        let rows = frame
            .chunks(usize::from(MINIMUM_WIDTH))
            .map(|row| row.iter().collect::<String>())
            .collect::<Vec<_>>();
        let edited = rows
            .iter()
            .position(|row| row.contains("│  edited   team v1 -> v2: description; type "))
            .ok_or("the edited line is not on screen")?;
        assert!(
            rows[edited + 1].contains("│           hotfix removed; types reordered "),
            "the wrapped row does not start under the taxonomy id: {:?}",
            rows[edited + 1]
        );
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
        assert_eq!(buffer[(0, 16)].symbol(), "└");
        Ok(())
    }

    #[test]
    fn create_types_list_uses_columns_without_property_counts()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        state.handle_event(Event::Paste("team".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(Event::Paste("Team changes".into()), &workspace);
        state.handle_event(ctrl(KeyCode::Char('n')), &workspace);
        state.handle_event(Event::Paste("feat".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(Event::Paste("Feature".into()), &workspace);
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        state.handle_event(ctrl(KeyCode::Char('n')), &workspace);
        state.handle_event(Event::Paste("fix".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(
            Event::Paste("Bug fix with a longer description".into()),
            &workspace,
        );
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        let mut terminal = Terminal::new(TestBackend::new(120, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        let types = state.create_areas[1];
        let mut found_add = false;
        let mut id_columns = Vec::new();
        for y in types.y + 1..types.bottom() - 1 {
            let row = row_text(&terminal, types.x + 1, y, types.width - 2);
            if row.contains(CREATE_TYPES_ADD_LABEL) {
                found_add = true;
            }
            if row.contains("feat") || row.contains("Bug fix") {
                // Display-column 3 is the id column start (marker is 1 cell wide
                // + 2 spacing cells); byte indices can't be compared because "›"
                // is multi-byte.
                let from_id: String = row.chars().skip(3).collect();
                id_columns.push(from_id);
            }
            assert!(!row.contains("properties)"), "unexpected count: {row:?}");
        }
        assert!(found_add);
        assert_eq!(id_columns.len(), 2);
        assert!(id_columns[0].starts_with("feat"), "{}", id_columns[0]);
        assert!(id_columns[1].starts_with("fix"), "{}", id_columns[1]);
        // Id column is padded to the add-label width so descriptions align.
        let desc0: String = id_columns[0]
            .chars()
            .skip(CREATE_TYPES_ADD_LABEL.len() + 2)
            .collect();
        let desc1: String = id_columns[1]
            .chars()
            .skip(CREATE_TYPES_ADD_LABEL.len() + 2)
            .collect();
        assert!(desc0.starts_with("Feature"), "{desc0:?}");
        assert!(desc1.starts_with("Bug fix"), "{desc1:?}");
        Ok(())
    }

    #[test]
    fn types_and_properties_complete_locally_in_create_fork_and_edit()
    -> Result<(), Box<dyn std::error::Error>> {
        for action in ['n', 'f', 'e'] {
            let (workspace, source) = child_editor_workspace()?;
            let mut state = State::new(&workspace);
            state.category = Category::Taxonomies;
            state.home_focus = HomeFocus::Content;
            select_taxonomy(&mut state, 3);
            state.handle_event(key(KeyCode::Char(action)), &workspace);
            let initial_types = state.editor.as_ref().ok_or("missing taxonomy")?.types.len();
            state.handle_event(ctrl(KeyCode::Char('n')), &workspace);
            assert!(matches!(state.screen, Screen::TypeEditor));
            state.handle_event(ctrl(KeyCode::Char('n')), &workspace);
            assert!(matches!(state.screen, Screen::PropertyEditor));
            state.handle_event(Event::Paste("intent".into()), &workspace);
            state.handle_event(key(KeyCode::Down), &workspace);
            state.handle_event(Event::Paste("Why this change is needed".into()), &workspace);
            for (width, height) in [(MINIMUM_WIDTH, MINIMUM_HEIGHT), (120, 30)] {
                let mut terminal = Terminal::new(TestBackend::new(width, height))?;
                terminal.draw(|frame| state.render(frame))?;
                assert!(row_text(&terminal, 0, 1, width).contains("/ add type / add p"));
                assert!(row_text(&terminal, 0, height - 1, width).contains("ctrl+s: complete"));
                assert!(text(&terminal).contains("31/100"));
            }
            state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
            assert!(matches!(state.screen, Screen::TypeEditor));
            assert_eq!(
                state
                    .type_editor
                    .as_ref()
                    .ok_or("missing type")?
                    .draft
                    .properties
                    .len(),
                1
            );
            assert_eq!(
                state.editor.as_ref().ok_or("missing taxonomy")?.types.len(),
                initial_types
            );
            for _ in 0..3 {
                state.handle_event(key(KeyCode::Up), &workspace);
            }
            assert_eq!(
                state.type_editor.as_ref().ok_or("missing type")?.focus,
                CreateFocus::Name
            );
            state.handle_event(Event::Paste("feat".into()), &workspace);
            state.handle_event(key(KeyCode::Down), &workspace);
            state.handle_event(Event::Paste("Feature change".into()), &workspace);
            for (width, height) in [(MINIMUM_WIDTH, MINIMUM_HEIGHT), (120, 30)] {
                let mut terminal = Terminal::new(TestBackend::new(width, height))?;
                terminal.draw(|frame| state.render(frame))?;
                assert!(text(&terminal).contains("+ add property"));
                assert!(text(&terminal).contains("required"));
                let [metadata, properties] =
                    state.type_editor.as_ref().ok_or("missing type")?.areas;
                assert_eq!(metadata.height, 8);
                assert_eq!(metadata.bottom(), properties.y);
                assert_eq!(
                    terminal.backend().buffer()[(metadata.x, metadata.y)].fg,
                    Color::Yellow
                );
                assert!(row_text(&terminal, 0, metadata.bottom() - 1, width).contains("18/80"));
            }
            state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
            assert!(matches!(state.screen, Screen::TaxonomyEditor));
            assert_eq!(
                state.editor.as_ref().ok_or("missing taxonomy")?.types.len(),
                initial_types + 1
            );
            if action == 'e' {
                state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
            } else {
                rename_creation(
                    &mut state,
                    &workspace,
                    if action == 'n' { "created" } else { "forked" },
                )?;
                if action == 'n' {
                    state.handle_event(key(KeyCode::Down), &workspace);
                    state.handle_event(Event::Paste("Created taxonomy".into()), &workspace);
                }
                stage_creation(&mut state, &workspace);
            }
            assert!(matches!(state.screen, Screen::Home));
            assert_eq!(workspace.saves.get(), 0);
            let id = TaxonomyId::new(match action {
                'n' => "created",
                'f' => "forked",
                _ => "original",
            })?;
            let catalog = state.global.as_ref().ok_or("missing global")?.catalog()?;
            let stored = catalog.find(&id).ok_or("missing staged taxonomy")?;
            let kind = stored.commit_types().last().ok_or("missing type")?;
            assert_eq!(kind.id().as_str(), "feat");
            assert_eq!(kind.properties()[0].key().as_str(), "intent");
            assert_eq!(
                *kind.properties()[0].requirement(),
                PropertyRequirement::Required
            );
            if action != 'n' {
                assert_eq!(stored.commit_types()[0], source.commit_types()[0]);
            }
            if action == 'f' {
                assert_eq!(catalog.find(source.id()), Some(&source));
            }
        }
        Ok(())
    }

    #[test]
    fn edited_properties_preserve_schema_and_policy_until_type_completion()
    -> Result<(), Box<dyn std::error::Error>> {
        let (workspace, source) = child_editor_workspace()?;
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        select_taxonomy(&mut state, 3);
        state.handle_event(key(KeyCode::Char('e')), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(key(KeyCode::Enter), &workspace);
        assert!(matches!(state.screen, Screen::PropertyEditor));
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        assert!(
            matches!(state.status, Some(Notice::Error(ref message)) if message.contains("100"))
        );
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(
            row_text(&terminal, 0, 1, MINIMUM_WIDTH)
                .contains("/ edit taxonomy / edit type / edit p")
        );
        let property = state.property_editor.as_mut().ok_or("missing property")?;
        property.focus = child_editor::PropertyFocus::Description;
        property.text.description.select_all();
        state.handle_event(Event::Paste("Edited notes".into()), &workspace);
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        assert!(matches!(state.screen, Screen::TypeEditor));
        assert_eq!(
            state
                .editor
                .as_ref()
                .ok_or("missing taxonomy")?
                .build()?
                .commit_types()[0],
            source.commit_types()[0]
        );
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        let draft = &state.editor.as_ref().ok_or("missing taxonomy")?.types[0];
        assert_eq!(draft.schema_version, 4);
        assert_eq!(draft.properties[0].description, "Edited notes");
        assert_eq!(
            draft.properties[0].requirement,
            PropertyRequirement::Optional
        );
        assert_eq!(
            workspace.global.borrow().catalog()?.find(source.id()),
            Some(&source)
        );
        Ok(())
    }

    #[test]
    fn duplicate_validation_and_discard_confirmations_preserve_parent_drafts()
    -> Result<(), Box<dyn std::error::Error>> {
        let (workspace, source) = child_editor_workspace()?;
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        select_taxonomy(&mut state, 3);
        state.handle_event(key(KeyCode::Char('e')), &workspace);
        state.handle_event(ctrl(KeyCode::Char('n')), &workspace);
        state.handle_event(Event::Paste("existing".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(Event::Paste("Duplicate".into()), &workspace);
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        assert!(
            matches!(state.status, Some(Notice::Error(ref message)) if message.contains("already exists"))
        );
        state.handle_event(key(KeyCode::Esc), &workspace);
        assert!(matches!(state.screen, Screen::ConfirmDiscardChild(_)));
        state.handle_event(key(KeyCode::Char('y')), &workspace);
        state.open_type_editor(Some(0));
        state.handle_event(ctrl(KeyCode::Char('n')), &workspace);
        state.handle_event(Event::Paste("notes".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(Event::Paste("Duplicate property".into()), &workspace);
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        assert!(
            matches!(state.status, Some(Notice::Error(ref message)) if message.contains("already exists"))
        );
        state.handle_event(key(KeyCode::Esc), &workspace);
        assert!(matches!(state.screen, Screen::ConfirmDiscardChild(_)));
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        assert!(matches!(state.screen, Screen::PropertyEditor));
        let property = state.property_editor.as_mut().ok_or("missing property")?;
        property.focus = child_editor::PropertyFocus::Name;
        property.text.name.select_all();
        state.handle_event(Event::Paste("intent".into()), &workspace);
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        assert_eq!(
            state
                .type_editor
                .as_ref()
                .ok_or("missing type")?
                .draft
                .properties
                .len(),
            2
        );
        state.handle_event(key(KeyCode::Esc), &workspace);
        state.handle_event(key(KeyCode::Char('y')), &workspace);
        assert!(
            matches!(state.screen, Screen::TaxonomyEditor) && state.editor_mode == EditorMode::Edit
        );
        assert_eq!(
            state
                .editor
                .as_ref()
                .ok_or("missing taxonomy")?
                .build()?
                .commit_types(),
            source.commit_types()
        );
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }

    #[test]
    fn staging_an_unchanged_edit_reports_that_nothing_was_staged()
    -> Result<(), Box<dyn std::error::Error>> {
        let (workspace, source) = child_editor_workspace()?;
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        select_taxonomy(&mut state, 3);
        let library = state.global.clone().ok_or("missing global")?;
        state.handle_event(key(KeyCode::Char('e')), &workspace);
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);

        assert!(
            matches!(state.status, Some(Notice::Info(ref message)) if message == "No changes to stage.")
        );
        assert!(matches!(state.screen, Screen::Home));
        assert_eq!(state.category, Category::Taxonomies);
        assert_eq!(
            state.global.as_ref().map(ConfigurationSession::global),
            Some(library.global())
        );

        state.handle_event(key(KeyCode::Char('e')), &workspace);
        state.handle_event(key(KeyCode::Up), &workspace);
        state
            .create_text
            .as_mut()
            .ok_or("missing text")?
            .description
            .move_cursor(CursorMove::End);
        state.handle_event(Event::Paste(" revised".into()), &workspace);
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);

        assert!(
            matches!(state.status, Some(Notice::Info(ref message)) if message.starts_with("Staged taxonomy \"original\"."))
        );
        let staged = state
            .global
            .as_ref()
            .ok_or("missing global")?
            .catalog()?
            .find(source.id())
            .cloned()
            .ok_or("missing taxonomy")?;
        assert_eq!(staged.description().as_ref(), "Original taxonomy revised");
        assert_eq!(staged.version(), TaxonomyVersion::new(4)?);
        Ok(())
    }

    #[test]
    fn property_list_mouse_selection_changes_focus_without_moving_text_cursors()
    -> Result<(), Box<dyn std::error::Error>> {
        let (workspace, _) = child_editor_workspace()?;
        for (width, height) in [(MINIMUM_WIDTH, MINIMUM_HEIGHT), (120, 30)] {
            let mut state = State::new(&workspace);
            state.category = Category::Taxonomies;
            state.home_focus = HomeFocus::Content;
            select_taxonomy(&mut state, 3);
            state.handle_event(key(KeyCode::Char('f')), &workspace);
            state.open_type_editor(Some(0));
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| state.render(frame))?;
            let [metadata, properties] = state.type_editor.as_ref().ok_or("missing type")?.areas;
            state.handle_event(
                Event::Mouse(ratatui::crossterm::event::MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: properties.x + 2,
                    row: properties.y + 2,
                    modifiers: KeyModifiers::NONE,
                }),
                &workspace,
            );
            assert_eq!(
                state.type_editor.as_ref().ok_or("missing type")?.selected,
                1
            );
            terminal.draw(|frame| state.render(frame))?;
            assert_eq!(
                terminal.backend().buffer()[(metadata.x, metadata.y)].fg,
                Color::DarkGray
            );
            assert_eq!(
                terminal.backend().buffer()[(properties.x, properties.y)].fg,
                Color::Yellow
            );
            assert!(
                row_text(&terminal, properties.x, properties.y + 2, properties.width)
                    .contains("optional")
            );
            state.handle_event(
                Event::Mouse(ratatui::crossterm::event::MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: metadata.x + 2,
                    row: metadata.y + 2,
                    modifiers: KeyModifiers::NONE,
                }),
                &workspace,
            );
            assert_eq!(
                state.type_editor.as_ref().ok_or("missing type")?.focus,
                CreateFocus::Types
            );
            assert_eq!(
                state
                    .type_editor
                    .as_ref()
                    .ok_or("missing type")?
                    .text
                    .name
                    .cursor(),
                (0, "existing".len())
            );
            state.handle_event(key(KeyCode::Enter), &workspace);
            terminal.draw(|frame| state.render(frame))?;
            assert!(row_text(&terminal, 0, 1, width).contains("/ edit type / edit p"));
            assert!(text(&terminal).contains("135/100"));
            state.handle_event(key(KeyCode::Esc), &workspace);
            assert!(matches!(state.screen, Screen::TypeEditor));
            state.handle_event(key(KeyCode::Esc), &workspace);
            assert!(
                matches!(state.screen, Screen::TaxonomyEditor)
                    && state.editor_mode == EditorMode::Create
            );
        }
        Ok(())
    }

    #[test]
    fn taxonomy_escape_confirms_only_changed_drafts_and_supports_mouse_choices()
    -> Result<(), Box<dyn std::error::Error>> {
        for (action, width, height) in [('n', 60, 18), ('f', 120, 30), ('e', 60, 18)] {
            let (workspace, source) = child_editor_workspace()?;
            let mut state = State::new(&workspace);
            state.category = Category::Taxonomies;
            state.home_focus = HomeFocus::Content;
            select_taxonomy(&mut state, 3);
            state.handle_event(key(KeyCode::Char(action)), &workspace);
            assert!(!state.taxonomy_is_dirty());
            state.handle_event(key(KeyCode::Esc), &workspace);
            assert!(matches!(state.screen, Screen::Home));
            state.handle_event(key(KeyCode::Char(action)), &workspace);
            if action == 'e' {
                state.handle_event(key(KeyCode::Up), &workspace);
                state.handle_event(Event::Paste(" changed".into()), &workspace);
            } else {
                state.handle_event(Event::Paste("x".into()), &workspace);
            }
            assert!(state.taxonomy_is_dirty());
            let draft = state.editor.clone();
            let cursor = state.create_text.as_ref().map(|text| text.name.cursor());
            state.handle_event(key(KeyCode::Esc), &workspace);
            assert!(matches!(state.screen, Screen::ConfirmDiscardCreate(_)));
            assert!(state.dialog.buttons.is_none());
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| state.render(frame))?;
            assert!(text(&terminal).contains("Discard Taxonomy"));
            let buttons = state.dialog.buttons.ok_or("missing buttons")?;
            state.handle_event(click(Rect::new(0, 0, 1, 1)), &workspace);
            state.handle_event(Event::Paste("ignored".into()), &workspace);
            state.handle_event(key(KeyCode::Down), &workspace);
            assert!(matches!(state.screen, Screen::ConfirmDiscardCreate(_)));
            state.handle_event(click(buttons.keep), &workspace);
            assert!(matches!(state.screen, Screen::TaxonomyEditor));
            assert_eq!(state.editor, draft);
            assert_eq!(
                state.create_text.as_ref().map(|text| text.name.cursor()),
                cursor
            );
            assert!(state.dialog.buttons.is_none());
            state.handle_event(key(KeyCode::Esc), &workspace);
            state.handle_event(click(buttons.confirm), &workspace);
            assert!(matches!(state.screen, Screen::ConfirmDiscardCreate(_)));
            state.handle_event(key(KeyCode::Enter), &workspace);
            assert!(state.taxonomy_is_dirty());
            state.handle_event(key(KeyCode::Esc), &workspace);
            terminal.draw(|frame| state.render(frame))?;
            let buttons = state.dialog.buttons.ok_or("missing buttons")?;
            state.handle_event(click(buttons.confirm), &workspace);
            assert!(matches!(state.screen, Screen::Home));
            assert!(state.editor.is_none());
            assert!(state.initial_editor.is_none());
            assert!(state.dialog.buttons.is_none());
            assert_eq!(
                workspace.global.borrow().catalog()?.find(source.id()),
                Some(&source)
            );
            assert_eq!(workspace.saves.get(), 0);
        }
        Ok(())
    }

    #[test]
    fn child_buttons_and_library_breadcrumb_preserve_discard_scope()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        state.handle_event(ctrl(KeyCode::Char('n')), &workspace);
        state.handle_event(Event::Paste("feat".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(Event::Paste("Feature".into()), &workspace);
        let type_cursor = state
            .type_editor
            .as_ref()
            .ok_or("missing type")?
            .text
            .description
            .cursor();
        state.handle_event(ctrl(KeyCode::Char('n')), &workspace);
        state.handle_event(Event::Paste("intent".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(Event::Paste("Why".into()), &workspace);
        let property_cursor = state
            .property_editor
            .as_ref()
            .ok_or("missing property")?
            .text
            .description
            .cursor();
        state.handle_event(key(KeyCode::Esc), &workspace);
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(text(&terminal).contains("Discard Property"));
        let buttons = state.dialog.buttons.ok_or("missing buttons")?;
        state.handle_event(click(buttons.keep), &workspace);
        assert!(matches!(state.screen, Screen::PropertyEditor));
        assert_eq!(
            state
                .property_editor
                .as_ref()
                .ok_or("missing property")?
                .text
                .description
                .cursor(),
            property_cursor
        );
        state.handle_event(key(KeyCode::Esc), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        let buttons = state.dialog.buttons.ok_or("missing buttons")?;
        state.handle_event(click(buttons.confirm), &workspace);
        assert!(state.property_editor.is_none());
        assert!(
            state
                .type_editor
                .as_ref()
                .ok_or("missing type")?
                .draft
                .properties
                .is_empty()
        );
        assert_eq!(
            state
                .type_editor
                .as_ref()
                .ok_or("missing type")?
                .text
                .description
                .cursor(),
            type_cursor
        );
        state.handle_event(key(KeyCode::Esc), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        assert!(text(&terminal).contains("Discard Type"));
        let buttons = state.dialog.buttons.ok_or("missing buttons")?;
        state.handle_event(click(buttons.keep), &workspace);
        assert_eq!(
            state
                .type_editor
                .as_ref()
                .ok_or("missing type")?
                .text
                .description
                .cursor(),
            type_cursor
        );
        state.handle_event(key(KeyCode::Esc), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        let buttons = state.dialog.buttons.ok_or("missing buttons")?;
        state.handle_event(click(buttons.confirm), &workspace);
        assert!(
            state
                .editor
                .as_ref()
                .ok_or("missing taxonomy")?
                .types
                .is_empty()
        );
        assert!(!state.taxonomy_is_dirty());
        state.handle_event(ctrl(KeyCode::Char('n')), &workspace);
        state.handle_event(Event::Paste("feat".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(Event::Paste("Feature".into()), &workspace);
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        assert!(state.taxonomy_is_dirty());
        assert_eq!(
            state.editor.as_ref().ok_or("missing taxonomy")?.types.len(),
            1
        );
        state.handle_event(key(KeyCode::Esc), &workspace);
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        assert!(matches!(state.screen, Screen::TaxonomyEditor));
        state.handle_event(key(KeyCode::Enter), &workspace);
        state.handle_event(ctrl(KeyCode::Char('n')), &workspace);
        state.handle_event(Event::Paste("new-property".into()), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        let breadcrumb = state.breadcrumb_area;
        state.handle_event(click(breadcrumb), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        assert!(text(&terminal).contains("Discard Taxonomy"));
        let buttons = state.dialog.buttons.ok_or("missing buttons")?;
        state.handle_event(click(buttons.keep), &workspace);
        assert!(matches!(state.screen, Screen::PropertyEditor));
        assert_eq!(
            state
                .property_editor
                .as_ref()
                .ok_or("missing property")?
                .text
                .name
                .lines(),
            &["new-property"]
        );
        state.handle_event(click(breadcrumb), &workspace);
        terminal.draw(|frame| state.render(frame))?;
        let buttons = state.dialog.buttons.ok_or("missing buttons")?;
        state.handle_event(click(buttons.confirm), &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(
            state.editor.is_none()
                && state.type_editor.is_none()
                && state.property_editor.is_none()
        );
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }

    #[test]
    fn configuration_discard_dialog_invalidates_mouse_targets_after_resize()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        state.handle_event(Event::Paste("team".into()), &workspace);
        state.handle_event(key(KeyCode::Esc), &workspace);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        let old = state.dialog.buttons.ok_or("missing buttons")?;
        state.handle_event(Event::Resize(30, 7), &workspace);
        assert!(state.dialog.buttons.is_none());
        state.handle_event(click(old.confirm), &workspace);
        assert!(matches!(state.screen, Screen::ConfirmDiscardCreate(_)));
        let mut tiny = Terminal::new(TestBackend::new(30, 7))?;
        tiny.draw(|frame| state.render(frame))?;
        assert!(state.dialog.buttons.is_none());
        state.handle_event(click(old.keep), &workspace);
        assert!(matches!(state.screen, Screen::ConfirmDiscardCreate(_)));
        state.handle_event(key(KeyCode::Esc), &workspace);
        assert!(matches!(state.screen, Screen::TaxonomyEditor));
        assert!(state.taxonomy_is_dirty());
        assert_eq!(
            state
                .create_text
                .as_ref()
                .ok_or("missing text")?
                .name
                .lines(),
            &["team"]
        );
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }

    fn heading_row(terminal: &Terminal<TestBackend>, heading: &str) -> Option<String> {
        let area = terminal.backend().buffer().area;
        (0..area.height)
            .map(|y| row_text(terminal, 0, y, area.width))
            .find(|row| row.contains(&format!("{heading} ⠒")))
    }

    fn error_text(state: &State) -> Option<&str> {
        match &state.status {
            Some(Notice::Error(message)) => Some(message),
            _ => None,
        }
    }

    #[test]
    fn create_name_heading_previews_the_saved_identifier_without_rewriting_the_name()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        let long = "Release Notes ".repeat(5);
        let long_note = format!(
            "saves as {}",
            "release-notes-".repeat(5).trim_end_matches('-')
        );
        for (name, compact, wide) in [
            (
                "Team Changes",
                "saves as team-changes",
                "saves as team-changes",
            ),
            ("2fa", "start with a letter", "start with a letter"),
            ("Café", "use letters a-z", "use letters a-z"),
            ("!!!", "use letters a-z", "use letters a-z"),
            (
                long.as_str(),
                "saves as release-notes-release-notes-rel…",
                long_note.as_str(),
            ),
            ("team-changes", "", ""),
            ("", "", ""),
        ] {
            rename_creation(&mut state, &workspace, name)?;
            assert_eq!(state.status.as_ref().map(|_| ()), None);
            for (width, height, note) in [(MINIMUM_WIDTH, MINIMUM_HEIGHT, compact), (120, 30, wide)]
            {
                let mut terminal = Terminal::new(TestBackend::new(width, height))?;
                terminal.draw(|frame| state.render(frame))?;
                let heading = heading_row(&terminal, "Name").ok_or("missing Name heading")?;
                assert_eq!(heading.chars().count(), usize::from(width));
                if note.is_empty() {
                    assert!(heading.ends_with("⠒│"), "{name:?}: {heading:?}");
                } else {
                    assert!(
                        heading.ends_with(&format!(" {note}│")),
                        "{name:?}: {heading:?}"
                    );
                }
            }
            assert_eq!(
                state
                    .create_text
                    .as_ref()
                    .ok_or("missing text")?
                    .name
                    .lines(),
                &[name.to_owned()]
            );
        }
        Ok(())
    }

    #[test]
    fn child_names_complete_as_identifiers_and_report_plain_errors()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        state.handle_event(ctrl(KeyCode::Char('n')), &workspace);
        state.handle_event(Event::Paste("Breaking\nChange".into()), &workspace);
        assert!(state.status.is_none());
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        let heading = heading_row(&terminal, "Name").ok_or("missing Name heading")?;
        assert!(
            heading.ends_with(" saves as breaking-change│"),
            "{heading:?}"
        );
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(
            Event::Paste("  Changes that break callers.  \n\n".into()),
            &workspace,
        );

        state.handle_event(ctrl(KeyCode::Char('n')), &workspace);
        state.handle_event(Event::Paste("two words".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(Event::Paste("Why it matters \n".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(key(KeyCode::Right), &workspace);
        state.handle_event(key(KeyCode::Right), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(Event::Paste("Behavior\r\nChanged".into()), &workspace);
        assert!(state.status.is_none());
        for (width, height) in [(MINIMUM_WIDTH, MINIMUM_HEIGHT), (120, 30)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| state.render(frame))?;
            let heading =
                heading_row(&terminal, "Condition name").ok_or("missing condition heading")?;
            assert!(
                heading.ends_with(" saves as behavior-changed│"),
                "{heading:?}"
            );
            if height > MINIMUM_HEIGHT {
                let heading = heading_row(&terminal, "Name").ok_or("missing Name heading")?;
                assert!(heading.ends_with(" saves as two-words│"), "{heading:?}");
            }
        }
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(Event::Paste("\nWhen callers break.\n\n".into()), &workspace);
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        assert!(state.status.is_none());
        assert!(matches!(state.screen, Screen::TypeEditor));
        let property = &state
            .type_editor
            .as_ref()
            .ok_or("missing type")?
            .draft
            .properties[0];
        assert_eq!(property.key, "two-words");
        assert_eq!(property.description, "Why it matters");
        let PropertyRequirement::Conditional(condition) = &property.requirement else {
            return Err("missing condition".into());
        };
        assert_eq!(condition.id().as_str(), "behavior-changed");
        assert_eq!(condition.rationale(), "When callers break.");

        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        assert!(state.status.is_none());
        let types = &state.editor.as_ref().ok_or("missing taxonomy")?.types;
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].id, "breaking-change");
        assert_eq!(types[0].description, "Changes that break callers.");
        let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
        terminal.draw(|frame| state.render(frame))?;
        assert!(text(&terminal).contains("breaking-change"));

        state.handle_event(ctrl(KeyCode::Char('n')), &workspace);
        state.handle_event(Event::Paste("BREAKING change".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(Event::Paste("Duplicate".into()), &workspace);
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        assert_eq!(
            error_text(&state),
            Some("A type named \"breaking-change\" already exists in this taxonomy.")
        );
        assert!(matches!(state.screen, Screen::TypeEditor));
        assert_eq!(
            state.editor.as_ref().ok_or("missing taxonomy")?.types.len(),
            1
        );

        for (name, message) in [
            ("2fa", "Type name must start with a letter: \"2fa\"."),
            (
                "Café",
                "Type name can't use \"é\". Use letters a-z and digits.",
            ),
            ("!!!", "Type name needs a letter."),
            ("   ", "Type name is required."),
        ] {
            let editor = state.type_editor.as_mut().ok_or("missing type")?;
            editor.focus = CreateFocus::Name;
            editor.text.name.select_all();
            state.handle_event(Event::Paste(name.into()), &workspace);
            state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
            assert_eq!(error_text(&state), Some(message));
            assert!(matches!(state.screen, Screen::TypeEditor));
        }
        Ok(())
    }

    #[test]
    fn staging_trims_edited_prose_and_names_the_staged_identifier()
    -> Result<(), Box<dyn std::error::Error>> {
        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        state.handle_event(key(KeyCode::Char('n')), &workspace);
        state.handle_event(Event::Paste("Team Changes".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(Event::Paste(" \n ".into()), &workspace);
        state.handle_event(ctrl(KeyCode::Char('n')), &workspace);
        state.handle_event(Event::Paste("feat".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(Event::Paste("Feature".into()), &workspace);
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        stage_creation(&mut state, &workspace);
        assert_eq!(
            error_text(&state),
            Some("Taxonomy description is required.")
        );
        assert!(
            matches!(state.screen, Screen::TaxonomyEditor)
                && state.editor_mode == EditorMode::Create
        );

        state.create_focus = CreateFocus::Description;
        state
            .create_text
            .as_mut()
            .ok_or("missing text")?
            .description
            .select_all();
        state.handle_event(Event::Paste("  Team changes \n\n".into()), &workspace);
        stage_creation(&mut state, &workspace);
        assert!(matches!(state.screen, Screen::Home));
        assert!(matches!(
            state.status,
            Some(Notice::Info(ref message))
                if message == "Staged taxonomy \"team-changes\". Ctrl+S reviews before applying."
        ));
        let staged = state
            .global
            .as_ref()
            .ok_or("missing global")?
            .catalog()?
            .find(&TaxonomyId::new("team-changes")?)
            .cloned()
            .ok_or("missing staged taxonomy")?;
        assert_eq!(staged.description().as_str(), "Team changes");
        assert_eq!(staged.commit_types()[0].id().as_str(), "feat");
        Ok(())
    }

    #[test]
    fn notices_read_as_sentences_and_name_the_blank_field() {
        assert_eq!(sentence("global load failed"), "Global load failed.");
        assert_eq!(sentence(" Already a sentence. "), "Already a sentence.");
        assert_eq!(sentence("Apply now?"), "Apply now?");
        assert_eq!(sentence(""), "");

        let kind = TypeDraft {
            id: "release".into(),
            description: " ".into(),
            schema_version: 1,
            properties: Vec::new(),
        };
        assert_eq!(kind.build(), Err("Type description is required.".into()));
        let property = PropertyDraft {
            key: "notes".into(),
            description: " ".into(),
            ..PropertyDraft::empty()
        };
        assert_eq!(
            property.build(),
            Err("Property description is required.".into())
        );
        let taxonomy = TaxonomyDraft {
            id: "team".into(),
            description: "Team changes".into(),
            ..TaxonomyDraft::empty()
        };
        assert_eq!(
            taxonomy.build(),
            Err("Add a type before staging the taxonomy.".into())
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

    #[test]
    fn every_built_in_entry_fits_its_editor_budget() {
        fn length(text: &str) -> usize {
            text.graphemes(true).count()
        }
        let mut over = Vec::new();
        for taxonomy in gitserious_core::built_in_taxonomies() {
            let used = length(taxonomy.id().as_str()) + length(taxonomy.description().as_str());
            if used > CREATE_TEXT_LIMIT {
                over.push(format!("{} {used}/{CREATE_TEXT_LIMIT}", taxonomy.id()));
            }
            for kind in taxonomy.commit_types() {
                let used = length(kind.id().as_str()) + length(kind.description());
                if used > child_editor::TYPE_LIMIT {
                    over.push(format!(
                        "{}/{} {used}/{}",
                        taxonomy.id(),
                        kind.id(),
                        child_editor::TYPE_LIMIT
                    ));
                }
                for property in kind.properties() {
                    let used = length(property.key().as_str()) + length(property.description());
                    if used > child_editor::PROPERTY_LIMIT {
                        over.push(format!(
                            "{}/{}/{} {used}/{}",
                            taxonomy.id(),
                            kind.id(),
                            property.key(),
                            child_editor::PROPERTY_LIMIT
                        ));
                    }
                }
            }
        }
        assert!(
            over.is_empty(),
            "built-in entries over their editor budget:\n{}",
            over.join("\n")
        );
    }

    #[test]
    fn text_fields_open_at_the_end_and_take_home_and_end() -> Result<(), Box<dyn std::error::Error>>
    {
        let (workspace, _) = child_editor_workspace()?;
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        select_taxonomy(&mut state, 3);
        state.handle_event(key(KeyCode::Char('e')), &workspace);
        state.handle_event(key(KeyCode::Up), &workspace);
        let description = |state: &State| -> Result<_, Box<dyn std::error::Error>> {
            let text = &state
                .create_text
                .as_ref()
                .ok_or("missing text")?
                .description;
            Ok((text.lines().join("\n"), text.cursor()))
        };
        assert_eq!(description(&state)?, ("Original taxonomy".into(), (0, 17)));
        state.handle_event(key(KeyCode::Home), &workspace);
        assert_eq!(description(&state)?.1, (0, 0));
        state.handle_event(key(KeyCode::End), &workspace);
        for character in " notes".chars() {
            state.handle_event(key(KeyCode::Char(character)), &workspace);
        }
        assert_eq!(
            description(&state)?,
            ("Original taxonomy notes".into(), (0, 23))
        );
        Ok(())
    }

    #[test]
    fn edit_opens_the_create_editor_with_its_name_fixed() -> Result<(), Box<dyn std::error::Error>>
    {
        let (workspace, source) = child_editor_workspace()?;
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        select_taxonomy(&mut state, 3);
        state.handle_event(key(KeyCode::Char('e')), &workspace);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| state.render(frame))?;
        let rendered = text(&terminal);
        assert!(rendered.contains("fixed after creation"));
        assert!(rendered.contains("Original taxonomy"));
        assert!(rendered.contains(CREATE_TYPES_ADD_LABEL));
        assert!(rendered.contains("existing"));
        assert!(!rendered.contains("Metadata"));
        assert_eq!(state.create_focus, CreateFocus::Types);

        for _ in 0..3 {
            state.handle_event(key(KeyCode::Up), &workspace);
        }
        assert_eq!(state.create_focus, CreateFocus::Description);
        state.handle_event(Event::Paste("renamed".into()), &workspace);
        assert_eq!(
            state
                .create_text
                .as_ref()
                .ok_or("missing text")?
                .name
                .lines()
                .join("\n"),
            source.id().as_str()
        );
        Ok(())
    }

    #[test]
    fn failed_staging_keeps_the_edited_description_as_typed()
    -> Result<(), Box<dyn std::error::Error>> {
        let (workspace, _) = child_editor_workspace()?;
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        select_taxonomy(&mut state, 3);
        state.handle_event(key(KeyCode::Char('e')), &workspace);
        state.handle_event(key(KeyCode::Up), &workspace);
        assert_eq!(state.create_focus, CreateFocus::Description);
        state
            .create_text
            .as_mut()
            .ok_or("missing text")?
            .description
            .select_all();
        state.handle_event(Event::Paste("  Edited notes \n".into()), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        state.handle_event(key(KeyCode::Down), &workspace);
        assert_eq!(state.child_selected, 1);
        state.handle_event(key(KeyCode::Char('d')), &workspace);
        assert!(matches!(state.screen, Screen::ConfirmRemoveType(0)));
        state.handle_event(key(KeyCode::Char('y')), &workspace);
        state.handle_event(ctrl(KeyCode::Char('s')), &workspace);
        assert_eq!(
            error_text(&state),
            Some("Add a type before staging the taxonomy.")
        );
        assert!(
            matches!(state.screen, Screen::TaxonomyEditor) && state.editor_mode == EditorMode::Edit
        );
        assert_eq!(
            state
                .create_text
                .as_ref()
                .ok_or("missing text")?
                .description
                .lines()
                .join("\n"),
            "  Edited notes \n"
        );
        Ok(())
    }

    #[test]
    fn natural_names_create_and_stage_a_taxonomy_with_ctrl_s_at_every_level()
    -> Result<(), Box<dyn std::error::Error>> {
        fn send(state: &mut State, workspace: &Workspace, event: Event) {
            state.handle_event(event, workspace);
            assert_eq!(error_text(state), None);
        }
        fn type_text(state: &mut State, workspace: &Workspace, text: &str) {
            for character in text.chars() {
                send(state, workspace, key(KeyCode::Char(character)));
            }
        }
        fn assert_preview(
            state: &mut State,
            heading: &str,
            note: &str,
        ) -> Result<(), Box<dyn std::error::Error>> {
            for (width, height) in [(MINIMUM_WIDTH, MINIMUM_HEIGHT), (120, 30)] {
                let mut terminal = Terminal::new(TestBackend::new(width, height))?;
                terminal.draw(|frame| state.render(frame))?;
                let row = heading_row(&terminal, heading).ok_or("missing heading")?;
                assert!(row.ends_with(&format!(" {note}│")), "{row:?}");
            }
            Ok(())
        }

        let workspace = Workspace::new();
        let mut state = State::new(&workspace);
        state.category = Category::Taxonomies;
        state.home_focus = HomeFocus::Content;
        send(&mut state, &workspace, key(KeyCode::Char('n')));
        type_text(&mut state, &workspace, "Team Changes");
        assert_preview(&mut state, "Name", "saves as team-changes")?;
        send(&mut state, &workspace, key(KeyCode::Enter));
        type_text(&mut state, &workspace, "Changes our team ships");

        send(&mut state, &workspace, ctrl(KeyCode::Char('n')));
        type_text(&mut state, &workspace, "Breaking change");
        assert_preview(&mut state, "Name", "saves as breaking-change")?;
        send(&mut state, &workspace, key(KeyCode::Enter));
        type_text(&mut state, &workspace, "Changes that break callers");

        send(&mut state, &workspace, ctrl(KeyCode::Char('n')));
        type_text(&mut state, &workspace, "two words");
        assert_preview(&mut state, "Name", "saves as two-words")?;
        send(&mut state, &workspace, key(KeyCode::Enter));
        type_text(&mut state, &workspace, "What the change breaks");

        send(&mut state, &workspace, ctrl(KeyCode::Char('s')));
        assert!(matches!(state.screen, Screen::TypeEditor));
        send(&mut state, &workspace, ctrl(KeyCode::Char('s')));
        assert!(
            matches!(state.screen, Screen::TaxonomyEditor)
                && state.editor_mode == EditorMode::Create
        );
        send(&mut state, &workspace, ctrl(KeyCode::Char('s')));
        assert!(matches!(state.screen, Screen::Home));
        assert!(matches!(
            state.status,
            Some(Notice::Info(ref message))
                if message == "Staged taxonomy \"team-changes\". Ctrl+S reviews before applying."
        ));
        let staged = state
            .global
            .as_ref()
            .ok_or("missing global")?
            .catalog()?
            .find(&TaxonomyId::new("team-changes")?)
            .cloned()
            .ok_or("missing staged taxonomy")?;
        let [kind] = staged.commit_types() else {
            return Err("expected one type".into());
        };
        assert_eq!(kind.id().as_str(), "breaking-change");
        assert_eq!(kind.properties().len(), 1);
        assert_eq!(kind.properties()[0].key().as_str(), "two-words");
        assert_eq!(workspace.saves.get(), 0);
        Ok(())
    }
}
