use crate::library_lookup::LibraryLookup;

use gitserious_app::{
    DraftField, GlobalConfiguration, ProjectRevision, RepositoryRoot, StoredProject,
    initial_project_configuration,
};

use ratatui::{Terminal, backend::TestBackend};

use super::taxonomies::Mode;
use super::tests::Workspace;
use super::*;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn press(state: &mut State, workspace: &Workspace, code: KeyCode) {
    assert!(!state.handle_event(
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE)),
        workspace
    ));
}

fn ctrl(state: &mut State, workspace: &Workspace, character: char) {
    assert!(!state.handle_event(
        Event::Key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::CONTROL
        )),
        workspace
    ));
}

/// Config opened on an initialized built-in project, with the Library unreadable.
fn fixture() -> Result<(Workspace, State)> {
    let workspace = Workspace::new();
    *workspace.project.borrow_mut() = ConfigurationSession::open_project(
        RepositoryRoot::new(std::env::temp_dir())?,
        Some(StoredProject {
            configuration: initial_project_configuration(&GlobalConfiguration::default(), None)?,
            revision: ProjectRevision::of_bytes(b"opened"),
        }),
    );
    let mut state = State::new(&workspace);
    state.global = None;
    state.global_error = "the Library is corrupt".into();
    Ok((workspace, state))
}

fn open_outline(state: &mut State, workspace: &Workspace) {
    press(state, workspace, KeyCode::Right);
    press(state, workspace, KeyCode::Down);
    assert_eq!(state.project_section, ProjectSection::Taxonomies);
    press(state, workspace, KeyCode::Enter);
    assert_eq!(state.editor(), Some(Editor::Project));
}

fn draft(state: &State) -> Result<&LibraryDraft> {
    Ok(state
        .project
        .as_ref()
        .and_then(ConfigurationSession::project_draft)
        .ok_or("the project has a draft")?)
}

fn select(state: &mut State, workspace: &Workspace, path: &[&str]) -> Result<NodeId> {
    for depth in 1..path.len() {
        let parent = draft(state)?
            .find_path(&path[..depth])
            .ok_or("parent missing")?;
        state.project_taxonomies.select(parent.clone());
        if !state
            .project_taxonomies
            .observe(Some(draft(state)?))
            .expanded
            .contains(&parent)
        {
            press(state, workspace, KeyCode::Enter);
        }
    }
    let id = draft(state)?.find_path(path).ok_or("node missing")?;
    state.project_taxonomies.select(id.clone());
    Ok(id)
}

fn replace(state: &mut State, workspace: &Workspace, text: &str) -> Result {
    let observation = state.project_taxonomies.observe(Some(draft(state)?));
    let Mode::Edit { node, field, .. } = &observation.mode else {
        return Err("not editing".into());
    };
    let length = draft(state)?
        .text(node, *field)
        .ok_or("field missing")?
        .chars()
        .count();
    press(state, workspace, KeyCode::Home);
    for _ in 0..length {
        press(state, workspace, KeyCode::Delete);
    }
    assert!(!state.handle_event(Event::Paste(text.into()), workspace));
    Ok(())
}

#[test]
fn project_taxonomies_edit_without_the_library_and_apply_only_after_review() -> Result {
    let (workspace, mut state) = fixture()?;
    open_outline(&mut state, &workspace);
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| state.render(frame))?;
    let screen = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(screen.contains("conventional") && screen.contains(" project "));
    assert!(
        !screen.contains(" custom "),
        "a project row named a Library origin"
    );
    let fix = select(&mut state, &workspace, &["conventional", "fix"])?;

    press(&mut state, &workspace, KeyCode::Char('e'));
    replace(&mut state, &workspace, "repair")?;
    press(&mut state, &workspace, KeyCode::Esc);

    assert!(!state.project_taxonomies.editing());
    assert_eq!(draft(&state)?.text(&fix, DraftField::Name), Some("repair"));
    assert_eq!(
        workspace.saves(),
        0,
        "a project edit was written before Apply"
    );
    assert!(state.scope_dirty(ConfigurationDestination::Project));

    ctrl(&mut state, &workspace, 's');
    let Screen::Review(ConfigurationDestination::Project) = state.screen else {
        return Err(format!("expected review, got {:?}", state.screen).into());
    };
    let review = state.project.as_ref().ok_or("project")?.review()?.changes;
    assert!(review.contains("  ~ conventional  v2 -> v3"), "{review}");
    press(&mut state, &workspace, KeyCode::Enter);
    assert_eq!(workspace.saves(), 1);
    Ok(())
}

#[test]
fn project_deletion_confirms_and_never_removes_the_default() -> Result {
    let (workspace, mut state) = fixture()?;
    open_outline(&mut state, &workspace);

    select(&mut state, &workspace, &["conventional"])?;
    press(&mut state, &workspace, KeyCode::Char('d'));
    assert!(matches!(state.screen, Screen::Home));
    assert!(matches!(&state.status, Some(Notice::Error(message)) if message.contains("default")));

    select(&mut state, &workspace, &["research"])?;
    press(&mut state, &workspace, KeyCode::Char('d'));
    let question = state.confirmation().ok_or("no confirmation")?;
    assert_eq!(
        question.message,
        "Delete taxonomy \"research\" from this project?"
    );
    press(&mut state, &workspace, KeyCode::Char('y'));

    assert!(draft(&state)?.find_path(&["research"]).is_none());
    assert_eq!(workspace.saves(), 0);
    Ok(())
}

#[test]
fn leaving_the_outline_returns_to_the_project_sections() -> Result {
    let (workspace, mut state) = fixture()?;
    open_outline(&mut state, &workspace);
    select(&mut state, &workspace, &["conventional"])?;

    press(&mut state, &workspace, KeyCode::Left);

    assert_eq!(state.editor(), None);
    assert_eq!(state.category, Category::Project);
    assert!(
        state
            .project_detail()
            .contains("Enter opens these taxonomies")
    );
    Ok(())
}

#[test]
fn an_uninitialized_project_has_no_outline_to_open() {
    let workspace = Workspace::new();
    let mut state = State::new(&workspace);
    press(&mut state, &workspace, KeyCode::Right);
    press(&mut state, &workspace, KeyCode::Down);
    press(&mut state, &workspace, KeyCode::Enter);

    assert_eq!(state.editor(), None);
    assert!(
        matches!(&state.status, Some(Notice::Error(message)) if message.contains("Initialize"))
    );
}
