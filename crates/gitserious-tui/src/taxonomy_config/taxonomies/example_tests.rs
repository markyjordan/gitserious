use gitserious_app::{
    ConfigurationDestination, ConfigurationSession, ConfigurationWorkspace, GlobalConfiguration,
};
use ratatui::crossterm::event::Event;
use ratatui::{Terminal, backend::TestBackend};
use std::cell::RefCell;

use super::super::{Category, HomeFocus, State, tests::Workspace};
use super::*;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn key(
    state: &mut State,
    workspace: &dyn ConfigurationWorkspace,
    code: KeyCode,
    modifiers: KeyModifiers,
) {
    state.handle_event(Event::Key(KeyEvent::new(code, modifiers)), workspace);
}

fn field(state: &State) -> Result<DraftField> {
    match state.taxonomies.mode {
        Mode::Edit { field, .. } => Ok(field),
        _ => Err("EDIT".into()),
    }
}

struct PersistedWorkspace {
    global: RefCell<GlobalConfiguration>,
    project: Workspace,
}

impl ConfigurationWorkspace for PersistedWorkspace {
    fn load(
        &self,
        destination: ConfigurationDestination,
    ) -> std::result::Result<ConfigurationSession, String> {
        match destination {
            ConfigurationDestination::Global => Ok(ConfigurationSession::open_global(
                self.global.borrow().clone(),
            )),
            ConfigurationDestination::Project => self.project.load(destination),
        }
    }

    fn save(
        &self,
        session: &ConfigurationSession,
    ) -> std::result::Result<ConfigurationSession, String> {
        let saved = session
            .library()
            .ok_or("global library")?
            .build()
            .map_err(|problems| format!("{problems:?}"))?;
        *self.global.borrow_mut() = saved.clone();
        Ok(ConfigurationSession::open_global(saved))
    }
}

