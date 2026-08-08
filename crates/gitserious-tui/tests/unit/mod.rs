use std::error::Error;

use gitserious_app::{CommitTypeSelection, CommitTypeSelector};
use gitserious_core::built_in_commit_types;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{CommitTypeSelectorError, PickerState, RatatuiCommitTypeSelector, handle_key, render};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn buffer_text(backend: &TestBackend) -> String {
    backend
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect()
}

#[test]
fn navigation_wraps_and_home_end_select_boundaries() {
    let length = built_in_commit_types().len();
    let mut state = PickerState::default();
    state.previous(length);
    assert_eq!(state.selected, length - 1);
    state.next(length);
    assert_eq!(state.selected, 0);
    state.next(length);
    assert_eq!(state.selected, 1);
    state.last(length);
    assert_eq!(state.selected, length - 1);
    state.first();
    assert_eq!(state.selected, 0);
}

#[test]
fn every_navigation_key_updates_the_selected_row() {
    let definitions = built_in_commit_types();
    let mut state = PickerState::default();
    for code in [KeyCode::Down, KeyCode::Char('j')] {
        assert!(handle_key(key(code), definitions, &mut state).is_none());
    }
    assert_eq!(state.selected, 2);
    for code in [KeyCode::Up, KeyCode::Char('k')] {
        assert!(handle_key(key(code), definitions, &mut state).is_none());
    }
    assert_eq!(state.selected, 0);
    state.selected = 4;
    assert!(handle_key(key(KeyCode::Home), definitions, &mut state).is_none());
    assert_eq!(state.selected, 0);
    assert!(handle_key(key(KeyCode::End), definitions, &mut state).is_none());
    assert_eq!(state.selected, definitions.len() - 1);
}

#[test]
fn enter_returns_the_highlighted_open_identifier() {
    let definitions = built_in_commit_types();
    let mut state = PickerState { selected: 1 };
    assert_eq!(
        handle_key(key(KeyCode::Enter), definitions, &mut state),
        Some(CommitTypeSelection::Selected(definitions[1].id().clone()))
    );
}

#[test]
fn escape_q_and_control_c_cancel_while_other_keys_are_ignored() {
    let definitions = built_in_commit_types();
    for event in [
        key(KeyCode::Esc),
        key(KeyCode::Char('q')),
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
    ] {
        let mut state = PickerState::default();
        assert_eq!(
            handle_key(event, definitions, &mut state),
            Some(CommitTypeSelection::Cancelled)
        );
    }
    let mut state = PickerState::default();
    assert!(handle_key(key(KeyCode::Char('x')), definitions, &mut state).is_none());
}

#[test]
fn normal_render_contains_catalog_details_selection_and_help() -> Result<(), Box<dyn Error>> {
    let backend = TestBackend::new(100, 24);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| {
        render(frame, built_in_commit_types(), PickerState { selected: 1 });
    })?;
    let text = buffer_text(terminal.backend());
    assert!(text.contains("gitserious commit"));
    assert!(text.contains("feat"));
    assert!(text.contains("fix"));
    assert!(text.contains(built_in_commit_types()[1].description()));
    assert!(text.contains("Enter select"));
    Ok(())
}

#[test]
fn narrow_or_short_render_requests_resize_and_keeps_cancel_help() -> Result<(), Box<dyn Error>> {
    for (width, height) in [(23, 20), (80, 7)] {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|frame| {
            render(frame, built_in_commit_types(), PickerState::default());
        })?;
        let text = buffer_text(terminal.backend());
        assert!(text.contains("Terminal too small"));
        assert!(text.contains("Esc"));
        assert!(text.contains("cancel"));
    }
    Ok(())
}

#[test]
fn empty_catalog_is_rejected_before_terminal_initialization() -> Result<(), Box<dyn Error>> {
    let error = RatatuiCommitTypeSelector
        .select(&[])
        .err()
        .ok_or("empty catalog should fail")?;
    assert!(matches!(error, CommitTypeSelectorError::EmptyCatalog));
    assert!(!error.to_string().is_empty());
    assert!(error.source().is_none());
    Ok(())
}
