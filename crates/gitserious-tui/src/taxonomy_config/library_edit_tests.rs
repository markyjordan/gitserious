use ratatui::{Terminal, backend::TestBackend};

use super::library_rows_tests::{Result, fixture, press, select};
use super::*;

const SIZES: [(u16, u16); 2] = [(87, 40), (100, 30)];

fn property() -> Result<(tests::Workspace, State)> {
    let (workspace, mut state) = fixture()?;
    select(&mut state, &["team"])?;
    press(&mut state, &workspace, &[KeyCode::Enter]);
    select(&mut state, &["team", "feat"])?;
    press(&mut state, &workspace, &[KeyCode::Enter]);
    select(&mut state, &["team", "feat", "constraints"])?;
    Ok((workspace, state))
}

fn draw(state: &mut State, size: (u16, u16)) -> Result<Terminal<TestBackend>> {
    let mut terminal = Terminal::new(TestBackend::new(size.0, size.1))?;
    terminal.draw(|frame| state.render(frame))?;
    Ok(terminal)
}

fn row(terminal: &Terminal<TestBackend>, y: u16) -> String {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

#[test]
fn edit_focus_has_amber_cues_muted_navigation_and_a_footer_counter() -> Result {
    for size in SIZES {
        let (workspace, mut state) = property()?;
        state.embedded = true;
        let browsing = draw(&mut state, size)?;
        let footer = tests::footer_text(&row(&browsing, size.1 - 1));
        assert!(!footer.contains("NAV") && !footer.contains("EDIT"));
        assert!(footer.contains("e edit") && footer.contains("d delete"));

        press(&mut state, &workspace, &[KeyCode::Char('e')]);
        let terminal = draw(&mut state, size)?;
        let buffer = terminal.backend().buffer();
        let footer = tests::footer_text(&row(&terminal, size.1 - 1));
        assert!(footer.contains("EDIT   enter next field  ←/→ cursor  esc save"));
        assert!(footer.trim_end().ends_with("11/80"));
        assert_eq!(buffer[(1, size.1 - 1)].bg, crate::theme::AMBER);
        assert!(buffer[(4, 2)].modifier.contains(Modifier::DIM));
        let areas = state.taxonomies.observe(state.library());
        let mut gutters = 0;
        for y in 0..size.1 - 1 {
            let text = row(&terminal, y);
            if text.contains("Library") && y > 2 {
                assert!(buffer[(CONTENT_X + 1, y)].modifier.contains(Modifier::DIM));
            }
            for x in 0..size.0 {
                let cell = &buffer[(x, y)];
                if cell.symbol() == "▌" {
                    assert_eq!(cell.fg, crate::theme::AMBER);
                    gutters += 1;
                }
                if areas.inspector_area.contains((x, y).into()) {
                    assert!(!cell.modifier.contains(Modifier::DIM));
                }
            }
            if text.contains("Property constraints") {
                assert!(text.trim_end().ends_with("EDIT"));
            }
            assert!(!text.contains("11/80"));
        }
        assert_eq!(gutters, 2);

        press(&mut state, &workspace, &[KeyCode::Enter, KeyCode::Enter]);
        let terminal = draw(&mut state, size)?;
        let footer = tests::footer_text(&row(&terminal, size.1 - 1));
        assert!(footer.contains("←/→ policy"));
        assert!(!footer.contains("/80") && !footer.contains("/100"));
        press(&mut state, &workspace, &[KeyCode::Esc]);
        let terminal = draw(&mut state, size)?;
        assert!(!row(&terminal, size.1 - 1).contains("EDIT"));
        assert!(
            !terminal.backend().buffer()[(4, 2)]
                .modifier
                .contains(Modifier::DIM)
        );
    }
    Ok(())
}

#[test]
fn an_empty_condition_keeps_editing_and_colors_its_footer_count_red() -> Result {
    for size in SIZES {
        let (workspace, mut state) = property()?;
        press(
            &mut state,
            &workspace,
            &[
                KeyCode::Char('e'),
                KeyCode::Enter,
                KeyCode::Enter,
                KeyCode::Enter,
                KeyCode::Home,
            ],
        );
        press(&mut state, &workspace, &[KeyCode::Delete; 80]);
        press(&mut state, &workspace, &[KeyCode::Esc]);
        assert!(state.taxonomies.editing());
        let terminal = draw(&mut state, size)?;
        let footer = row(&terminal, size.1 - 1);
        assert!(footer.trim_end().ends_with("0/80"));
        let x = u16::try_from(
            footer[..footer.find("0/80").ok_or("counter")?]
                .chars()
                .count(),
        )?;
        assert_eq!(terminal.backend().buffer()[(x, size.1 - 1)].fg, Color::Red);
        let area = state.taxonomies.observe(state.library()).inspector_area;
        let buffer = terminal.backend().buffer();
        let text = (area.y..area.bottom())
            .map(|y| {
                (area.x..area.right())
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join(" ");
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            text.contains("A conditional policy needs a condition."),
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn the_counter_counts_graphemes_in_the_focused_field() -> Result {
    for size in SIZES {
        let (workspace, mut state) = property()?;
        press(
            &mut state,
            &workspace,
            &[KeyCode::Char('e'), KeyCode::Enter, KeyCode::Home],
        );
        press(&mut state, &workspace, &[KeyCode::Delete; 100]);
        state.handle_event(Event::Paste("e\u{301}🦀".into()), &workspace);
        let terminal = draw(&mut state, size)?;
        assert!(row(&terminal, size.1 - 1).trim_end().ends_with("2/100"));
    }
    Ok(())
}
