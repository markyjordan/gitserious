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
    assert_eq!(session.cards.definition().id().as_str(), "feat");

    let mut cancelled = AuthoringSession::new(definitions, None);
    assert_eq!(
        cancelled.handle_event(key(KeyCode::Esc)),
        Some(AuthoringOutcome::Cancelled)
    );
}

#[test]
fn typing_on_the_subject_line_lists_the_types_whose_key_starts_with_it()
-> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), None);
    typed(&mut session, "r");
    let buffer = rendered_buffer(&mut session, 100, 30)?;
    assert_eq!(row_text(&buffer, 100, 1).trim_end(), "▎  r");
    assert_eq!(
        picker_rows(&buffer, 100, 6..10),
        [
            "refactor    motivation · decision · invariant",
            "revert      reason · impact · follow-up",
            "",
            "refactor    A structural transformation that preserves intended behavior.",
        ],
        "perf and chore contain r but do not start with it"
    );
    press(&mut session, KeyCode::Down);
    assert_eq!(session.definition().id().as_str(), "revert");
    press(&mut session, KeyCode::Down);
    assert_eq!(session.definition().id().as_str(), "refactor");
    press(&mut session, KeyCode::Up);
    assert_eq!(session.definition().id().as_str(), "revert");

    press(&mut session, KeyCode::Backspace);
    let buffer = rendered_buffer(&mut session, 100, 30)?;
    assert_eq!(
        row_text(&buffer, 100, 1).trim_end(),
        "▎   type(scope): description"
    );
    assert_eq!(picker_rows(&buffer, 100, 6..17).len(), 11);
    assert_eq!(session.definition().id().as_str(), "feat");

    typed(&mut session, "fi");
    let buffer = rendered_buffer(&mut session, 100, 30)?;
    assert_eq!(row_text(&buffer, 100, 1).trim_end(), "▎  fi");
    assert_eq!(
        picker_rows(&buffer, 100, 6..9),
        [
            "fix         cause · decision · constraints",
            "",
            "fix         A causal correction of an observed failure or incorrect behavior.",
        ]
    );
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.stage, Stage::Compose);
    assert_eq!(session.cards.definition().id().as_str(), "fix");
    assert!(!session.cards.dirty(), "the filter is not a description");
    Ok(())
}

#[test]
fn a_filter_nothing_matches_says_so_and_enter_waits() -> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), None);
    typed(&mut session, "qx");
    assert_eq!(
        session.stage,
        Stage::SelectType,
        "q types on the subject line"
    );
    let buffer = rendered_buffer(&mut session, 100, 30)?;
    assert_eq!(row_text(&buffer, 100, 1).trim_end(), "▎  qx");
    assert_eq!(
        picker_rows(&buffer, 100, 6..7),
        ["No type starts with \"qx\""]
    );
    let text = rendered(&mut session, 100, 30)?;
    assert!(!text.contains("feat"), "no type row or description");
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.stage, Stage::SelectType);

    press(&mut session, KeyCode::Backspace);
    press(&mut session, KeyCode::Backspace);
    let buffer = rendered_buffer(&mut session, 100, 30)?;
    assert!(picker_rows(&buffer, 100, 6..7)[0].starts_with("feat"));

    typed(&mut session, "zz");
    press(&mut session, KeyCode::Esc);
    assert_eq!(
        session.stage,
        Stage::SelectType,
        "esc clears the filter first"
    );
    let buffer = rendered_buffer(&mut session, 100, 30)?;
    assert_eq!(
        row_text(&buffer, 100, 1).trim_end(),
        "▎   type(scope): description"
    );
    assert!(picker_rows(&buffer, 100, 6..7)[0].starts_with("feat"));
    assert_eq!(
        session.handle_event(key(KeyCode::Esc)),
        Some(AuthoringOutcome::Cancelled)
    );
    Ok(())
}

#[test]
fn left_and_right_switch_the_taxonomy_and_keep_the_filter() -> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::with_context(super::super::built_in_context()?);
    let buffer = rendered_buffer(&mut session, 100, 30)?;
    let taxonomies = row_text(&buffer, 100, 4);
    assert!(
        taxonomies.starts_with("   conventional  research  infra-ops  "),
        "{taxonomies}"
    );
    assert!(taxonomies.ends_with("  conventional-v1   "), "{taxonomies}");

    typed(&mut session, "fi");
    press(&mut session, KeyCode::Right);
    let buffer = rendered_buffer(&mut session, 100, 30)?;
    assert!(row_text(&buffer, 100, 4).ends_with("  research-v1   "));
    assert_eq!(row_text(&buffer, 100, 1).trim_end(), "▎  fi");
    assert_eq!(picker_rows(&buffer, 100, 7..8), [""], "one match");
    assert!(picker_rows(&buffer, 100, 6..7)[0].starts_with("fix "));
    assert_eq!(session.definition().id().as_str(), "fix");

    press(&mut session, KeyCode::Right);
    let buffer = rendered_buffer(&mut session, 100, 30)?;
    assert!(row_text(&buffer, 100, 4).ends_with("  infra-ops-v1   "));
    assert_eq!(
        picker_rows(&buffer, 100, 6..7),
        ["No type starts with \"fi\""]
    );
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.stage, Stage::SelectType);

    press(&mut session, KeyCode::Left);
    press(&mut session, KeyCode::Left);
    let buffer = rendered_buffer(&mut session, 100, 30)?;
    assert_eq!(
        picker_rows(&buffer, 100, 6..7),
        ["fix         cause · decision · constraints"]
    );
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.stage, Stage::Compose);
    assert_eq!(
        session.taxonomy().ok_or("missing taxonomy")?.id().as_str(),
        "conventional"
    );
    assert_eq!(session.cards.definition().id().as_str(), "fix");
    Ok(())
}

