use std::cell::{Cell, RefCell};

use crate::config::library_lookup::LibraryLookup;
use gitserious_app::{GlobalConfiguration, RepositoryRoot};
use gitserious_core::{
    CommitTypeDefinition, CommitTypeId, ConditionId, Description, PropertyCondition,
    PropertyDefinition, PropertyKey, PropertyRequirement, SchemaVersion, TaxonomyVersion,
};

use super::*;

pub(super) type Result<T = (), E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

const LIBRARY_PATH: &str = "/home/tester/.config/gitserious/config.toml";

pub(super) struct Workspace {
    pub(super) global: RefCell<LibraryDraft>,
    pub(super) project: RefCell<ProjectSnapshot>,
    pub(super) saves: Cell<usize>,
    pub(super) library_path: PathBuf,
}

impl Workspace {
    pub(super) fn new() -> Self {
        let global = GlobalConfiguration::default();
        let root = RepositoryRoot::new(std::env::temp_dir()).unwrap_or_else(|_| unreachable!());
        Self {
            global: RefCell::new(LibraryDraft::from_configuration(global)),
            project: RefCell::new(ProjectSnapshot::new(root, None)),
            saves: Cell::new(0),
            library_path: PathBuf::from(LIBRARY_PATH),
        }
    }

    pub(super) fn saves(&self) -> usize {
        self.saves.get()
    }
}

impl ConfigurationWorkspace for Workspace {
    fn load_library(&self) -> Result<LibraryDraft, String> {
        Ok(self.global.borrow().clone())
    }

    fn save_library(&self, library: &LibraryDraft) -> Result<LibraryDraft, String> {
        self.saves.set(self.saves.get() + 1);
        if self.global.borrow().persisted() != library.persisted() {
            return Err("global configuration changed while editing".into());
        }
        let saved = library
            .saved_on(
                "2026-11-08"
                    .parse::<gitserious_app::TaxonomyDate>()
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|problems| format!("{problems:?}"))?;
        *self.global.borrow_mut() = saved.clone();
        Ok(saved)
    }

    fn load_project(&self) -> Result<ProjectSnapshot, String> {
        Ok(self.project.borrow().clone())
    }

    fn library_taxonomies(&self) -> Result<Vec<Taxonomy>, String> {
        Ok(self
            .global
            .borrow()
            .persisted()
            .map(|library| {
                library
                    .custom_taxonomies()
                    .iter()
                    .filter_map(gitserious_app::LibraryTaxonomy::usable)
                    .collect()
            })
            .unwrap_or_default())
    }

    fn library_path(&self) -> PathBuf {
        self.library_path.clone()
    }
}

pub(super) fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

/// Creates a taxonomy through the palette, which leaves the outline cursor where it was.
pub(super) fn create_taxonomy(state: &mut State, workspace: &dyn ConfigurationWorkspace) {
    for event in [
        key(KeyCode::Char('/')),
        Event::Paste("create taxonomy".into()),
        key(KeyCode::Enter),
    ] {
        assert!(!state.handle_event(event, workspace));
    }
}

pub(super) fn footer_text(text: &str) -> String {
    let mut text = text.to_owned();
    for mode in [" NAV ", " EDIT "] {
        if let Some(at) = text.find(mode) {
            let end = at + mode.len();
            let hints = text[end..].trim_start();
            text = format!(" {mode}  {hints}");
            break;
        }
    }
    for key in [
        "←/→",
        "←",
        "q",
        "↑/↓",
        "enter",
        "esc",
        "f",
        "e",
        "d",
        "c",
        "alt+↑/↓",
        "tab",
        "/",
    ] {
        text = text.replace(&format!(" {key}  "), &format!(" {key} "));
    }
    text
}

pub(super) fn team() -> Result<Workspace, Box<dyn std::error::Error>> {
    let property = |key: &str, requirement| -> Result<_, Box<dyn std::error::Error>> {
        Ok(PropertyDefinition::new(
            PropertyKey::new(key)?,
            format!("The {key}."),
            requirement,
        )?)
    };
    let commit_type = |id: &str, properties| -> Result<_, Box<dyn std::error::Error>> {
        Ok(CommitTypeDefinition::new(
            Some(SchemaVersion::new(1)?),
            CommitTypeId::new(id)?,
            format!("A {id} change."),
            properties,
        )?)
    };
    let constraints = PropertyRequirement::Conditional(PropertyCondition::new(
        ConditionId::new("has-limits")?,
        "Record limits when a change is bounded by them.",
    )?);
    let team = Taxonomy::new(
        TaxonomyId::new("team")?,
        Some(TaxonomyVersion::new(1)?),
        Description::new("Team taxonomy")?,
        None,
        vec![
            commit_type(
                "feat",
                vec![
                    property("intent", PropertyRequirement::Required)?,
                    property("constraints", constraints)?,
                ],
            )?,
            commit_type(
                "fix",
                vec![property("cause", PropertyRequirement::Required)?],
            )?,
            commit_type("docs", Vec::new())?,
        ],
    )?;
    let workspace = Workspace::new();
    let conventional = TaxonomyId::new("conventional")?;
    *workspace.global.borrow_mut() = LibraryDraft::from_configuration(GlobalConfiguration::new(
        1,
        conventional.clone(),
        vec![conventional],
        vec![team.into()],
    )?);
    Ok(workspace)
}

pub(super) fn press(state: &mut State, workspace: &Workspace, keys: &[KeyCode]) {
    for &code in keys {
        assert!(!state.handle_event(
            Event::Key(KeyEvent::new(code, KeyModifiers::NONE)),
            workspace
        ));
    }
}

/// The Library with the built-ins, the custom `team`, and `draft-ideas`, saved with no types,
/// with conventional open under the cursor.
pub(super) fn fixture() -> Result<(Workspace, State)> {
    let workspace = team()?;
    let mut state = State::new(&workspace);
    press(&mut state, &workspace, &[KeyCode::Char('2')]);
    create_taxonomy(&mut state, &workspace);
    state.handle_event(Event::Paste("Draft ideas".into()), &workspace);
    press(&mut state, &workspace, &[KeyCode::Enter]);
    state.handle_event(
        Event::Paste("Ideas that have not earned a type yet.".into()),
        &workspace,
    );
    assert!(!state.handle_event(
        Event::Key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
        &workspace
    ));
    select(&mut state, &["conventional"])?;
    press(&mut state, &workspace, &[KeyCode::Enter]);
    Ok((workspace, state))
}

pub(super) fn select(state: &mut State, path: &[&str]) -> Result {
    let id = state
        .library()
        .and_then(|library| library.find_path(path))
        .ok_or("node missing")?;
    state.taxonomies.select(id);
    Ok(())
}
