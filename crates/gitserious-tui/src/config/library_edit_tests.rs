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

fn text(terminal: &Terminal<TestBackend>) -> String {
    (0..terminal.backend().buffer().area.height)
        .map(|y| row(terminal, y))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn new_taxonomy_form_omits_facts_until_saved() -> Result {
    for size in [(87, 40), (100, 30), (125, 51), (66, 18)] {
        let (workspace, mut state) = fixture()?;
        press(&mut state, &workspace, &[KeyCode::Down; 30]);
        assert_eq!(
            state.taxonomies.observe(state.library()).cursor_path,
            ["+ new taxonomy"]
        );
        press(&mut state, &workspace, &[KeyCode::Enter]);
        for input in ["", "Fresh ideas"] {
            state.handle_event(Event::Paste(input.into()), &workspace);
            let terminal = draw(&mut state, size)?;
            let screen = text(&terminal);
            assert!(!screen.contains("Version"), "{size:?}: {screen}");
            assert!(!screen.contains("Types"), "{size:?}: {screen}");
            assert!(!screen.contains("Created"), "{size:?}: {screen}");
            assert!(!screen.contains("Modified"), "{size:?}: {screen}");
            assert!(screen.contains("Name"), "{size:?}: {screen}");
            assert!(screen.contains("Description"), "{size:?}: {screen}");
        }
        press(&mut state, &workspace, &[KeyCode::Enter]);
        state.handle_event(Event::Paste("Ideas for the team.".into()), &workspace);
        let terminal = draw(&mut state, size)?;
        let screen = text(&terminal);
        assert!(!screen.contains("Version"), "{size:?}: {screen}");
        assert!(!screen.contains("Types"), "{size:?}: {screen}");

        state.handle_event(
            Event::Key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
            &workspace,
        );
        assert!(!state.taxonomies.editing());
        let terminal = draw(&mut state, size)?;
        let screen = text(&terminal);
        press(&mut state, &workspace, &[KeyCode::PageDown]);
        let terminal = draw(&mut state, size)?;
        let screen = screen + &text(&terminal);
        assert!(screen.contains("Types"), "{size:?}: {screen}");
        assert!(!screen.contains("Version"), "{size:?}: {screen}");
        press(&mut state, &workspace, &[KeyCode::Char('e')]);
        let terminal = draw(&mut state, (size.0, size.1.max(30)))?;
        let screen = text(&terminal);
        assert!(!screen.contains("Version"), "{size:?}: {screen}");
        state.handle_event(
            Event::Key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
            &workspace,
        );
        press(&mut state, &workspace, &[KeyCode::Char('f')]);
        let terminal = draw(&mut state, (size.0, size.1.max(30)))?;
        let screen = text(&terminal);
        assert!(!screen.contains("Version"), "{size:?}: {screen}");
        state.handle_event(
            Event::Key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
            &workspace,
        );
        press(&mut state, &workspace, &[KeyCode::PageDown]);
        let terminal = draw(&mut state, size)?;
        let screen = screen + &text(&terminal);
        assert!(screen.contains("Types"), "{size:?}: {screen}");
        assert!(screen.contains("Forked from"), "{size:?}: {screen}");
    }
    Ok(())
}

#[test]
fn saved_taxonomy_dates_appear_only_while_inspecting() -> Result {
    let (workspace, mut state) = fixture()?;
    select(&mut state, &["team"])?;
    let terminal = draw(&mut state, (100, 40))?;
    let inspect = text(&terminal);
    press(&mut state, &workspace, &[KeyCode::Char('e')]);
    let terminal = draw(&mut state, (100, 40))?;
    let edit = text(&terminal);
    assert!(
        inspect.contains("Created")
            && !inspect.contains("Created on")
            && inspect.contains("Modified")
            && !inspect.contains("Modified on")
            && !edit.contains("Created")
            && !edit.contains("Modified"),
        "inspect created {} modified {} edit created {} modified {}\nINSPECT\n{inspect}\nEDIT\n{edit}",
        inspect.contains("Created"),
        inspect.contains("Modified"),
        edit.contains("Created"),
        edit.contains("Modified"),
    );
    Ok(())
}

#[test]
fn edit_focus_has_amber_cues_muted_navigation_and_a_footer_counter() -> Result {
    for size in SIZES {
        let (workspace, mut state) = property()?;
        let browsing = draw(&mut state, size)?;
        let footer = tests::footer_text(&row(&browsing, size.1 - 1));
        assert!(!footer.contains("NAV") && !footer.contains("EDIT"));
        assert!(footer.contains("e edit") && footer.contains("d delete"));

        press(&mut state, &workspace, &[KeyCode::Char('e')]);
        let terminal = draw(&mut state, size)?;
        let buffer = terminal.backend().buffer();
        let footer = tests::footer_text(&row(&terminal, size.1 - 1));
        assert!(footer.contains("EDIT   enter next field  ←/→ cursor  ctrl+w  save  esc cancel"));
        assert!(footer.trim_end().ends_with("11/80"));
        assert_eq!(buffer[(1, size.1 - 1)].bg, theme().pending);
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
                    assert_eq!(cell.fg, theme().pending);
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
        state.handle_event(
            Event::Key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
            &workspace,
        );
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
        state.handle_event(
            Event::Key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
            &workspace,
        );
        assert!(state.taxonomies.editing());
        let terminal = draw(&mut state, size)?;
        let footer = row(&terminal, size.1 - 1);
        assert!(footer.trim_end().ends_with("0/80"));
        let x = u16::try_from(
            footer[..footer.find("0/80").ok_or("counter")?]
                .chars()
                .count(),
        )?;
        assert_eq!(
            terminal.backend().buffer()[(x, size.1 - 1)].fg,
            theme().blocks
        );
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
