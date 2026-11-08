use crate::library_lookup::LibraryLookup;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::{Constraint, Layout, Rect};

use super::copy_tests::team;
use super::tests::Workspace;
use super::*;

type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const WIDTH: u16 = 120;

fn press(state: &mut State, workspace: &Workspace, keys: &[KeyCode]) {
    for &code in keys {
        state.handle_event(
            Event::Key(KeyEvent::new(code, KeyModifiers::NONE)),
            workspace,
        );
    }
}

fn rows(state: &mut State, height: u16) -> TestResult<Vec<String>> {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, height))?;
    terminal.draw(|frame| {
        let [_, body] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(frame.area());
        state.render_at(frame, body);
    })?;
    let buffer = terminal.backend().buffer();
    Ok(buffer
        .content
        .chunks(usize::from(buffer.area.width))
        .map(|row| row.iter().map(ratatui::buffer::Cell::symbol).collect())
        .collect())
}

fn popup(rows: &[String]) -> TestResult<Rect> {
    let mut origin = None;
    let mut far = None;
    for (y, row) in rows.iter().enumerate() {
        for (x, symbol) in row.chars().enumerate() {
            if symbol == '╔' {
                assert!(origin.is_none(), "two alert boxes");
                origin = Some((u16::try_from(x)?, u16::try_from(y)?));
            }
            if symbol == '╝' {
                far = Some((u16::try_from(x)?, u16::try_from(y)?));
            }
        }
    }
    let (x, y) = origin.ok_or("alert box missing")?;
    let (right, bottom) = far.ok_or("alert box has no far corner")?;
    Ok(Rect::new(
        x,
        y,
        right.saturating_add(1).saturating_sub(x),
        bottom.saturating_add(1).saturating_sub(y),
    ))
}

fn assert_centered(label: &str, rows: &[String]) -> TestResult {
    let popup = popup(rows)?;
    let width = u16::try_from(rows.first().ok_or("empty screen")?.chars().count())?;
    let height = u16::try_from(rows.len())?;
    let left = popup.x;
    let top = popup.y;
    let right_margin = width.saturating_sub(popup.right());
    let bottom_margin = height.saturating_sub(popup.bottom());
    assert!(
        left.abs_diff(right_margin) <= 1,
        "{label} horizontal margins are {left} and {right_margin} at {}..{} of {width}",
        popup.x,
        popup.right()
    );
    assert!(
        top.abs_diff(bottom_margin) <= 1,
        "{label} vertical margins are {top} and {bottom_margin} at {}..{} of {height}",
        popup.y,
        popup.bottom()
    );
    Ok(())
}

fn library(state: &mut State, workspace: &Workspace) -> TestResult {
    press(state, workspace, &[KeyCode::Char('2')]);
    let team = state
        .library()
        .and_then(|library| library.find_path(&["team"]))
        .ok_or("team")?;
    state.taxonomies.select(team);
    Ok(())
}

#[test]
fn the_copy_picker_is_centered_on_the_terminal() -> TestResult {
    let workspace = team()?;
    let mut state = State::new(&workspace);
    library(&mut state, &workspace)?;
    press(
        &mut state,
        &workspace,
        &[
            KeyCode::Enter,
            KeyCode::Down,
            KeyCode::Enter,
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Char('c'),
        ],
    );
    let drawn = rows(&mut state, 40)?;
    assert!(
        drawn.join("\n").contains("Copy Property"),
        "copy picker missing"
    );
    assert_centered("copy picker", &drawn)
}

#[test]
fn the_delete_dialog_is_centered_on_the_terminal() -> TestResult {
    let workspace = team()?;
    let mut state = State::new(&workspace);
    library(&mut state, &workspace)?;
    press(&mut state, &workspace, &[KeyCode::Char('d')]);
    assert!(matches!(state.screen, Screen::ConfirmRemove(_)));
    let drawn = rows(&mut state, 40)?;
    assert!(
        drawn.join("\n").contains("Delete Taxonomy"),
        "delete dialog missing"
    );
    assert_centered("delete dialog", &drawn)
}

#[test]
fn the_command_palette_is_centered_on_the_terminal() -> TestResult {
    let workspace = team()?;
    let mut state = State::new(&workspace);
    press(
        &mut state,
        &workspace,
        &[KeyCode::Char('2'), KeyCode::Char('/')],
    );
    assert!(state.home_overlay.is_some());
    let drawn = rows(&mut state, 41)?;
    assert!(
        drawn.join("\n").contains("Commands"),
        "command palette missing"
    );
    assert_centered("command palette", &drawn)
}
