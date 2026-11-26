use crate::config::library_lookup::LibraryLookup;

use super::copy_tests::team;
use super::tests::{Workspace, footer_text};
use super::*;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Each action key and the footer hint that offers it. Nothing offers n.
const LETTERS: [(char, &str); 5] = [
    ('e', "e edit"),
    ('f', "f fork"),
    ('c', "c copy"),
    ('d', "d delete"),
    ('n', "n new"),
];

/// Only one root stays open, so one walk opens conventional's feat for built-in rows and the
/// other opens all of team for custom and add rows.
const OPENINGS: [&[&[&str]]; 2] = [
    &[&["conventional"], &["conventional", "feat"]],
    &[
        &["team"],
        &["team", "feat"],
        &["team", "fix"],
        &["team", "docs"],
    ],
];

/// The Library with each of `paths` open.
fn opened(workspace: &Workspace, paths: &[&[&str]]) -> Result<State> {
    let mut state = State::new(workspace);
    state.handle_event(Event::Key(KeyEvent::from(KeyCode::Char('2'))), workspace);
    for &path in paths {
        let node = state
            .library()
            .and_then(|library| library.find_path(path))
            .ok_or("the node exists")?;
        let open = state
            .taxonomies
            .observe(state.library())
            .expanded
            .contains(&node);
        state.taxonomies.select(node);
        if !open {
            state.handle_event(Event::Key(KeyEvent::from(KeyCode::Enter)), workspace);
        }
    }
    Ok(state)
}

/// A fresh `team` workspace, since a reorder saves into it, and `opened` with the cursor on
/// its `row`th row, or `None` past the last row.
fn on_row(paths: &[&[&str]], row: usize) -> Result<Option<(Workspace, State)>> {
    let workspace = team()?;
    let mut state = opened(&workspace, paths)?;
    for _ in 0..50 {
        state.handle_event(Event::Key(KeyEvent::from(KeyCode::Up)), &workspace);
    }
    for _ in 0..row {
        let before = state.taxonomies.observe(state.library()).cursor;
        state.handle_event(Event::Key(KeyEvent::from(KeyCode::Down)), &workspace);
        if state.taxonomies.observe(state.library()).cursor == before {
            return Ok(None);
        }
    }
    Ok(Some((workspace, state)))
}

/// Everything a key could change: the draft, the outline, the screen, and whether an alert
/// shows, so a key that only alerts counts as acting.
fn trace(state: &State) -> String {
    format!(
        "{:?} {:?} {:?} {}",
        state.library(),
        state.taxonomies.observe(state.library()),
        state.screen,
        state.status.is_some(),
    )
}

#[test]
fn every_library_row_takes_exactly_the_action_keys_its_footer_hints() -> Result {
    for paths in OPENINGS {
        let mut rows = 0;
        while let Some((_, state)) = on_row(paths, rows)? {
            let row = state.taxonomies.observe(state.library()).cursor_path;
            let footer = footer_text(
                &state
                    .taxonomies
                    .status_line(state.library(), 400)
                    .to_string(),
            );
            for (letter, hint) in LETTERS {
                let Some((workspace, mut state)) = on_row(paths, rows)? else {
                    return Err("the row moved".into());
                };
                let before = trace(&state);
                state.handle_event(
                    Event::Key(KeyEvent::from(KeyCode::Char(letter))),
                    &workspace,
                );
                let acted = trace(&state) != before;
                assert_eq!(
                    acted,
                    footer.contains(hint),
                    "{letter} on {row:?}: {footer}"
                );
            }
            let mut reordered = false;
            for code in [KeyCode::Up, KeyCode::Down] {
                let Some((workspace, mut state)) = on_row(paths, rows)? else {
                    return Err("the row moved".into());
                };
                let before = format!("{:?}", state.library());
                state.handle_event(
                    Event::Key(KeyEvent::new(code, KeyModifiers::ALT)),
                    &workspace,
                );
                reordered |= format!("{:?}", state.library()) != before;
                assert!(
                    state.status.is_none(),
                    "alt+{code:?} on {row:?} raised an alert"
                );
            }
            assert_eq!(
                reordered,
                footer.contains("alt+↑/↓ reorder"),
                "alt+↑/↓ on {row:?}: {footer}"
            );
            rows += 1;
        }
        assert!(rows > 10, "only {rows} rows");
    }
    Ok(())
}
