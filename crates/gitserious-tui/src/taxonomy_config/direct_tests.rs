use crate::library_lookup::LibraryLookup;
use std::cell::Cell;

use gitserious_app::{DraftField, GlobalConfiguration};
use ratatui::layout::Position;
use ratatui::{Terminal, backend::TestBackend};

use super::taxonomies::{Mode, RowKey};
use super::tests::Workspace;
use super::*;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Store {
    inner: Workspace,
    fail: Cell<bool>,
}

impl ConfigurationWorkspace for Store {
    fn load(
        &self,
        destination: ConfigurationDestination,
    ) -> std::result::Result<ConfigurationSession, String> {
        self.inner.load(destination)
    }
    fn save(
        &self,
        session: &ConfigurationSession,
    ) -> std::result::Result<ConfigurationSession, String> {
        if self.fail.get() {
            return Err("could not replace /config/config.toml: disk full".into());
        }
        self.inner.save(session)
    }
    fn library_path(&self) -> std::path::PathBuf {
        self.inner.library_path()
    }
}

fn fixture() -> Result<(Store, State)> {
    let store = Store {
        inner: super::copy_tests::team()?,
        fail: Cell::new(false),
    };
    let mut state = State::new(&store);
    press(&mut state, &store, KeyCode::Char('2'));
    Ok((store, state))
}

fn press(state: &mut State, store: &dyn ConfigurationWorkspace, code: KeyCode) {
    assert!(!state.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)), store));
}

fn select(state: &mut State, path: &[&str]) -> Result<NodeId> {
    for depth in 1..path.len() {
        let parent = state
            .library()
            .and_then(|library| library.find_path(&path[..depth]))
            .ok_or("parent missing")?;
        state.taxonomies.select(parent.clone());
        if !state
            .taxonomies
            .observe(state.library())
            .expanded
            .contains(&parent)
        {
            state.taxonomies.key(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                state
                    .global
                    .as_ref()
                    .and_then(ConfigurationSession::library)
                    .ok_or("Library missing")?,
            );
        }
    }
    let id = state
        .library()
        .and_then(|library| library.find_path(path))
        .ok_or("node missing")?;
    state.taxonomies.select(id.clone());
    Ok(id)
}

fn add_row(state: &mut State, store: &dyn ConfigurationWorkspace, row: RowKey) -> Result {
    let parent = match &row {
        RowKey::AddType(parent) | RowKey::AddProperty(parent) => parent,
        RowKey::Node(_) | RowKey::NewTaxonomy => return Err("expected add row".into()),
    };
    state.taxonomies.select(parent.clone());
    if !state
        .taxonomies
        .observe(state.library())
        .expanded
        .contains(parent)
    {
        press(state, store, KeyCode::Enter);
    }
    for _ in 0..50 {
        if state.taxonomies.observe(state.library()).cursor == Some(row.clone()) {
            return Ok(());
        }
        press(state, store, KeyCode::Down);
    }
    Err("add row missing".into())
}

fn paste(state: &mut State, store: &dyn ConfigurationWorkspace, text: &str) {
    assert!(!state.handle_event(Event::Paste(text.into()), store));
}

fn replace(state: &mut State, store: &dyn ConfigurationWorkspace, text: &str) -> Result {
    let observation = state.taxonomies.observe(state.library());
    let Mode::Edit { node, field, .. } = &observation.mode else {
        return Err("not editing".into());
    };
    let length = state
        .library()
        .and_then(|library| library.text(node, *field))
        .ok_or("field missing")?
        .chars()
        .count();
    press(state, store, KeyCode::Home);
    for _ in 0..length {
        press(state, store, KeyCode::Delete);
    }
    paste(state, store, text);
    Ok(())
}

fn stored(store: &Store) -> Result<GlobalConfiguration> {
    Ok(store
        .inner
        .global
        .borrow()
        .library()
        .and_then(LibraryDraft::persisted)
        .ok_or("stored Library missing")?
        .clone())
}

