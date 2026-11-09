use crate::library_lookup::LibraryLookup;

use gitserious_app::{
    DraftField, GlobalConfiguration, ProjectRevision, RepositoryRoot, StoredProject,
    initial_project_configuration,
};

use ratatui::{Terminal, backend::TestBackend};

use super::taxonomies::ObservedMode as Mode;
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
            configuration: initial_project_configuration(&GlobalConfiguration::default())?,
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
    assert!(screen.contains("conventional") && screen.contains(" default "));
    assert!(!screen.contains(" project "));
    assert!(
        !screen.contains(" custom "),
        "a project row named a Library origin"
    );
    assert!(
        !screen.contains("+ new taxonomy"),
        "the project outline offered a Library row"
    );
    let fix = select(&mut state, &workspace, &["conventional", "fix"])?;

    press(&mut state, &workspace, KeyCode::Char('e'));
    replace(&mut state, &workspace, "repair")?;
    ctrl(&mut state, &workspace, 'w');

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
    assert!(review.contains("  ~ conventional-v1"), "{review}");
    press(&mut state, &workspace, KeyCode::Enter);
    assert_eq!(workspace.saves(), 1);
    Ok(())
}

fn screen(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn project_taxonomy_editor_omits_library_dates() -> Result {
    let (workspace, mut state) = fixture()?;
    open_outline(&mut state, &workspace);
    select(&mut state, &workspace, &["conventional"])?;
    let mut terminal = Terminal::new(TestBackend::new(100, 40))?;
    terminal.draw(|frame| state.render(frame))?;
    let inspect = screen(&terminal);
    press(&mut state, &workspace, KeyCode::Char('e'));
    terminal.draw(|frame| state.render(frame))?;
    let edit = screen(&terminal);
    for (mode, text) in [("inspect", &inspect), ("edit", &edit)] {
        assert!(!text.contains("Created"), "{mode}\n{text}");
        assert!(!text.contains("Modified"), "{mode}\n{text}");
    }
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

/// Config opened on an initialized built-in project beside a Library holding `team`.
fn with_library() -> Result<(Workspace, State)> {
    let workspace = super::copy_tests::team()?;
    *workspace.project.borrow_mut() = ConfigurationSession::open_project(
        RepositoryRoot::new(std::env::temp_dir())?,
        Some(StoredProject {
            configuration: initial_project_configuration(&GlobalConfiguration::default())?,
            revision: ProjectRevision::of_bytes(b"opened"),
        }),
    );
    let state = State::new(&workspace);
    Ok((workspace, state))
}

fn import_row(state: &mut State, workspace: &Workspace, id: &str) -> Result {
    state.project_taxonomies.select(
        draft(state)?
            .find_path(&["conventional"])
            .ok_or("conventional")?,
    );
    press(state, workspace, KeyCode::Char('i'));
    let rows = state.catalog().ok_or("the Library loads")?;
    let index = rows
        .iter()
        .position(|taxonomy| taxonomy.id().as_str() == id)
        .ok_or("row missing")?;
    for _ in 0..index {
        press(state, workspace, KeyCode::Down);
    }
    press(state, workspace, KeyCode::Enter);
    Ok(())
}

#[test]
fn importing_copies_new_library_taxonomies_and_skips_identical_ones() -> Result {
    let (workspace, mut state) = with_library()?;
    open_outline(&mut state, &workspace);

    import_row(&mut state, &workspace, "team")?;
    assert!(matches!(state.screen, Screen::Home));
    assert!(draft(&state)?.find_path(&["team"]).is_some());
    assert!(matches!(&state.status, Some(Notice::Info(message)) if message.contains("Ctrl+S")));

    select(&mut state, &workspace, &["team"])?;
    press(&mut state, &workspace, KeyCode::Char('e'));
    let mut terminal = Terminal::new(TestBackend::new(87, 40))?;
    terminal.draw(|frame| state.render(frame))?;
    let screen = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(!screen.contains("Version"), "{screen}");
    assert!(screen.contains("Types"), "{screen}");
    press(&mut state, &workspace, KeyCode::Esc);

    import_row(&mut state, &workspace, "conventional")?;
    assert!(
        matches!(&state.status, Some(Notice::Info(message)) if message.contains("already has"))
    );
    assert_eq!(workspace.saves(), 0);
    assert!(state.scope_dirty(ConfigurationDestination::Project));
    Ok(())
}

#[test]
fn a_different_project_copy_is_replaced_or_kept_only_on_request() -> Result {
    let (workspace, mut state) = with_library()?;
    open_outline(&mut state, &workspace);
    select(&mut state, &workspace, &["conventional", "fix"])?;
    press(&mut state, &workspace, KeyCode::Char('e'));
    replace(&mut state, &workspace, "repair")?;
    ctrl(&mut state, &workspace, 'w');

    import_row(&mut state, &workspace, "conventional")?;
    let question = state.confirmation().ok_or("no conflict dialog")?;
    assert!(question.message.contains("differs from the Library's"));
    press(&mut state, &workspace, KeyCode::Esc);
    assert!(
        draft(&state)?
            .find_path(&["conventional", "repair"])
            .is_some()
    );

    import_row(&mut state, &workspace, "conventional")?;
    press(&mut state, &workspace, KeyCode::Char('c'));
    assert!(
        draft(&state)?
            .find_path(&["conventional-copy", "fix"])
            .is_some()
    );
    assert!(
        draft(&state)?
            .find_path(&["conventional", "repair"])
            .is_some()
    );

    import_row(&mut state, &workspace, "conventional")?;
    press(&mut state, &workspace, KeyCode::Char('r'));
    assert!(draft(&state)?.find_path(&["conventional", "fix"]).is_some());
    assert!(
        draft(&state)?
            .find_path(&["conventional", "repair"])
            .is_none()
    );
    Ok(())
}

#[test]
fn importing_needs_the_library_but_editing_does_not() -> Result {
    let (workspace, mut state) = fixture()?;
    open_outline(&mut state, &workspace);
    select(&mut state, &workspace, &["conventional"])?;

    press(&mut state, &workspace, KeyCode::Char('i'));

    assert!(matches!(state.screen, Screen::Home));
    assert!(
        matches!(&state.status, Some(Notice::Error(message)) if message.contains("the Library is corrupt"))
    );
    assert_eq!(state.editor(), Some(Editor::Project));
    Ok(())
}

fn library_has(workspace: &Workspace, path: &[&str]) -> bool {
    workspace
        .global
        .borrow()
        .library()
        .and_then(|library| library.find_path(path))
        .is_some()
}

#[test]
fn copying_to_the_library_saves_it_and_never_shadows_a_built_in() -> Result {
    let (workspace, mut state) = with_library()?;
    open_outline(&mut state, &workspace);

    select(&mut state, &workspace, &["conventional"])?;
    press(&mut state, &workspace, KeyCode::Char('l'));
    assert!(
        matches!(&state.status, Some(Notice::Info(message)) if message.contains("already has"))
    );
    assert_eq!(workspace.saves(), 0);

    select(&mut state, &workspace, &["conventional", "fix"])?;
    press(&mut state, &workspace, KeyCode::Char('e'));
    replace(&mut state, &workspace, "repair")?;
    ctrl(&mut state, &workspace, 'w');
    select(&mut state, &workspace, &["conventional"])?;
    press(&mut state, &workspace, KeyCode::Char('l'));

    assert!(library_has(&workspace, &["conventional-copy", "repair"]));
    assert!(!library_has(&workspace, &["conventional", "repair"]));
    assert_eq!(workspace.saves(), 1, "only the Library was written");
    assert!(state.scope_dirty(ConfigurationDestination::Project));
    Ok(())
}

#[test]
fn a_different_library_taxonomy_is_replaced_only_on_request() -> Result {
    let (workspace, mut state) = with_library()?;
    open_outline(&mut state, &workspace);
    import_row(&mut state, &workspace, "team")?;
    let team = draft(&state)?.find_path(&["team"]).ok_or("team")?;
    state.project_taxonomies.select(team);
    let first_type = draft(&state)?
        .node(&draft(&state)?.find_path(&["team"]).ok_or("team")?)
        .and_then(|node| node.taxonomy().types().first().map(|t| t.name().to_owned()))
        .ok_or("team has a type")?;
    select(&mut state, &workspace, &["team", &first_type])?;
    press(&mut state, &workspace, KeyCode::Char('e'));
    replace(&mut state, &workspace, "shipped")?;
    ctrl(&mut state, &workspace, 'w');
    select(&mut state, &workspace, &["team"])?;

    press(&mut state, &workspace, KeyCode::Char('l'));
    let question = state.confirmation().ok_or("no conflict dialog")?;
    assert!(question.message.contains("The Library's \"team\" differs"));
    press(&mut state, &workspace, KeyCode::Esc);
    assert!(!library_has(&workspace, &["team", "shipped"]));
    assert_eq!(workspace.saves(), 0);

    press(&mut state, &workspace, KeyCode::Char('l'));
    press(&mut state, &workspace, KeyCode::Char('r'));
    assert!(library_has(&workspace, &["team", "shipped"]));
    assert_eq!(workspace.saves(), 1);
    Ok(())
}

#[test]
fn copying_to_the_library_needs_the_library() -> Result {
    let (workspace, mut state) = fixture()?;
    open_outline(&mut state, &workspace);
    select(&mut state, &workspace, &["research"])?;

    press(&mut state, &workspace, KeyCode::Char('l'));

    assert!(
        matches!(&state.status, Some(Notice::Error(message)) if message.contains("the Library is corrupt"))
    );
    assert_eq!(workspace.saves(), 0);
    Ok(())
}

#[test]
fn explicit_edit_project_cancel_preserves_prior_staged_edit() -> Result {
    let (workspace, mut state) = fixture()?;
    open_outline(&mut state, &workspace);
    let fix = select(&mut state, &workspace, &["conventional", "fix"])?;
    press(&mut state, &workspace, KeyCode::Char('e'));
    replace(&mut state, &workspace, "repair")?;
    ctrl(&mut state, &workspace, 'w');
    assert!(!state.project_taxonomies.editing());
    select(&mut state, &workspace, &["conventional", "feat"])?;
    let before = format!("{:?}", draft(&state)?);
    press(&mut state, &workspace, KeyCode::Char('e'));
    replace(&mut state, &workspace, "discard this")?;
    press(&mut state, &workspace, KeyCode::Esc);
    assert_eq!(format!("{:?}", draft(&state)?), before);
    assert_eq!(draft(&state)?.text(&fix, DraftField::Name), Some("repair"));
    assert_eq!(workspace.saves(), 0);
    ctrl(&mut state, &workspace, 's');
    assert!(matches!(
        state.screen,
        Screen::Review(ConfigurationDestination::Project)
    ));
    press(&mut state, &workspace, KeyCode::Enter);
    assert_eq!(workspace.saves(), 1);
    Ok(())
}
