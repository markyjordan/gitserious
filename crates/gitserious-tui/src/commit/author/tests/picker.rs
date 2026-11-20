use super::*;

#[test]
fn picker_navigation_wraps_selects_and_cancels() {
    let definitions = built_in_commit_types();
    let mut session = AuthoringSession::new(definitions, None);
    assert_eq!(session.stage, Stage::SelectType);

    press(&mut session, KeyCode::Up);
    assert_eq!(session.selected_type, definitions.len() - 1);
    press(&mut session, KeyCode::Down);
    assert_eq!(session.selected_type, 0);
    for unsupported in [KeyCode::Tab, KeyCode::Home, KeyCode::End, KeyCode::PageDown] {
        press(&mut session, unsupported);
        assert_eq!(session.selected_type, 0);
        assert_eq!(session.stage, Stage::SelectType);
    }
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.stage, Stage::Compose);
    assert_eq!(session.composer.definition().id().as_str(), "feat");

    let mut cancelled = AuthoringSession::new(definitions, None);
    assert_eq!(
        cancelled.handle_event(key(KeyCode::Esc)),
        Some(AuthoringOutcome::Cancelled)
    );
}

#[test]
fn the_picker_ignores_text_until_enter_opens_the_subject() -> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), None);
    press(&mut session, KeyCode::Down);
    let picker = rendered_buffer(&mut session, 100, 30)?;
    typed(&mut session, "feat");
    paste(&mut session, "chore");
    press(&mut session, KeyCode::Backspace);
    assert_eq!(session.stage, Stage::SelectType);
    assert_eq!(rendered_buffer(&mut session, 100, 30)?, picker);

    press(&mut session, KeyCode::Enter);
    assert_eq!(session.stage, Stage::Compose);
    assert_eq!(session.composer.focus, MessageFieldId::Subject);
    assert!(rendered(&mut session, 100, 30)?.contains("fix(scope): description"));
    assert!(!session.composer.dirty());
    Ok(())
}

#[test]
fn left_and_right_switch_the_taxonomy_list() -> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::with_context(super::super::built_in_context()?);
    let buffer = rendered_buffer(&mut session, 100, 30)?;
    let taxonomies = row_text(&buffer, 100, 2);
    assert!(
        taxonomies.starts_with("   conventional  research  infra-ops  "),
        "{taxonomies}"
    );
    assert!(taxonomies.ends_with("  conventional-v1   "), "{taxonomies}");

    press(&mut session, KeyCode::Right);
    let buffer = rendered_buffer(&mut session, 100, 30)?;
    assert!(row_text(&buffer, 100, 2).ends_with("  research-v1   "));
    assert!(picker_rows(&buffer, 100, 4..5)[0].starts_with("hypothesis "));
    assert_eq!(session.definition().id().as_str(), "hypothesis");

    press(&mut session, KeyCode::Right);
    let buffer = rendered_buffer(&mut session, 100, 30)?;
    assert!(row_text(&buffer, 100, 2).ends_with("  infra-ops-v1   "));
    assert!(picker_rows(&buffer, 100, 4..5)[0].starts_with("provision "));
    assert_eq!(session.definition().id().as_str(), "provision");

    press(&mut session, KeyCode::Left);
    press(&mut session, KeyCode::Left);
    press(&mut session, KeyCode::Down);
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.stage, Stage::Compose);
    assert_eq!(
        session.taxonomy().ok_or("missing taxonomy")?.id().as_str(),
        "conventional"
    );
    assert_eq!(session.composer.definition().id().as_str(), "fix");
    Ok(())
}

#[test]
fn the_picker_never_draws_the_typing_note_or_a_tab_hint() -> Result<(), Box<dyn Error>> {
    for (width, height) in [(80, 24), (100, 30), (60, 18)] {
        let mut session = AuthoringSession::with_context(super::super::built_in_context()?);
        let text = rendered(&mut session, width, height)?;
        for removed in ["Typing filters", "Changing the type later", "tab taxonomy"] {
            assert!(!text.contains(removed), "{removed} at {width}x{height}");
        }
        assert_eq!(
            hints(&session),
            [
                ("enter", "choose"),
                ("↑/↓", "move"),
                ("←/→", "taxonomy"),
                ("esc", "back"),
            ]
        );
    }
    Ok(())
}

#[test]
fn the_selection_stays_visible_when_the_list_is_taller_than_the_panel() -> Result<(), Box<dyn Error>>
{
    let mut session = AuthoringSession::new(built_in_commit_types(), None);
    let shown = |session: &mut AuthoringSession| -> Result<Vec<String>, Box<dyn Error>> {
        let buffer = rendered_buffer(session, 100, 18)?;
        let selected = session.definition().id().to_string();
        let rows = picker_rows(&buffer, 100, 4..14);
        let row = rows
            .iter()
            .position(|row| row.split_whitespace().next() == Some(selected.as_str()))
            .ok_or("the selection scrolled out of view")?;
        assert_eq!(
            buffer[(3, 4 + u16::try_from(row)?)].bg,
            theme().cursor,
            "{selected} is highlighted"
        );
        Ok(rows
            .iter()
            .map(|row| row.split_whitespace().next().unwrap_or("").to_owned())
            .collect())
    };
    let top = [
        "feat", "fix", "refactor", "perf", "test", "docs", "chore", "build", "ci", "style",
    ];
    assert_eq!(shown(&mut session)?, top);
    press(&mut session, KeyCode::Up);
    assert_eq!(
        shown(&mut session)?,
        [
            "fix", "refactor", "perf", "test", "docs", "chore", "build", "ci", "style", "revert"
        ]
    );
    press(&mut session, KeyCode::Down);
    assert_eq!(shown(&mut session)?, top);
    press(&mut session, KeyCode::Up);
    assert_eq!(shown(&mut session)?[0], "fix");
    press(&mut session, KeyCode::Up);
    assert_eq!(
        shown(&mut session)?[0],
        "fix",
        "moving up inside the view does not scroll"
    );
    Ok(())
}