fn frame(state: &mut State, width: u16, height: u16) -> Result<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    terminal.draw(|frame| state.render(frame))?;
    let buffer = terminal.backend().buffer();
    Ok(buffer
        .content
        .chunks(usize::from(width))
        .map(|row| {
            row.iter()
                .map(ratatui::buffer::Cell::symbol)
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

fn validation_error(state: &State) -> Result<String> {
    let observation = state.taxonomies.observe(state.library());
    let Mode::Edit { node, field, .. } = observation.mode else {
        return Err("not editing".into());
    };
    let library = state.library().ok_or("Library")?;
    let problems = library.field_problems(&node);
    let problem = problems
        .iter()
        .find(|problem| problem.site == gitserious_app::ProblemSite::Field(node.clone(), field))
        .ok_or("field error missing")?;
    Ok(naming::problem_sentence(library, problem))
}

fn error(state: &State) -> Result<&str> {
    match &state.status {
        Some(Notice::Error(error)) => Ok(error),
        _ => Err("expected error".into()),
    }
}

#[test]
fn blank_creation_cancels_only_at_its_initial_values() -> Result {
    let (store, mut state) = fixture()?;
    let original = stored(&store)?;
    press(&mut state, &store, KeyCode::Char('n'));
    paste(&mut state, &store, "x");
    press(&mut state, &store, KeyCode::Backspace);
    press(&mut state, &store, KeyCode::Esc);
    assert!(state.taxonomies.navigating());
    assert_eq!(stored(&store)?, original);
    assert_eq!(store.inner.saves(), 0);

    let taxonomy = select(&mut state, &["team"])?;
    add_row(&mut state, &store, RowKey::AddType(taxonomy))?;
    press(&mut state, &store, KeyCode::Enter);
    press(&mut state, &store, KeyCode::Esc);
    assert!(state.taxonomies.navigating());
    assert_eq!(store.inner.saves(), 0);

    let commit_type = select(&mut state, &["team", "feat"])?;
    add_row(&mut state, &store, RowKey::AddProperty(commit_type))?;
    press(&mut state, &store, KeyCode::Enter);
    press(&mut state, &store, KeyCode::Enter);
    press(&mut state, &store, KeyCode::Enter);
    press(&mut state, &store, KeyCode::Right);
    press(&mut state, &store, KeyCode::Esc);
    assert!(matches!(
        state.taxonomies.observe(state.library()).mode,
        Mode::Edit {
            field: DraftField::Name,
            ..
        }
    ));
    assert!(validation_error(&state)?.contains("Property name is required"));
    assert_eq!(stored(&store)?, original);
    Ok(())
}

#[test]
fn partial_fields_and_duplicate_names_stay_in_edit_with_inline_errors() -> Result {
    let (store, mut state) = fixture()?;
    select(&mut state, &["team", "feat"])?;
    press(&mut state, &store, KeyCode::Char('e'));
    replace(&mut state, &store, "FIX")?;
    press(&mut state, &store, KeyCode::Esc);
    assert!(matches!(
        state.taxonomies.observe(state.library()).mode,
        Mode::Edit {
            field: DraftField::Name,
            ..
        }
    ));
    assert!(validation_error(&state)?.contains("already exists"));
    assert!(frame(&mut state, 87, 40)?.contains("named \"fix\" already exists"));
    assert_eq!(store.inner.saves(), 0);
    replace(&mut state, &store, "New kind")?;
    press(&mut state, &store, KeyCode::Enter);
    replace(&mut state, &store, " ")?;
    press(&mut state, &store, KeyCode::Esc);
    assert!(matches!(
        state.taxonomies.observe(state.library()).mode,
        Mode::Edit {
            field: DraftField::Description,
            ..
        }
    ));
    assert!(frame(&mut state, 87, 40)?.contains("Type description is required."));
    assert_eq!(store.inner.saves(), 0);
    paste(&mut state, &store, " New kind purpose ");
    let before = state.taxonomies.observe(state.library());
    press(&mut state, &store, KeyCode::Esc);
    let after = state.taxonomies.observe(state.library());
    assert!(state.taxonomies.navigating());
    assert_eq!(before.expanded, after.expanded);
    assert_eq!(before.outline_offset, after.outline_offset);
    assert_eq!(after.cursor_path, ["team", "new-kind"]);
    let saved = stored(&store)?;
    assert_eq!(saved.custom_taxonomies()[0].version().get(), 2);
    assert_eq!(
        saved.custom_taxonomies()[0].commit_types()[0].description(),
        "New kind purpose"
    );
    press(&mut state, &store, KeyCode::Char('e'));
    press(&mut state, &store, KeyCode::Esc);
    assert_eq!(store.inner.saves(), 1);
    assert_eq!(stored(&store)?, saved);
    Ok(())
}

#[test]
fn zero_type_entry_reopens_unlocks_until_eligible_and_stays_locked_after_last_type_removed()
-> Result {
    let (store, mut state) = fixture()?;
    press(&mut state, &store, KeyCode::Char('n'));
    paste(&mut state, &store, "Work in progress");
    press(&mut state, &store, KeyCode::Enter);
    paste(&mut state, &store, "Purpose");
    press(&mut state, &store, KeyCode::Esc);
    let saved = stored(&store)?;
    let record = saved
        .custom_taxonomies()
        .last()
        .ok_or("new record missing")?;
    assert_eq!(record.version().get(), 1);
    assert!(!record.name_locked());
    assert!(record.usable().is_none());
    assert!(frame(&mut state, 87, 40)?.contains("▲ invalid"));
    for embedded in [false, true] {
        state.embedded = embedded;
        for width in [MINIMUM_WIDTH, 87, 140] {
            let mut terminal = Terminal::new(TestBackend::new(width, 40))?;
            terminal.draw(|frame| state.render(frame))?;
            let outline = state.taxonomies.observe(state.library()).outline_area;
            let buffer = terminal.backend().buffer();
            let mut count = 0;
            for y in 0..40 {
                let row = (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>();
                if let Some(start) = row.find("▲ invalid") {
                    let x = u16::try_from(row[..start].chars().count())?;
                    if outline.contains(Position::new(x, y)) {
                        // The outline keeps the origin beside the status, on the focused cursor.
                        let entry = row
                            .chars()
                            .skip(usize::from(outline.x))
                            .take(usize::from(outline.width))
                            .collect::<String>();
                        assert!(entry.contains(" work-in-progress "), "{row}");
                        assert!(entry.contains(" custom "), "{row}");
                        let cell = &buffer[(x, y)];
                        assert_eq!((cell.fg, cell.bg), (theme().ground, theme().cursor));
                    } else {
                        // The Inspector's badge is plain red text, right-aligned on its heading.
                        assert!(row.trim_end().ends_with("▲ invalid"), "{row}");
                        for dx in 0.."▲ invalid".chars().count() {
                            let cell = &buffer[(x + u16::try_from(dx)?, y)];
                            assert_eq!((cell.fg, cell.bg), (theme().blocks, JET_BLACK));
                        }
                    }
                    count += 1;
                }
            }
            assert_eq!(count, 2);
        }
    }
    let mut state = State::new(&store);
    press(&mut state, &store, KeyCode::Char('2'));
    let taxonomy = select(&mut state, &["work-in-progress"])?;
    press(&mut state, &store, KeyCode::Char('e'));
    replace(&mut state, &store, "Final name")?;
    press(&mut state, &store, KeyCode::Esc);
    add_row(&mut state, &store, RowKey::AddType(taxonomy.clone()))?;
    press(&mut state, &store, KeyCode::Enter);
    paste(&mut state, &store, "Change");
    press(&mut state, &store, KeyCode::Enter);
    paste(&mut state, &store, "A change");
    let rows = frame(&mut state, 87, 40)?;
    assert!(
        rows.contains("▲ invalid"),
        "unsaved child changed saved eligibility"
    );
    press(&mut state, &store, KeyCode::Esc);
    let saved = stored(&store)?;
    let record = saved
        .custom_taxonomies()
        .last()
        .ok_or("new record missing")?;
    assert!(record.name_locked());
    assert!(
        saved
            .catalog()?
            .find(&TaxonomyId::new("final-name")?)
            .is_some()
    );
    assert_eq!(record.version().get(), 3);
    assert!(!frame(&mut state, 87, 40)?.contains("▲ invalid"));
    press(&mut state, &store, KeyCode::Char('d'));
    press(&mut state, &store, KeyCode::Char('y'));
    let saved = stored(&store)?;
    let record = saved
        .custom_taxonomies()
        .last()
        .ok_or("new record missing")?;
    assert!(record.name_locked());
    assert!(record.commit_types().is_empty());
    assert_eq!(record.version().get(), 4);
    assert!(
        !state
            .library()
            .ok_or("Library")?
            .fields(&taxonomy)
            .contains(&DraftField::Name)
    );
    Ok(())
}

#[test]
fn conditional_fields_are_required_and_type_and_property_never_show_invalid_chips() -> Result {
    let (store, mut state) = fixture()?;
    select(&mut state, &["team", "feat", "intent"])?;
    press(&mut state, &store, KeyCode::Char('e'));
    press(&mut state, &store, KeyCode::Enter);
    press(&mut state, &store, KeyCode::Enter);
    press(&mut state, &store, KeyCode::Right);
    press(&mut state, &store, KeyCode::Right);
    press(&mut state, &store, KeyCode::Esc);
    assert!(matches!(
        state.taxonomies.observe(state.library()).mode,
        Mode::Edit {
            field: DraftField::Condition,
            ..
        }
    ));
    assert!(validation_error(&state)?.contains("needs a condition"));
    paste(&mut state, &store, "Risky change");
    press(&mut state, &store, KeyCode::Esc);
    assert!(matches!(
        state.taxonomies.observe(state.library()).mode,
        Mode::Edit {
            field: DraftField::Rationale,
            ..
        }
    ));
    for (width, height) in [(66, 18), (87, 40), (140, 48)] {
        let text = frame(&mut state, width, height)?;
        assert!(!text.contains("▲ invalid"));
        assert_eq!(
            text.matches("+ new").count(),
            text.matches("+ new taxonomy").count()
        );
        assert!(!text.contains("~ modified"));
        assert!(super::tests::footer_text(&text).contains("esc save"));
        assert!(!text.contains("ctrl+s"));
    }
    paste(&mut state, &store, "Explain risky changes");
    press(&mut state, &store, KeyCode::Esc);
    let saved = stored(&store)?;
    assert_eq!(saved.custom_taxonomies()[0].version().get(), 2);
    assert_eq!(store.inner.saves(), 1);
    Ok(())
}

#[test]
fn failed_edit_keeps_fields_and_retry_saves_once_without_changing_identity() -> Result {
    let (store, mut state) = fixture()?;
    let id = select(&mut state, &["team", "feat", "intent"])?;
    press(&mut state, &store, KeyCode::Char('e'));
    replace(&mut state, &store, "new intent")?;
    let original = stored(&store)?;
    store.fail.set(true);
    press(&mut state, &store, KeyCode::Esc);
    assert!(state.taxonomies.editing());
    assert_eq!(
        state
            .library()
            .and_then(|library| library.text(&id, DraftField::Name)),
        Some("new intent")
    );
    assert_eq!(stored(&store)?, original);
    assert!(error(&state)?.contains("/config/config.toml: disk full"));
    store.fail.set(false);
    press(&mut state, &store, KeyCode::Esc);
    assert_eq!(
        state
            .library()
            .and_then(|library| library.find_path(&["team", "feat", "new-intent"])),
        Some(id)
    );
    assert_eq!(store.inner.saves(), 1);
    Ok(())
}

#[test]
fn copy_reorder_and_delete_rollback_on_failure_and_persist_on_retry() -> Result {
    for operation in ["copy", "reorder", "delete"] {
        let (store, mut state) = fixture()?;
        select(&mut state, &["team", "feat", "intent"])?;
        let original = stored(&store)?;
        let event = match operation {
            "copy" => {
                press(&mut state, &store, KeyCode::Char('c'));
                press(&mut state, &store, KeyCode::Char(' '));
                Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            }
            "reorder" => Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT)),
            _ => {
                press(&mut state, &store, KeyCode::Char('d'));
                Event::Key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE))
            }
        };
        let before_view = format!("{:?}", state.taxonomies);
        let before_screen = format!("{:?}", state.screen);
        let before_library = format!("{:?}", state.library());
        store.fail.set(true);
        assert!(!state.handle_event(event.clone(), &store));
        assert_eq!(
            format!("{:?}", state.taxonomies),
            before_view,
            "{operation}"
        );
        assert_eq!(format!("{:?}", state.screen), before_screen, "{operation}");
        assert_eq!(
            format!("{:?}", state.library()),
            before_library,
            "{operation}"
        );
        assert_eq!(stored(&store)?, original, "{operation}");
        assert!(error(&state)?.contains("disk full"));
        store.fail.set(false);
        assert!(!state.handle_event(event, &store));
        assert_ne!(stored(&store)?, original, "{operation}");
        assert!(!state.scope_dirty(ConfigurationDestination::Global));
        assert_eq!(stored(&store)?.custom_taxonomies()[0].version().get(), 2);
    }
    Ok(())
}