#[test]
fn the_picker_never_draws_the_typing_note_or_a_tab_hint() -> Result<(), Box<dyn Error>> {
    for (width, height) in [(80, 24), (100, 30), (60, 18)] {
        for filter in ["", "fi", "zz"] {
            let mut session = AuthoringSession::with_context(super::super::built_in_context()?);
            typed(&mut session, filter);
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
        let rows = picker_rows(&buffer, 100, 6..14);
        let row = rows
            .iter()
            .position(|row| row.split_whitespace().next() == Some(selected.as_str()))
            .ok_or("the selection scrolled out of view")?;
        assert_eq!(
            buffer[(3, 6 + u16::try_from(row)?)].bg,
            theme().cursor,
            "{selected} is highlighted"
        );
        Ok(rows
            .iter()
            .map(|row| row.split_whitespace().next().unwrap_or("").to_owned())
            .collect())
    };
    let top = [
        "feat", "fix", "refactor", "perf", "test", "docs", "chore", "build",
    ];
    assert_eq!(shown(&mut session)?, top);
    press(&mut session, KeyCode::Up);
    assert_eq!(
        shown(&mut session)?,
        [
            "perf", "test", "docs", "chore", "build", "ci", "style", "revert"
        ]
    );
    press(&mut session, KeyCode::Down);
    assert_eq!(shown(&mut session)?, top);
    for _ in 0..8 {
        press(&mut session, KeyCode::Down);
    }
    assert_eq!(
        shown(&mut session)?,
        [
            "fix", "refactor", "perf", "test", "docs", "chore", "build", "ci"
        ]
    );
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
    assert_eq!(session.cards.definition().id().as_str(), "eval");
    assert_eq!(session.stage, Stage::Compose);
    Ok(())
}

#[test]
fn type_rows_align_keys_and_properties_and_highlight_the_selection() -> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), None);
    press(&mut session, KeyCode::Down);
    let buffer = rendered_buffer(&mut session, 100, 30)?;
    assert_eq!(buffer[(0, 1)].symbol(), "▎");
    assert_eq!(buffer[(0, 1)].fg, theme().pending);
    assert!(buffer[(3, 1)].modifier.contains(Modifier::REVERSED));
    assert_eq!(find_ascii(&buffer, 100, 30, "type(scope)"), Some((4, 1)));
    assert_eq!(buffer[(4, 1)].fg, theme().dim);

    for (x, y) in [(1, 3), (98, 3), (1, 19), (98, 19)] {
        assert_eq!(buffer[(x, y)].bg, theme().band, "panel at ({x}, {y})");
    }
    for (x, y) in [(0, 3), (99, 3), (1, 2), (1, 20)] {
        assert_ne!(
            buffer[(x, y)].bg,
            theme().band,
            "outside the panel at ({x}, {y})"
        );
    }
    assert_no_box_drawing(&buffer, 100, 30);

    assert_eq!(find_ascii(&buffer, 100, 30, "feat"), Some((3, 6)));
    assert_eq!(
        find_ascii(&buffer, 100, 30, "intent · decision"),
        Some((15, 6))
    );
    assert_eq!(
        (buffer[(3, 6)].fg, buffer[(3, 6)].bg),
        (theme().text, theme().band)
    );
    assert_eq!(
        (buffer[(15, 6)].fg, buffer[(15, 6)].bg),
        (theme().dim, theme().band)
    );

    assert_eq!(
        row_text(&buffer, 100, 7).trim_end(),
        "   fix         cause · decision · constraints"
    );
    for column in 0..100 {
        let cell = &buffer[(column, 7)];
        if (1..99).contains(&column) {
            assert_eq!((cell.fg, cell.bg), (theme().ground, theme().cursor));
        } else {
            assert_ne!(cell.bg, theme().cursor);
        }
    }
    let detail = find_ascii(&buffer, 100, 30, "A causal correction").ok_or("missing detail")?;
    assert_eq!(detail, (15, 18));
    assert_eq!(buffer[(3, 18)].symbol(), "f");
    assert!(buffer[(3, 18)].modifier.contains(Modifier::BOLD));
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
            row_text(&buffer, 120, 6 + u16::try_from(index)?).trim_end(),
            format!("   {:<12}{properties}", definition.id().as_str())
        );
    }
    Ok(())
}
