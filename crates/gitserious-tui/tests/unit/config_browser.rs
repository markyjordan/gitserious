use std::cell::{Cell, RefCell};

use gitserious_app::{CustomConfiguration, fork_conventional_edits};
use gitserious_core::{TaxonomyId, TemplateId, TypesetId};
use ratatui::crossterm::event::KeyEvent;
use ratatui::{Terminal, backend::TestBackend};

use super::*;

#[derive(Default)]
struct Workspace {
    saves: Cell<usize>,
    fail: Cell<bool>,
    saved: RefCell<CustomConfiguration>,
}

impl ConfigurationWorkspace for Workspace {
    fn load(&self, destination: ConfigurationDestination) -> Result<ConfigurationSession, String> {
        match destination {
            ConfigurationDestination::Global => {
                ConfigurationSession::global(self.saved.borrow().clone())
            }
            ConfigurationDestination::Project => Err("run gitserious init".into()),
        }
    }
    fn save(&self, session: &ConfigurationSession) -> Result<ConfigurationSession, String> {
        if self.fail.get() {
            return Err("concurrent change".into());
        }
        self.saves.set(self.saves.get() + 1);
        *self.saved.borrow_mut() = session.custom().clone();
        ConfigurationSession::global(session.custom().clone())
    }
}

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
fn review_key() -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))
}

fn dirty(state: &mut State) -> Result<(), Box<dyn std::error::Error>> {
    state
        .session
        .as_mut()
        .ok_or("missing session")?
        .stage(fork_conventional_edits(
            &TemplateId::new("copy")?,
            &TaxonomyId::new("copy-taxonomy")?,
            &TypesetId::new("copy-typeset")?,
        ))?;
    Ok(())
}

#[test]
fn browser_and_inspection_do_not_write_and_failed_destination_is_recoverable()
-> Result<(), Box<dyn std::error::Error>> {
    let workspace = Workspace::default();
    let mut state = State::new(&workspace);
    let mut terminal = Terminal::new(TestBackend::new(90, 24))?;
    terminal.draw(|frame| state.render(frame))?;
    let text = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(text.contains("Configure gitserious"));
    assert!(text.contains("GLOBAL"));
    state.event(&key(KeyCode::Enter), &workspace);
    assert!(matches!(state.screen, Screen::Details(_)));
    terminal.draw(|frame| state.render(frame))?;
    state.event(&key(KeyCode::Esc), &workspace);
    state.event(&key(KeyCode::Tab), &workspace);
    assert!(state.status.contains("init"));
    assert_eq!(state.destination, ConfigurationDestination::Global);
    assert_eq!(workspace.saves.get(), 0);
    assert!(state.event(&key(KeyCode::Char('q')), &workspace));
    Ok(())
}

#[test]
fn review_confirmation_is_the_only_write_and_errors_retain_the_draft()
-> Result<(), Box<dyn std::error::Error>> {
    let workspace = Workspace::default();
    let mut state = State::new(&workspace);
    dirty(&mut state)?;
    state.event(&review_key(), &workspace);
    let Screen::Review(text) = &state.screen else {
        return Err("review not opened".into());
    };
    assert!(text.contains("BEFORE\nAbsent"));
    assert!(text.contains("copy-taxonomy"));
    assert_eq!(workspace.saves.get(), 0);
    state.event(&key(KeyCode::Esc), &workspace);
    state.event(&key(KeyCode::Char('q')), &workspace);
    assert!(matches!(state.screen, Screen::Confirm(_)));
    state.event(&key(KeyCode::Char('n')), &workspace);
    assert!(
        state
            .session
            .as_ref()
            .is_some_and(ConfigurationSession::is_dirty)
    );
    state.event(&review_key(), &workspace);
    workspace.fail.set(true);
    state.event(&key(KeyCode::Enter), &workspace);
    assert_eq!(state.status, "concurrent change");
    assert!(
        state
            .session
            .as_ref()
            .is_some_and(ConfigurationSession::is_dirty)
    );
    workspace.fail.set(false);
    state.event(&review_key(), &workspace);
    state.event(&key(KeyCode::Enter), &workspace);
    assert_eq!(workspace.saves.get(), 1);
    assert!(
        !state
            .session
            .as_ref()
            .is_some_and(ConfigurationSession::is_dirty)
    );
    Ok(())
}