#[test]
fn stale_library_cannot_overwrite_another_session() -> Result {
    let (store, mut first) = fixture()?;
    let mut second = State::new(&store);
    press(&mut second, &store, KeyCode::Char('2'));
    for state in [&mut first, &mut second] {
        select(state, &["team"])?;
        press(state, &store, KeyCode::Char('e'));
        paste(state, &store, " edited");
    }
    press(&mut first, &store, KeyCode::Esc);
    let current = stored(&store)?;
    press(&mut second, &store, KeyCode::Esc);
    assert!(second.taxonomies.editing());
    assert!(error(&second)?.contains("changed while editing"));
    assert_eq!(stored(&store)?, current);
    Ok(())
}

#[test]
fn editing_blocks_navigation_clicks_review_shortcut_and_normal_quit() -> Result {
    let (store, mut state) = fixture()?;
    press(&mut state, &store, KeyCode::Char('n'));
    paste(&mut state, &store, "Partial");
    frame(&mut state, 87, 40)?;
    let menu = state.tab_areas[Category::Project.index()];
    state.handle_event(
        Event::Mouse(ratatui::crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: menu.x,
            row: menu.y,
            modifiers: KeyModifiers::NONE,
        }),
        &store,
    );
    for code in [KeyCode::Tab, KeyCode::BackTab] {
        press(&mut state, &store, code);
    }
    state.handle_event(
        Event::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)),
        &store,
    );
    assert_eq!(state.category, Category::Taxonomies);
    assert!(state.taxonomies.editing());
    assert!(matches!(state.screen, Screen::Home));
    assert_eq!(store.inner.saves(), 0);
    press(&mut state, &store, KeyCode::Char('q'));
    press(&mut state, &store, KeyCode::Esc);
    assert!(state.taxonomies.editing());
    assert!(matches!(state.screen, Screen::Home));
    Ok(())
}
