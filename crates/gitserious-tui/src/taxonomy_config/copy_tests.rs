use gitserious_app::{GlobalConfiguration, Policy};
use gitserious_core::{
    CommitTypeDefinition, CommitTypeId, ConditionId, Description, PropertyCondition,
    PropertyDefinition, PropertyKey, PropertyRequirement, SchemaVersion, TaxonomyVersion,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::tests::Workspace;
use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn team() -> Result<Workspace, Box<dyn std::error::Error>> {
    let property = |key: &str, requirement| -> Result<_, Box<dyn std::error::Error>> {
        Ok(PropertyDefinition::new(
            PropertyKey::new(key)?,
            format!("The {key}."),
            requirement,
        )?)
    };
    let commit_type = |id: &str, properties| -> Result<_, Box<dyn std::error::Error>> {
        Ok(CommitTypeDefinition::new(
            SchemaVersion::new(1)?,
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
        TaxonomyVersion::new(1)?,
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
    *workspace.global.borrow_mut() = ConfigurationSession::open_global(GlobalConfiguration::new(
        1,
        conventional.clone(),
        vec![conventional],
        vec![team],
    )?);
    Ok(workspace)
}

fn press(state: &mut State, workspace: &Workspace, keys: &[KeyCode]) {
    for &code in keys {
        state.handle_event(
            Event::Key(KeyEvent::new(code, KeyModifiers::NONE)),
            workspace,
        );
    }
}

fn open_constraints(state: &mut State, workspace: &Workspace) -> TestResult {
    press(state, workspace, &[KeyCode::Char('2')]);
    let team = state
        .library()
        .and_then(|library| library.find_path(&["team"]))
        .ok_or("team loads")?;
    state.taxonomies.select(team);
    press(
        state,
        workspace,
        &[
            KeyCode::Right,
            KeyCode::Right,
            KeyCode::Right,
            KeyCode::Down,
            KeyCode::Down,
        ],
    );
    assert_eq!(cursor(state), ["team", "feat", "constraints"]);
    Ok(())
}

fn cursor(state: &State) -> Vec<String> {
    state.taxonomies.observe(state.library()).cursor_path
}

fn screen(state: &mut State) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut terminal = Terminal::new(TestBackend::new(87, 40))?;
    terminal.draw(|frame| state.render(frame))?;
    let buffer = terminal.backend().buffer();
    Ok(buffer
        .content
        .chunks(usize::from(buffer.area.width))
        .map(|row| row.iter().map(ratatui::buffer::Cell::symbol).collect())
        .collect())
}

fn notice(state: &State) -> Option<&str> {
    match &state.status {
        Some(Notice::Error(message) | Notice::Info(message)) => Some(message),
        None => None,
    }
}

fn policy(state: &State, path: &[&str]) -> Option<Policy> {
    let library = state.library()?;
    match library.node(&library.find_path(path)?)? {
        NodeRef::Property { property, .. } => Some(property.policy()),
        _ => None,
    }
}

#[test]
fn c_copies_a_property_to_the_chosen_types_and_each_copy_changes_apart() -> TestResult {
    let workspace = team()?;
    let mut state = State::new(&workspace);
    open_constraints(&mut state, &workspace)?;
    let rows = screen(&mut state)?;
    assert!(
        rows.iter()
            .any(|row| row.contains("c copy to other types…   d delete"))
    );
    assert!(rows[39].contains("c copy to…  ←/→ policy"), "{}", rows[39]);

    press(&mut state, &workspace, &[KeyCode::Char('c')]);
    assert_eq!(state.taxonomies.shell_keys(), ShellKeys::None);
    let rows = screen(&mut state)?.join("\n");
    assert!(
        rows.contains("Copy \"constraints\" from feat to:"),
        "{rows}"
    );
    assert!(
        rows.contains("[ ] fix") && rows.contains("[ ] docs"),
        "{rows}"
    );
    let footer = rows.lines().last().unwrap_or_default();
    assert!(
        footer.starts_with("↑/↓: move  space: toggle  enter: copy  esc: cancel"),
        "{footer}"
    );
    press(&mut state, &workspace, &[KeyCode::Enter]);
    assert_eq!(notice(&state), Some("Choose at least one type with space."));
    press(
        &mut state,
        &workspace,
        &[KeyCode::Char(' '), KeyCode::Down, KeyCode::Char(' ')],
    );
    assert!(screen(&mut state)?.join("\n").contains("[x] docs"));
    press(&mut state, &workspace, &[KeyCode::Enter]);

    assert_eq!(cursor(&state), ["team", "feat", "constraints"]);
    assert_eq!(state.taxonomies.shell_keys(), ShellKeys::Navigation);
    assert_eq!(state.pending_count(), 2);
    let rows = screen(&mut state)?;
    let fix = rows
        .iter()
        .position(|row| row.contains("▾ fix"))
        .ok_or("fix is expanded")?;
    assert!(
        rows[fix + 2].contains("constraints  conditional"),
        "{}",
        rows[fix + 2]
    );
    assert!(rows[fix + 2].contains("+ new"), "{}", rows[fix + 2]);
    assert!(rows.iter().any(|row| row.contains("▾ docs")));

    press(&mut state, &workspace, &[KeyCode::Down; 4]);
    assert_eq!(cursor(&state), ["team", "fix", "constraints"]);
    let rows = screen(&mut state)?.join("\n");
    assert!(rows.contains("in team › fix  · copied from feat"), "{rows}");
    press(&mut state, &workspace, &[KeyCode::Right]);
    assert_eq!(
        policy(&state, &["team", "fix", "constraints"]),
        Some(Policy::Optional)
    );
    assert_eq!(
        policy(&state, &["team", "feat", "constraints"]),
        Some(Policy::Conditional)
    );
    assert_eq!(
        policy(&state, &["team", "docs", "constraints"]),
        Some(Policy::Conditional)
    );
    assert_eq!(workspace.saves(), 0);
    Ok(())
}

#[test]
fn c_refuses_a_built_in_a_lone_type_and_a_target_that_has_the_key_with_a_sentence() -> TestResult {
    let workspace = team()?;
    let mut state = State::new(&workspace);
    open_constraints(&mut state, &workspace)?;
    press(
        &mut state,
        &workspace,
        &[KeyCode::Char('c'), KeyCode::Char(' '), KeyCode::Enter],
    );
    let before = format!("{:?}", state.library());
    press(
        &mut state,
        &workspace,
        &[KeyCode::Char('c'), KeyCode::Char(' '), KeyCode::Enter],
    );
    assert_eq!(
        notice(&state),
        Some("Type \"fix\" already has a property \"constraints\"; nothing was copied.")
    );
    assert_eq!(format!("{:?}", state.library()), before);
    assert_eq!(state.taxonomies.shell_keys(), ShellKeys::None);
    press(&mut state, &workspace, &[KeyCode::Esc]);
    assert_eq!(state.taxonomies.shell_keys(), ShellKeys::Navigation);
    assert_eq!(format!("{:?}", state.library()), before);

    let conventional = state
        .library()
        .and_then(|library| library.find_path(&["conventional"]))
        .ok_or("conventional loads")?;
    state.taxonomies.select(conventional);
    press(
        &mut state,
        &workspace,
        &[
            KeyCode::Right,
            KeyCode::Right,
            KeyCode::Right,
            KeyCode::Down,
        ],
    );
    assert_eq!(cursor(&state), ["conventional", "feat", "intent"]);
    press(&mut state, &workspace, &[KeyCode::Char('c')]);
    assert_eq!(
        notice(&state),
        Some("Built-in taxonomies are read-only; fork one to customize it.")
    );
    assert_eq!(state.taxonomies.shell_keys(), ShellKeys::Navigation);

    let library = state
        .global
        .as_mut()
        .and_then(ConfigurationSession::library_mut)
        .ok_or("the Library loads")?;
    for path in [["team", "fix"], ["team", "docs"]] {
        let commit_type = library.find_path(&path).ok_or("the type exists")?;
        library.edit(&commit_type, DraftEdit::Remove)?;
    }
    let intent = library
        .find_path(&["team", "feat", "intent"])
        .ok_or("feat has intent")?;
    state.taxonomies.select(intent);
    press(&mut state, &workspace, &[KeyCode::Char('c')]);
    assert_eq!(
        notice(&state),
        Some("Taxonomy \"team\" has no other type to copy to.")
    );
    assert_eq!(state.taxonomies.shell_keys(), ShellKeys::Navigation);
    Ok(())
}

#[test]
fn the_copy_picker_keeps_its_choices_when_the_user_clicks_behind_it() -> TestResult {
    let workspace = team()?;
    let mut state = State::new(&workspace);
    open_constraints(&mut state, &workspace)?;
    press(
        &mut state,
        &workspace,
        &[KeyCode::Char('c'), KeyCode::Char(' ')],
    );
    screen(&mut state)?;
    let mode = state.taxonomies.observe(state.library()).mode;
    let project = state.tab_areas[Category::Project.index()];
    for (column, row) in [(project.x + 1, project.y), (20, 2)] {
        state.handle_event(
            Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                modifiers: KeyModifiers::NONE,
            }),
            &workspace,
        );
        assert_eq!(state.category, Category::Taxonomies);
        assert_eq!(state.taxonomies.observe(state.library()).mode, mode);
        assert_eq!(cursor(&state), ["team", "feat", "constraints"]);
    }
    press(&mut state, &workspace, &[KeyCode::Enter]);
    assert_eq!(state.pending_count(), 1);
    assert_eq!(
        policy(&state, &["team", "fix", "constraints"]),
        Some(Policy::Conditional)
    );
    Ok(())
}