#[test]
fn clicking_a_taxonomy_or_a_type_row_selects_it() -> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::with_context(super::super::built_in_context()?);
    let buffer = rendered_buffer(&mut session, 100, 30)?;
    let conventional = find_ascii(&buffer, 100, 30, "conventional").ok_or("missing tab")?;
    let research = find_ascii(&buffer, 100, 30, "research").ok_or("missing tab")?;
    assert_eq!(buffer[conventional].fg, theme().text);
    assert!(
        buffer[conventional]
            .modifier
            .contains(Modifier::BOLD | Modifier::UNDERLINED)
    );
    assert_eq!(buffer[research].fg, theme().dim);
    assert!(!buffer[research].modifier.contains(Modifier::UNDERLINED));

    assert!(
        session
            .handle_event(left_click(research.0 + 7, research.1))
            .is_none()
    );
    assert_eq!(
        session.taxonomy().ok_or("missing taxonomy")?.id().as_str(),
        "research"
    );
    let buffer = rendered_buffer(&mut session, 100, 30)?;
    assert!(buffer[research].modifier.contains(Modifier::UNDERLINED));
    let eval = find_ascii(&buffer, 100, 30, "eval ").ok_or("missing eval row")?;
    assert!(session.handle_event(left_click(98, eval.1)).is_none());
    assert_eq!(session.stage, Stage::SelectType);
    assert_eq!(session.definition().id().as_str(), "eval");
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.composer.definition().id().as_str(), "eval");
    assert_eq!(session.stage, Stage::Compose);
    Ok(())
}

#[test]
fn type_rows_align_keys_and_properties_and_highlight_the_selection() -> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), None);
    press(&mut session, KeyCode::Down);
    let buffer = rendered_buffer(&mut session, 100, 30)?;
    assert!(reversed_positions(&buffer, 100, 30).is_empty());
    assert_eq!(row_text(&buffer, 100, 1).trim_end(), "");

    for (x, y) in [(1, 1), (98, 1), (1, 17), (98, 17)] {
        assert_eq!(buffer[(x, y)].bg, theme().band, "panel at ({x}, {y})");
    }
    for (x, y) in [(0, 1), (99, 1), (1, 0), (1, 18)] {
        assert_ne!(
            buffer[(x, y)].bg,
            theme().band,
            "outside the panel at ({x}, {y})"
        );
    }
    assert_no_box_drawing(&buffer, 100, 30);

    assert_eq!(find_ascii(&buffer, 100, 30, "feat"), Some((3, 4)));
    assert_eq!(
        find_ascii(&buffer, 100, 30, "intent · decision"),
        Some((15, 4))
    );
    assert_eq!(
        (buffer[(3, 4)].fg, buffer[(3, 4)].bg),
        (theme().text, theme().band)
    );
    assert_eq!(
        (buffer[(15, 4)].fg, buffer[(15, 4)].bg),
        (theme().dim, theme().band)
    );

    assert_eq!(
        row_text(&buffer, 100, 5).trim_end(),
        "   fix         cause · decision · constraints"
    );
    for column in 0..100 {
        let cell = &buffer[(column, 5)];
        if (1..99).contains(&column) {
            assert_eq!((cell.fg, cell.bg), (theme().ground, theme().cursor));
        } else {
            assert_ne!(cell.bg, theme().cursor);
        }
    }
    let detail = find_ascii(&buffer, 100, 30, "A causal correction").ok_or("missing detail")?;
    assert_eq!(detail, (15, 16));
    assert_eq!(buffer[(3, 16)].symbol(), "f");
    assert!(buffer[(3, 16)].modifier.contains(Modifier::BOLD));
    Ok(())
}

#[test]
fn type_picker_renders_the_complete_effective_conventional_catalog() -> Result<(), Box<dyn Error>> {
    let definitions = built_in_commit_types();
    assert_eq!(
        definitions
            .iter()
            .map(|definition| definition.id().as_str())
            .collect::<Vec<_>>(),
        [
            "feat", "fix", "refactor", "perf", "test", "docs", "chore", "build", "ci", "style",
            "revert",
        ]
    );

    let mut session = AuthoringSession::new(definitions, None);
    let buffer = rendered_buffer(&mut session, 120, 30)?;
    for (index, definition) in definitions.iter().enumerate() {
        let properties = definition
            .properties()
            .iter()
            .map(|property| property.key().to_string())
            .collect::<Vec<_>>()
            .join(" · ");
        assert_eq!(
            row_text(&buffer, 120, 4 + u16::try_from(index)?).trim_end(),
            format!("   {:<12}{properties}", definition.id().as_str())
        );
    }
    Ok(())
}

#[test]
fn a_taller_terminal_scrolls_the_type_list_back_to_fill_it() -> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), None);
    press(&mut session, KeyCode::Up);
    let short = rendered_buffer(&mut session, 100, 18)?;
    assert_eq!(
        picker_rows(&short, 100, 4..5)[0].split_whitespace().next(),
        Some("fix")
    );
    let tall = rendered_buffer(&mut session, 100, 30)?;
    let rows = picker_rows(&tall, 100, 4..15);
    assert_eq!(rows[0].split_whitespace().next(), Some("feat"));
    assert_eq!(rows[10].split_whitespace().next(), Some("revert"));
    assert_eq!(session.type_offset, 0);
    Ok(())
}