#[test]
fn inspector_edits_a_multiline_example_and_save_reopen_preserves_it() -> Result {
    let workspace = PersistedWorkspace {
        global: RefCell::new(GlobalConfiguration::default()),
        project: Workspace::new(),
    };
    let mut state = State::new(&workspace);
    state.category = Category::Taxonomies;
    state.home_focus = HomeFocus::Content;
    state.active_scope = ConfigurationDestination::Global;
    let library = state
        .global
        .as_mut()
        .and_then(ConfigurationSession::library_mut)
        .ok_or("library")?;
    let source = library.find_path(&["conventional"]).ok_or("source")?;
    library.fork(&source)?;
    let commit_type = library
        .find_path(&["conventional-fork", "feat"])
        .ok_or("type")?;
    let property = library.edit(&commit_type, DraftEdit::AddProperty)?;
    library.edit(&property, DraftEdit::Text(DraftField::Name, "verification"))?;
    library.edit(
        &property,
        DraftEdit::Text(DraftField::Description, "How this was checked."),
    )?;
    let snapshot = library.clone();
    state
        .taxonomies
        .open(property.clone(), DraftField::Name, &snapshot);
    let mut terminal = Terminal::new(TestBackend::new(66, 18))?;
    for _ in 0..3 {
        key(&mut state, &workspace, KeyCode::Enter, KeyModifiers::NONE);
    }
    assert_eq!(field(&state)?, DraftField::Example);
    let seed = "feat([<scope>]): <subject>\nverification:\n<value>";
    assert_eq!(
        state
            .library()
            .ok_or("library")?
            .text(&property, DraftField::Example),
        Some(seed)
    );
    terminal.draw(|frame| state.render(frame))?;
    for _ in 0..seed.graphemes(true).count() {
        key(
            &mut state,
            &workspace,
            KeyCode::Backspace,
            KeyModifiers::NONE,
        );
    }
    let body =
        "feat(db): ship café\r\nverification:\r\n  Checked 👩‍💻界e\u{301}\r\n\r\nStaging passed.\r\n";
    state.handle_event(Event::Paste(body.into()), &workspace);
    let normalized = body.replace("\r\n", "\n");
    assert_eq!(
        state
            .library()
            .ok_or("library")?
            .preview(&property)
            .ok_or("preview")?
            .message,
        normalized
    );
    key(&mut state, &workspace, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(field(&state)?, DraftField::Example);
    let expected = normalized + "\n";
    assert_eq!(
        state
            .library()
            .ok_or("library")?
            .text(&property, DraftField::Example),
        Some(expected.as_str())
    );
    for (width, height) in [(66, 18), (87, 40), (66, 22), (66, 18)] {
        terminal.backend_mut().resize(width, height);
        terminal.resize(Rect::new(0, 0, width, height))?;
        terminal.draw(|frame| state.render(frame))?;
        let buffer = terminal.backend().buffer();
        let cursors = buffer
            .area
            .positions()
            .filter(|position| buffer[*position].modifier.contains(Modifier::REVERSED))
            .collect::<Vec<_>>();
        assert_eq!(cursors.len(), 1);
        assert!(cursors[0].y < height - crate::theme::FOOTER_HEIGHT);
        assert!(cursors[0].y > state.taxonomies.outline_area.bottom());
        key(&mut state, &workspace, KeyCode::Up, KeyModifiers::NONE);
        key(&mut state, &workspace, KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(field(&state)?, DraftField::Example);
    }
    key(&mut state, &workspace, KeyCode::Enter, KeyModifiers::ALT);
    assert_eq!(field(&state)?, DraftField::Name);
    key(&mut state, &workspace, KeyCode::Esc, KeyModifiers::NONE);
    key(
        &mut state,
        &workspace,
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
    );
    key(&mut state, &workspace, KeyCode::Enter, KeyModifiers::NONE);
    let reopened = State::new(&workspace);
    let library = reopened.library().ok_or("reopened")?;
    let property = library
        .find_path(&["conventional-fork", "feat", "verification"])
        .ok_or("saved property")?;
    assert_eq!(
        library.text(&property, DraftField::Example),
        Some(expected.as_str())
    );
    assert_eq!(library.changes().count(), 0);
    Ok(())
}

#[test]
fn long_example_cursor_and_grapheme_deletion_stay_inside_the_inspector() -> Result {
    let mut library =
        LibraryDraft::from_configuration(gitserious_app::GlobalConfiguration::default());
    let source = library.find_path(&["conventional"]).ok_or("source")?;
    library.fork(&source)?;
    let property = library
        .find_path(&["conventional-fork", "feat", "intent"])
        .ok_or("intent")?;
    let message = format!(
        "feat(db): ship\nintent:\n{}👩‍💻",
        "\nlong explanatory line with wide 界 characters".repeat(30)
    );
    library.edit(&property, DraftEdit::Text(DraftField::Example, &message))?;
    let mut view = TaxonomiesView::default();
    view.open(property.clone(), DraftField::Example, &library);
    let mut terminal = Terminal::new(TestBackend::new(51, 12))?;
    terminal.draw(|frame| view.render(frame, frame.area(), Some(&library), true, false))?;
    assert!(view.inspector_offset > 0);
    view.edit_key(
        KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        &mut library,
    )?;
    assert_eq!(
        library.text(&property, DraftField::Example),
        message.strip_suffix("👩‍💻")
    );
    view.edit_key(
        KeyEvent::new(KeyCode::Home, KeyModifiers::CONTROL),
        &mut library,
    )?;
    terminal.draw(|frame| view.render(frame, frame.area(), Some(&library), true, false))?;
    let buffer = terminal.backend().buffer();
    let row = buffer
        .area
        .positions()
        .find(|position| buffer[*position].modifier.contains(Modifier::REVERSED))
        .ok_or("cursor")?;
    assert_eq!(buffer[row].symbol(), "f");
    assert!(usize::from(row.y) < usize::from(buffer.area.height));
    Ok(())
}
