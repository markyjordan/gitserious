use std::error::Error;

use gitserious_app::{CommitDraftAuthor, CommitDraftAuthorOutcome};
use gitserious_core::{
    CommitTypeDefinition, CommitTypeId, ConditionId, PropertyCondition, PropertyDefinition,
    PropertyKey, PropertyMultiplicity, PropertyRequirement, SchemaVersion, built_in_commit_types,
    render_commit_message,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use tui_textarea::{CursorMove, CursorRenderMode, TextArea, WrapMode};

use crate::{RatatuiCommitDraftAuthor, RatatuiCommitDraftAuthorError};

mod author_harness {
    pub(crate) mod state {
        include!("../../src/author/state.rs");
    }

    pub(crate) mod render {
        include!("../../src/author/render.rs");
    }
}

use author_harness::render::render;
use author_harness::state::{
    AuthoringSession, ConfirmationAction, FieldId, FieldKind, FieldStatus, Stage,
};

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn modified_key(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, modifiers))
}

fn press(session: &mut AuthoringSession<'_>, code: KeyCode) {
    assert!(session.handle_event(key(code)).is_none());
}

fn modified_press(session: &mut AuthoringSession<'_>, code: KeyCode, modifiers: KeyModifiers) {
    assert!(
        session
            .handle_event(modified_key(code, modifiers))
            .is_none()
    );
}

fn paste(session: &mut AuthoringSession<'_>, text: &str) {
    assert!(
        session
            .handle_event(Event::Paste(text.to_owned()))
            .is_none()
    );
}

fn set_document(session: &mut AuthoringSession<'_>, document: &str, cursor_line: u16) {
    session.composer.editor = TextArea::new(document.lines().map(str::to_owned).collect());
    session
        .composer
        .editor
        .move_cursor(CursorMove::Jump(cursor_line, 0));
    session.composer.issues.clear();
}

fn buffer_text(backend: &TestBackend) -> String {
    backend
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect()
}

fn rendered(
    session: &mut AuthoringSession<'_>,
    width: u16,
    height: u16,
) -> Result<String, Box<dyn Error>> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| render(frame, session))?;
    Ok(buffer_text(terminal.backend()))
}

fn rendered_buffer(
    session: &mut AuthoringSession<'_>,
    width: u16,
    height: u16,
) -> Result<Buffer, Box<dyn Error>> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| render(frame, session))?;
    Ok(terminal.backend().buffer().clone())
}

fn find_ascii(buffer: &Buffer, width: u16, height: u16, needle: &str) -> Option<(u16, u16)> {
    let characters = needle.chars().collect::<Vec<_>>();
    for y in 0..height {
        for x in 0..width {
            if characters.iter().enumerate().all(|(offset, character)| {
                u16::try_from(offset)
                    .ok()
                    .and_then(|offset| x.checked_add(offset))
                    .filter(|column| *column < width)
                    .is_some_and(|column| buffer[(column, y)].symbol() == character.to_string())
            }) {
                return Some((x, y));
            }
        }
    }
    None
}

fn find_ascii_on_row_from_right(
    buffer: &Buffer,
    width: u16,
    row: u16,
    needle: &str,
) -> Option<u16> {
    let characters = needle.chars().collect::<Vec<_>>();
    (0..width).rev().find(|x| {
        characters.iter().enumerate().all(|(offset, character)| {
            u16::try_from(offset)
                .ok()
                .and_then(|offset| x.checked_add(offset))
                .filter(|column| *column < width)
                .is_some_and(|column| buffer[(column, row)].symbol() == character.to_string())
        })
    })
}

fn row_text(buffer: &Buffer, width: u16, row: u16) -> String {
    (0..width)
        .map(|column| buffer[(column, row)].symbol())
        .collect()
}

fn reversed_positions(buffer: &Buffer, width: u16, height: u16) -> Vec<(u16, u16)> {
    (0..height)
        .flat_map(|row| (0..width).map(move |column| (column, row)))
        .filter(|position| buffer[*position].modifier.contains(Modifier::REVERSED))
        .collect()
}

fn terminal_edge_cursor_positions(buffer: &Buffer, width: u16, height: u16) -> Vec<(u16, u16)> {
    (0..height)
        .flat_map(|row| (0..width).map(move |column| (column, row)))
        .filter(|position| buffer[*position].symbol() == "█")
        .collect()
}

fn assert_blank_row(buffer: &Buffer, width: u16, row: u16) {
    assert!(
        (0..width).all(|column| buffer[(column, row)].symbol() == " "),
        "row {row} was not blank: {:?}",
        row_text(buffer, width, row)
    );
}

fn assert_inner_blank_row(buffer: &Buffer, width: u16, row: u16) {
    assert!(
        (1..width.saturating_sub(1)).all(|column| buffer[(column, row)].symbol() == " "),
        "inner row {row} was not blank: {:?}",
        row_text(buffer, width, row)
    );
}

fn assert_black_canvas(buffer: &Buffer) {
    for (index, cell) in buffer.content().iter().enumerate() {
        let width = usize::from(buffer.area.width);
        let x = index % width;
        let continuation = x > 0 && Line::from(buffer.content()[index - 1].symbol()).width() > 1;
        assert!(
            continuation || matches!(cell.bg, Color::Black | Color::Yellow),
            "unexpected canvas background {:?} on symbol {:?} at ({}, {})",
            cell.bg,
            cell.symbol(),
            x,
            index / width
        );
    }
    assert!(buffer.content().iter().any(|cell| cell.bg == Color::Black));
}

fn find_symbol(buffer: &Buffer, width: u16, height: u16, symbol: &str) -> Option<(u16, u16)> {
    (0..height)
        .flat_map(|row| (0..width).map(move |column| (column, row)))
        .find(|position| buffer[*position].symbol() == symbol)
}

fn assert_no_box_drawing(buffer: &Buffer, width: u16, height: u16) {
    const BOX_DRAWING: [&str; 11] = ["┌", "┐", "└", "┘", "│", "─", "├", "┤", "┬", "┴", "┼"];
    assert!(
        buffer
            .content()
            .iter()
            .all(|cell| !BOX_DRAWING.contains(&cell.symbol())),
        "rendered {width}x{height} buffer contained a box-drawing border"
    );
}

fn assert_bold_yellow_heading(
    buffer: &Buffer,
    width: u16,
    height: u16,
    heading: &str,
) -> Result<(u16, u16), Box<dyn Error>> {
    let position = find_ascii(buffer, width, height, heading).ok_or("missing section heading")?;
    for offset in 0..u16::try_from(heading.chars().count())? {
        let cell = &buffer[(position.0 + offset, position.1)];
        assert_eq!(cell.fg, Color::Yellow);
        assert!(cell.modifier.contains(Modifier::BOLD));
    }
    Ok(position)
}

fn assert_stage_header(
    session: &mut AuthoringSession<'_>,
    width: u16,
    height: u16,
    title: &str,
    step: &str,
) -> Result<(), Box<dyn Error>> {
    let buffer = rendered_buffer(session, width, height)?;
    assert_eq!(find_ascii(&buffer, width, height, title), Some((0, 0)));
    let position = find_ascii(&buffer, width, height, step).ok_or("missing step counter")?;
    assert_eq!(position.1, 0);
    assert!(position.0 >= width.saturating_sub(8));
    assert!((0..width).all(|column| !matches!(buffer[(column, 0)].symbol(), "─" | "┌" | "┐")));
    Ok(())
}

fn assert_highlighted_footer(
    session: &mut AuthoringSession<'_>,
    width: u16,
    height: u16,
) -> Result<(), Box<dyn Error>> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| render(frame, session))?;
    for column in 0..width {
        assert_eq!(
            terminal.backend().buffer()[(column, height - 1)].bg,
            Color::Yellow
        );
    }
    Ok(())
}

fn valid_feat_document() -> &'static str {
    "scope:\n\n\ndescription:\ncompose durable message\n\nintent:\nexplain intent 🦀\n\nbehavior:\nfirst line\nsecond line\n\nconstraints:\n\n\ninvariants:\n\n\nvalidation:"
}

fn valid_feat_session() -> AuthoringSession<'static> {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    set_document(&mut session, valid_feat_document(), 10);
    modified_press(&mut session, KeyCode::Char('s'), KeyModifiers::CONTROL);
    session
}

fn property(
    key: &str,
    requirement: PropertyRequirement,
    multiplicity: PropertyMultiplicity,
) -> Result<PropertyDefinition, Box<dyn Error>> {
    Ok(PropertyDefinition::new(
        PropertyKey::new(key)?,
        format!("Description for {key}."),
        requirement,
        multiplicity,
    )?)
}

fn presentation_definition() -> Result<CommitTypeDefinition, Box<dyn Error>> {
    Ok(CommitTypeDefinition::new(
        SchemaVersion::V1,
        CommitTypeId::new("custom")?,
        "Exercise every property presentation.",
        vec![
            property(
                "required-field",
                PropertyRequirement::Required,
                PropertyMultiplicity::Single,
            )?,
            property(
                "recommended-field",
                PropertyRequirement::Recommended,
                PropertyMultiplicity::Single,
            )?,
            property(
                "optional-field",
                PropertyRequirement::Optional,
                PropertyMultiplicity::Single,
            )?,
            property(
                "conditional-field",
                PropertyRequirement::Conditional(PropertyCondition::new(
                    ConditionId::new("when-needed")?,
                    "required when the condition applies",
                )?),
                PropertyMultiplicity::Single,
            )?,
        ],
    )?)
}

fn guidance_definition(description: &str) -> Result<CommitTypeDefinition, Box<dyn Error>> {
    Ok(CommitTypeDefinition::new(
        SchemaVersion::V1,
        CommitTypeId::new("custom")?,
        "Exercise guidance measurement.",
        vec![PropertyDefinition::new(
            PropertyKey::new("context")?,
            description,
            PropertyRequirement::Optional,
            PropertyMultiplicity::Single,
        )?],
    )?)
}

fn repeatable_definition() -> Result<CommitTypeDefinition, Box<dyn Error>> {
    Ok(CommitTypeDefinition::new(
        SchemaVersion::V1,
        CommitTypeId::new("custom")?,
        "Exercise repeatable values.",
        vec![property(
            "evidence",
            PropertyRequirement::Required,
            PropertyMultiplicity::Multiple,
        )?],
    )?)
}

#[test]
fn picker_navigation_wraps_selects_and_cancels() {
    let definitions = built_in_commit_types();
    let mut session = AuthoringSession::new(definitions, None);
    assert_eq!(session.stage, Stage::SelectType);

    press(&mut session, KeyCode::Up);
    assert_eq!(session.selected_type, definitions.len() - 1);
    press(&mut session, KeyCode::Down);
    assert_eq!(session.selected_type, 0);
    for unsupported in [
        KeyCode::Char('j'),
        KeyCode::Char('k'),
        KeyCode::Home,
        KeyCode::End,
    ] {
        press(&mut session, unsupported);
        assert_eq!(session.selected_type, 0);
    }
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.stage, Stage::Compose);
    assert_eq!(session.definition().id().as_str(), "feat");

    let mut cancelled = AuthoringSession::new(definitions, None);
    assert_eq!(
        cancelled.handle_event(key(KeyCode::Char('q'))),
        Some(CommitDraftAuthorOutcome::Cancelled)
    );
    let mut cancelled = AuthoringSession::new(definitions, None);
    assert_eq!(
        cancelled.handle_event(modified_key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        Some(CommitDraftAuthorOutcome::Cancelled)
    );
}

#[test]
fn schema_form_is_prepopulated_in_order_and_starts_under_scope() -> Result<(), Box<dyn Error>> {
    let definitions = vec![presentation_definition()?];
    let session = AuthoringSession::new(&definitions, Some(0));
    assert_eq!(session.stage, Stage::Compose);
    assert!(session.preselected);
    assert_eq!(session.composer.editor.cursor(), (3, 0));
    assert!(!session.composer.dirty());
    assert_eq!(
        session.composer.editor.lines(),
        [
            "Message Subject",
            "",
            "scope:",
            "",
            "",
            "description:",
            "",
            "",
            "Message Body",
            "",
            "required-field:",
            "",
            "",
            "recommended-field:",
            "",
            "",
            "optional-field:",
            "",
            "",
            "conditional-field:",
            "",
            "",
            "Message Footer",
            "",
        ]
    );
    Ok(())
}

#[test]
fn invalid_review_marks_every_blocker_and_moves_to_the_first() {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    modified_press(&mut session, KeyCode::Char('s'), KeyModifiers::CONTROL);

    assert_eq!(session.stage, Stage::Compose);
    assert_eq!(session.composer.editor.cursor(), (6, 0));
    assert_eq!(session.composer.issues.len(), 3);
    for field in [
        FieldId::Description,
        FieldId::Property(0),
        FieldId::Property(1),
    ] {
        assert!(
            session
                .composer
                .issues
                .iter()
                .any(|issue| issue.field == Some(field))
        );
    }
    assert!(session.review.is_none());
}

#[test]
fn review_is_exact_backtracking_is_lossless_and_enter_returns_the_typed_draft()
-> Result<(), Box<dyn Error>> {
    let mut session = valid_feat_session();
    let expected = "feat: compose durable message\n\nintent:\nexplain intent 🦀\n\nbehavior:\nfirst line\nsecond line\n";
    assert_eq!(session.stage, Stage::Review);
    assert_eq!(
        session
            .review
            .as_ref()
            .ok_or("missing review")?
            .message
            .as_str(),
        expected
    );

    press(&mut session, KeyCode::Esc);
    assert_eq!(session.stage, Stage::Compose);
    assert_eq!(
        session.composer.editor.lines().join("\n"),
        valid_feat_document()
    );
    modified_press(&mut session, KeyCode::Char('s'), KeyModifiers::CONTROL);
    let outcome = session
        .handle_event(key(KeyCode::Enter))
        .ok_or("review confirmation did not finish")?;
    let CommitDraftAuthorOutcome::Authored(draft) = outcome else {
        return Err("review unexpectedly cancelled".into());
    };
    assert_eq!(
        render_commit_message(&built_in_commit_types()[0], &draft)?.as_str(),
        expected
    );
    Ok(())
}

#[test]
fn short_review_locks_arrow_and_page_scrolling() -> Result<(), Box<dyn Error>> {
    let mut session = valid_feat_session();
    let _ = rendered_buffer(&mut session, 100, 24)?;
    assert_eq!(session.review.as_ref().map(|review| review.scroll), Some(0));
    assert_eq!(
        session.review.as_ref().map(|review| review.scrollable),
        Some(false)
    );

    for locked in [
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::Char('j'),
        KeyCode::Char('k'),
    ] {
        press(&mut session, locked);
        assert_eq!(session.review.as_ref().map(|review| review.scroll), Some(0));
    }
    Ok(())
}

#[test]
fn long_review_scrolls_until_a_taller_viewport_locks_it() -> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    let behavior = (1..=30)
        .map(|line| format!("behavior line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let document = valid_feat_document().replace("first line\nsecond line", &behavior);
    set_document(&mut session, &document, 10);
    modified_press(&mut session, KeyCode::Char('s'), KeyModifiers::CONTROL);

    let _ = rendered_buffer(&mut session, 100, 24)?;
    assert_eq!(
        session.review.as_ref().map(|review| review.scrollable),
        Some(true)
    );
    press(&mut session, KeyCode::Down);
    assert_eq!(session.review.as_ref().map(|review| review.scroll), Some(1));
    press(&mut session, KeyCode::Up);
    assert_eq!(session.review.as_ref().map(|review| review.scroll), Some(0));
    press(&mut session, KeyCode::PageDown);
    assert_eq!(
        session.review.as_ref().map(|review| review.scroll),
        Some(10)
    );
    press(&mut session, KeyCode::PageUp);
    assert_eq!(session.review.as_ref().map(|review| review.scroll), Some(0));

    press(&mut session, KeyCode::PageDown);
    let _ = rendered_buffer(&mut session, 100, 60)?;
    assert_eq!(
        session.review.as_ref().map(|review| review.scrollable),
        Some(false)
    );
    assert_eq!(session.review.as_ref().map(|review| review.scroll), Some(0));
    press(&mut session, KeyCode::Down);
    press(&mut session, KeyCode::PageDown);
    assert_eq!(session.review.as_ref().map(|review| review.scroll), Some(0));
    Ok(())
}

#[test]
fn review_trims_trailing_whitespace_from_every_encoded_field() -> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    let document = "scope:\napi \t\n\ndescription:\nship it  \n\nintent:\n  leading reason \t\n\nbehavior:\nfirst  \nsecond\t\n";
    set_document(&mut session, document, 10);

    modified_press(&mut session, KeyCode::Char('s'), KeyModifiers::CONTROL);

    assert_eq!(session.stage, Stage::Review);
    assert_eq!(
        session
            .review
            .as_ref()
            .ok_or("missing review")?
            .message
            .as_str(),
        "feat(api): ship it\n\nintent:\n  leading reason\n\nbehavior:\nfirst\nsecond\n"
    );
    press(&mut session, KeyCode::Esc);
    assert_eq!(
        session.composer.editor.lines().join("\n"),
        document.strip_suffix('\n').ok_or("missing final newline")?
    );
    Ok(())
}

#[test]
fn scope_and_description_reject_multiline_input_and_enter_advances() {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    let pristine = session.composer.editor.lines().to_vec();

    paste(
        &mut session,
        "api
server",
    );
    assert_eq!(session.composer.editor.lines(), pristine);
    paste(&mut session, "api");
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.composer.editor.cursor(), (6, 0));

    paste(
        &mut session,
        "ship
change",
    );
    assert!(session.composer.editor.lines()[6].is_empty());
    paste(&mut session, "ship change");
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.composer.editor.cursor(), (11, 0));

    paste(
        &mut session,
        "first
second",
    );
    assert_eq!(
        &session.composer.editor.lines()[11..=12],
        ["first", "second"]
    );
}

#[test]
fn conventional_document_editing_preserves_ctrl_k_unicode_paste_and_history() {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    paste(&mut session, "alpha beta 🦀");
    let end = session.composer.editor.cursor();
    modified_press(&mut session, KeyCode::Left, KeyModifiers::CONTROL);
    assert!(session.composer.editor.cursor().1 < end.1);

    modified_press(&mut session, KeyCode::Char('k'), KeyModifiers::CONTROL);
    assert_eq!(session.composer.editor.lines()[3], "alpha beta ");
    modified_press(&mut session, KeyCode::Char('u'), KeyModifiers::CONTROL);
    assert_eq!(session.composer.editor.lines()[3], "alpha beta 🦀");
    modified_press(&mut session, KeyCode::Char('r'), KeyModifiers::CONTROL);
    assert_eq!(session.composer.editor.lines()[3], "alpha beta ");

    session.composer.editor.move_cursor(CursorMove::Jump(11, 0));
    paste(&mut session, "line one\nline two");
    assert!(
        session
            .composer
            .editor
            .lines()
            .iter()
            .any(|line| line == "line two")
    );
}

#[test]
fn ctrl_t_is_inert_and_conventional_input_remains_active() {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    let pristine = session.composer.editor.lines().to_vec();
    let cursor = session.composer.editor.cursor();

    modified_press(&mut session, KeyCode::Char('t'), KeyModifiers::CONTROL);
    assert_eq!(session.composer.editor.lines(), pristine);
    assert_eq!(session.composer.editor.cursor(), cursor);
    assert!(!session.composer.dirty());

    paste(&mut session, "still editing");
    assert_eq!(session.composer.editor.lines()[3], "still editing");
}

#[test]
fn schema_headings_are_immutable_during_conventional_editing() -> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    let pristine = session.composer.editor.lines().to_vec();
    let description_line = pristine
        .iter()
        .position(|line| line == "description:")
        .ok_or("description heading")?;
    let description_line = u16::try_from(description_line)?;

    session
        .composer
        .editor
        .move_cursor(CursorMove::Jump(description_line, 0));
    press(&mut session, KeyCode::Right);
    assert_eq!(
        session.composer.editor.cursor(),
        (usize::from(description_line.saturating_add(1)), 0)
    );
    session
        .composer
        .editor
        .move_cursor(CursorMove::Jump(description_line, 0));
    press(&mut session, KeyCode::Char('x'));
    assert_eq!(session.composer.editor.lines(), pristine);

    press(&mut session, KeyCode::Delete);
    assert_eq!(session.composer.editor.lines(), pristine);

    session
        .composer
        .editor
        .move_cursor(CursorMove::Jump(description_line.saturating_add(1), 0));
    press(&mut session, KeyCode::Backspace);
    assert_eq!(session.composer.editor.lines(), pristine);

    paste(&mut session, "scope:\n");
    assert_eq!(session.composer.editor.lines(), pristine);

    session.composer.editor.start_selection();
    session
        .composer
        .editor
        .move_cursor(CursorMove::Jump(description_line, 0));
    press(&mut session, KeyCode::Backspace);
    assert_eq!(session.composer.editor.lines(), pristine);

    Ok(())
}

#[test]
fn conventional_navigation_skips_schema_headings() {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    assert_eq!(session.composer.editor.cursor(), (3, 0));

    press(&mut session, KeyCode::Down);
    assert_eq!(session.composer.editor.cursor(), (6, 0));
    press(&mut session, KeyCode::Up);
    assert_eq!(session.composer.editor.cursor(), (3, 0));
    press(&mut session, KeyCode::Right);
    assert_eq!(session.composer.editor.cursor(), (6, 0));
    press(&mut session, KeyCode::Left);
    assert_eq!(session.composer.editor.cursor(), (3, 0));
}

#[test]
fn vertical_arrows_move_within_multiline_fields_then_cross_field_boundaries()
-> Result<(), Box<dyn Error>> {
    let definitions = vec![presentation_definition()?];
    let mut session = AuthoringSession::new(&definitions, Some(0));

    for expected in [(6, 0), (11, 0), (14, 0), (17, 0), (20, 0)] {
        press(&mut session, KeyCode::Down);
        assert_eq!(session.composer.editor.cursor(), expected);
    }
    press(&mut session, KeyCode::Down);
    assert_eq!(session.composer.editor.cursor(), (20, 0));
    for expected in [(17, 0), (14, 0), (11, 0), (6, 0), (3, 0)] {
        press(&mut session, KeyCode::Up);
        assert_eq!(session.composer.editor.cursor(), expected);
    }
    assert!(!session.composer.dirty());

    session.composer.editor.move_cursor(CursorMove::Jump(11, 0));
    paste(
        &mut session,
        "first
second",
    );
    press(&mut session, KeyCode::Up);
    assert_eq!(session.composer.editor.cursor(), (11, 5));
    press(&mut session, KeyCode::Down);
    assert_eq!(session.composer.editor.cursor(), (12, 5));
    press(&mut session, KeyCode::Down);
    assert_eq!(
        session.composer.current_field(&definitions[0]),
        Some(FieldKind::Property {
            definition_index: 1,
            value_index: 0,
        })
    );
    press(&mut session, KeyCode::Up);
    assert_eq!(session.composer.editor.cursor(), (12, 0));
    Ok(())
}

#[test]
fn vertical_arrows_follow_soft_wrapped_visual_rows() -> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    paste(&mut session, &"x".repeat(81));
    let _ = rendered_buffer(&mut session, 72, 24)?;
    let wrapped_end = session.composer.editor.cursor();

    press(&mut session, KeyCode::Up);
    let previous_visual_row = session.composer.editor.cursor();
    assert_eq!(previous_visual_row.0, wrapped_end.0);
    assert!(previous_visual_row.1 < wrapped_end.1);

    press(&mut session, KeyCode::Down);
    assert_eq!(session.composer.editor.cursor(), wrapped_end);
    Ok(())
}

#[test]
fn enter_advances_single_line_and_blank_fields_without_editing_the_document()
-> Result<(), Box<dyn Error>> {
    let definitions = vec![presentation_definition()?];
    let mut blank = AuthoringSession::new(&definitions, Some(0));
    let pristine = blank.composer.editor.lines().to_vec();

    for expected in [(6, 0), (11, 0), (14, 0), (17, 0), (20, 0)] {
        press(&mut blank, KeyCode::Enter);
        assert_eq!(blank.composer.editor.cursor(), expected);
        assert_eq!(blank.composer.editor.lines(), pristine);
        assert!(!blank.composer.dirty());
    }
    press(&mut blank, KeyCode::Enter);
    assert_eq!(blank.composer.editor.cursor(), (20, 0));

    let mut populated = AuthoringSession::new(&definitions, Some(0));
    paste(&mut populated, "api");
    press(&mut populated, KeyCode::Enter);
    assert_eq!(populated.composer.editor.cursor(), (6, 0));
    paste(&mut populated, "ship change");
    press(&mut populated, KeyCode::Enter);
    assert_eq!(populated.composer.editor.cursor(), (11, 0));
    paste(&mut populated, "first line");
    press(&mut populated, KeyCode::Enter);
    assert_eq!(populated.composer.editor.cursor(), (12, 0));
    assert_eq!(populated.composer.editor.lines()[11], "first line");
    assert!(populated.composer.editor.lines()[12].is_empty());
    assert_eq!(populated.composer.editor.lines()[14], "recommended-field:");
    Ok(())
}

#[test]
fn backspace_and_delete_join_an_explicit_second_line_and_preserve_its_separator() {
    for key in [KeyCode::Backspace, KeyCode::Delete] {
        let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
        set_document(
            &mut session,
            "scope:\n\n\ndescription:\n\n\nintent:\n\nmoves up\n\nbehavior:\n\n\nconstraints:\n\n\ninvariants:\n\n\nvalidation:\n\n\n",
            8,
        );
        press(&mut session, KeyCode::Home);

        press(&mut session, key);

        assert_eq!(session.composer.editor.cursor(), (8, 0));
        assert_eq!(session.composer.editor.lines()[8], "moves up");
        assert!(session.composer.editor.lines()[9].is_empty());
        assert_eq!(session.composer.editor.lines()[10], "behavior:");

        modified_press(&mut session, KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert!(session.composer.editor.lines()[7].is_empty());
        assert_eq!(session.composer.editor.lines()[8], "moves up");
        modified_press(&mut session, KeyCode::Char('r'), KeyModifiers::CONTROL);
        assert_eq!(session.composer.editor.lines()[8], "moves up");
        assert!(session.composer.editor.lines()[9].is_empty());
    }
}

#[test]
fn multiple_schema_values_still_compile_when_present() -> Result<(), Box<dyn Error>> {
    let definitions = vec![repeatable_definition()?];
    let mut session = AuthoringSession::new(&definitions, Some(0));
    set_document(
        &mut session,
        "scope:\n\ndescription:\ncollect evidence\n\nevidence:\nfirst\n\nevidence:\nsecond\nline\n",
        9,
    );

    assert_eq!(
        session.composer.current_field(&definitions[0]),
        Some(FieldKind::Property {
            definition_index: 0,
            value_index: 1,
        })
    );
    modified_press(&mut session, KeyCode::Char('s'), KeyModifiers::CONTROL);
    assert_eq!(session.stage, Stage::Review);
    assert_eq!(
        session
            .review
            .as_ref()
            .ok_or("missing review")?
            .message
            .as_str(),
        "custom: collect evidence\n\nevidence:\nfirst\n\nevidence:\nsecond\nline\n"
    );
    Ok(())
}

#[test]
fn ctrl_n_and_ctrl_d_use_conventional_editor_behavior_without_changing_the_schema() {
    let definitions = built_in_commit_types();
    let mut session = AuthoringSession::new(definitions, Some(0));
    let headings = session
        .composer
        .editor
        .lines()
        .iter()
        .filter(|line| line.ends_with(':'))
        .cloned()
        .collect::<Vec<_>>();

    paste(&mut session, "abc");
    session.composer.editor.move_cursor(CursorMove::Head);
    modified_press(&mut session, KeyCode::Char('d'), KeyModifiers::CONTROL);
    assert_eq!(session.composer.editor.lines()[3], "bc");
    modified_press(&mut session, KeyCode::Char('n'), KeyModifiers::CONTROL);

    assert_eq!(
        session
            .composer
            .editor
            .lines()
            .iter()
            .filter(|line| line.ends_with(':'))
            .cloned()
            .collect::<Vec<_>>(),
        headings
    );
}

#[test]
fn parser_rejects_unknown_malformed_and_duplicate_single_headers() -> Result<(), Box<dyn Error>> {
    let definitions = vec![presentation_definition()?];
    let documents = [
        "scope:\n\ndescription:\nvalid\n\nrequired-field:\ncomplete\n\nmystery:\nvalue\n",
        "scope:\n\ndescription:\nvalid\n\nrequired-field\ncomplete\n",
        "scope:\n\ndescription:\nvalid\n\ndescription:\nduplicate\n\nrequired-field:\ncomplete\n",
        "scope:\n\ndescription:\nvalid\n\nrequired-field:\ncomplete\n\nrequired-field:\nduplicate\n",
    ];
    for document in documents {
        let mut session = AuthoringSession::new(&definitions, Some(0));
        set_document(&mut session, document, 0);
        modified_press(&mut session, KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(session.stage, Stage::Compose);
        assert!(!session.composer.issues.is_empty());
        assert!(session.review.is_none());
    }

    let mut indented_heading = AuthoringSession::new(&definitions, Some(0));
    set_document(
        &mut indented_heading,
        "scope:\n\ndescription:\nvalid\n\nrequired-field:\n  note:\n  value\n",
        6,
    );
    modified_press(
        &mut indented_heading,
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
    );
    assert_eq!(indented_heading.stage, Stage::Review);
    Ok(())
}

#[test]
fn hud_tracks_requirement_completion_validation_and_cursor_context() -> Result<(), Box<dyn Error>> {
    let definitions = vec![presentation_definition()?];
    let mut session = AuthoringSession::new(&definitions, Some(0));
    set_document(
        &mut session,
        "scope:\n\ndescription:\ncover contracts\n\nrequired-field:\ncomplete\n\nrecommended-field:\n\n\noptional-field:\n\n\nconditional-field:\n\n",
        16,
    );
    assert_eq!(
        session.composer.current_field(&definitions[0]),
        Some(FieldKind::Property {
            definition_index: 3,
            value_index: 0,
        })
    );
    let hud = session.composer.hud_fields(&definitions[0]);
    assert_eq!(hud[0].status, FieldStatus::Incomplete);
    assert_eq!(hud[1].status, FieldStatus::Complete);
    assert_eq!(hud[2].status, FieldStatus::Complete);
    assert!(
        hud[3..]
            .iter()
            .all(|field| field.status == FieldStatus::Incomplete)
    );

    modified_press(&mut session, KeyCode::Char('s'), KeyModifiers::CONTROL);
    assert_eq!(session.stage, Stage::Review);
    assert_eq!(
        session
            .review
            .as_ref()
            .ok_or("missing review")?
            .message
            .as_str(),
        "custom: cover contracts\n\nrequired-field:\ncomplete\n"
    );

    press(&mut session, KeyCode::Esc);
    set_document(
        &mut session,
        "scope:\n\ndescription:\ntwo\nlines\n\nrequired-field:\ncomplete\n",
        4,
    );
    let hud = session.composer.hud_fields(&definitions[0]);
    assert_eq!(hud[1].status, FieldStatus::Invalid);
    Ok(())
}

#[test]
fn untouched_and_dirty_cancellation_follow_distinct_confirmation_paths() {
    let definitions = built_in_commit_types();
    let mut unpinned = AuthoringSession::new(definitions, None);
    press(&mut unpinned, KeyCode::Enter);
    assert!(!unpinned.composer.dirty());
    press(&mut unpinned, KeyCode::Esc);
    assert_eq!(unpinned.stage, Stage::SelectType);

    press(&mut unpinned, KeyCode::Enter);
    paste(&mut unpinned, "dirty");
    press(&mut unpinned, KeyCode::Esc);
    assert_eq!(unpinned.stage, Stage::Confirm);
    assert_eq!(unpinned.confirmation, ConfirmationAction::ChangeType);
    press(&mut unpinned, KeyCode::Enter);
    assert_eq!(unpinned.stage, Stage::Compose);
    assert_eq!(unpinned.composer.editor.lines()[3], "dirty");
    press(&mut unpinned, KeyCode::Esc);
    press(&mut unpinned, KeyCode::Char('y'));
    assert_eq!(unpinned.stage, Stage::SelectType);
    assert!(!unpinned.composer.dirty());

    let mut pinned = AuthoringSession::new(definitions, Some(0));
    assert_eq!(
        pinned.handle_event(key(KeyCode::Esc)),
        Some(CommitDraftAuthorOutcome::Cancelled)
    );
    let mut pinned = AuthoringSession::new(definitions, Some(0));
    paste(&mut pinned, "dirty");
    press(&mut pinned, KeyCode::Esc);
    assert_eq!(pinned.confirmation, ConfirmationAction::Cancel);
    assert_eq!(
        pinned.handle_event(key(KeyCode::Char('y'))),
        Some(CommitDraftAuthorOutcome::Cancelled)
    );
}

#[test]
fn review_cancellation_confirmation_can_resume_without_losing_the_preview()
-> Result<(), Box<dyn Error>> {
    let mut session = valid_feat_session();
    let message = session
        .review
        .as_ref()
        .ok_or("missing review")?
        .message
        .clone();
    press(&mut session, KeyCode::Char('q'));
    assert_eq!(session.stage, Stage::Confirm);
    press(&mut session, KeyCode::Char('n'));
    assert_eq!(session.stage, Stage::Review);
    assert_eq!(
        session.review.as_ref().ok_or("missing review")?.message,
        message
    );
    Ok(())
}

#[test]
fn every_stage_hud_footer_and_responsive_boundary_render() -> Result<(), Box<dyn Error>> {
    let mut picker = AuthoringSession::new(built_in_commit_types(), None);
    let text = rendered(&mut picker, 100, 24)?;
    assert!(text.contains("Select commit type"));
    assert!(!text.contains("gitserious commit"));
    assert!(!text.contains("Choose the semantic contract for this commit."));
    assert!(!text.contains("Commit types"));
    assert_eq!(
        text.matches("An intentional addition or expansion of capability.")
            .count(),
        1
    );
    assert!(text.contains("enter: select"));
    assert_stage_header(&mut picker, 100, 24, "Select commit type", "Step 1/3")?;
    let picker_buffer = rendered_buffer(&mut picker, 100, 24)?;
    assert_blank_row(&picker_buffer, 100, 1);
    assert_no_box_drawing(&picker_buffer, 100, 24);
    assert_black_canvas(&picker_buffer);
    let selected = find_ascii(&picker_buffer, 100, 24, "› feat").ok_or("missing selection")?;
    assert_eq!(selected, (0, 2));
    assert_highlighted_footer(&mut picker, 100, 24)?;

    let definitions = vec![presentation_definition()?];
    let mut composer = AuthoringSession::new(&definitions, Some(0));
    composer
        .composer
        .editor
        .move_cursor(CursorMove::Jump(20, 0));
    let text = rendered(&mut composer, 120, 32)?;
    assert!(text.contains("Compose commit message"));
    assert!(text.contains("Type: custom"));
    assert!(!text.contains("Keymap:"));
    assert!(text.contains("○  description"));
    assert!(text.contains("conditional-field"));
    assert!(text.contains("conditional"));
    assert!(text.contains("required when the condition"));
    assert!(text.contains("applies"));
    assert!(!text.contains("ctrl+t"));
    assert!(!text.contains("Complete every required field before review."));
    assert!(!text.contains("repeatable"));
    assert_stage_header(&mut composer, 120, 32, "Compose commit message", "Step 2/3")?;
    let composer_buffer = rendered_buffer(&mut composer, 120, 32)?;
    assert_blank_row(&composer_buffer, 120, 2);
    for heading in ["Message Properties", "Property Description"] {
        assert_bold_yellow_heading(&composer_buffer, 120, 32, heading)?;
    }
    assert!(find_ascii(&composer_buffer, 120, 32, "Message form").is_none());
    assert_black_canvas(&composer_buffer);
    assert_highlighted_footer(&mut composer, 120, 32)?;

    modified_press(&mut composer, KeyCode::Char('s'), KeyModifiers::CONTROL);
    let invalid_buffer = rendered_buffer(&mut composer, 120, 32)?;
    let text = invalid_buffer
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(text.contains("!  description"));
    assert!(text.contains("!  required-field"));
    assert!(!text.contains("Error:"));
    assert!(find_ascii(&invalid_buffer, 120, 32, "Message form").is_none());
    let issue = composer
        .composer
        .issues
        .first()
        .ok_or("missing validation issue")?;
    let issue_position = find_ascii(&invalid_buffer, 120, 32, issue.message.as_str())
        .ok_or("missing validation status")?;
    assert_eq!(issue_position.1, 29);
    assert_eq!(invalid_buffer[issue_position].fg, Color::Red);

    let mut review = valid_feat_session();
    let text = rendered(&mut review, 100, 24)?;
    assert!(text.contains("Review and commit"));
    assert!(!text.contains("Review the exact canonical message before Git creates the commit."));
    assert!(text.contains("feat: compose durable message"));
    assert!(text.contains("enter: commit"));
    assert_stage_header(&mut review, 100, 24, "Review and commit", "Step 3/3")?;
    let review_buffer = rendered_buffer(&mut review, 100, 24)?;
    assert_eq!(
        find_ascii(&review_buffer, 100, 24, "feat: compose durable message"),
        Some((0, 3)),
    );
    assert_blank_row(&review_buffer, 100, 1);
    assert_bold_yellow_heading(&review_buffer, 100, 24, "Commit message")?;
    assert_no_box_drawing(&review_buffer, 100, 24);
    assert_black_canvas(&review_buffer);
    assert_highlighted_footer(&mut review, 100, 24)?;
    Ok(())
}

#[test]
fn confirmation_and_too_small_views_use_centered_borderless_groups() -> Result<(), Box<dyn Error>> {
    let mut review = valid_feat_session();
    press(&mut review, KeyCode::Char('q'));
    let text = rendered(&mut review, 100, 24)?;
    assert!(text.contains("Confirm discard"));
    assert!(text.contains("Discard this draft and cancel"));
    assert!(text.contains("y: discard | enter/esc/n: keep editing"));
    assert!(text.contains("Review and commit"));
    assert!(text.contains("Step 3/3"));
    let buffer = rendered_buffer(&mut review, 100, 24)?;
    let title = find_ascii(&buffer, 100, 24, "Confirm discard").ok_or("missing dialog title")?;
    let question = "Discard this draft and cancel the commit?";
    let question_position =
        find_ascii(&buffer, 100, 24, question).ok_or("missing centered question")?;
    let hint = "y: discard | enter/esc/n: keep editing";
    let hint_position = find_ascii(&buffer, 100, 24, hint).ok_or("missing centered hint")?;
    let popup_left = (100_u16 - 54) / 2;
    let centered_x = |text: &str| {
        popup_left
            + (54_u16.saturating_sub(u16::try_from(Line::from(text).width()).unwrap_or(u16::MAX)))
                / 2
    };
    assert!(question_position.0.abs_diff(centered_x(question)) <= 1);
    assert_eq!(question_position.1, title.1 + 2);
    assert!(hint_position.0.abs_diff(centered_x(hint)) <= 1);
    assert_eq!(hint_position.1, title.1 + 4);
    assert!(
        (popup_left..popup_left + 54).all(|column| buffer[(column, title.1 + 1)].symbol() == " ")
    );
    assert!(
        (popup_left..popup_left + 54).all(|column| buffer[(column, title.1 + 3)].symbol() == " ")
    );
    assert_bold_yellow_heading(&buffer, 100, 24, "Confirm discard")?;
    assert_no_box_drawing(&buffer, 100, 24);
    assert_black_canvas(&buffer);

    for (width, height) in [(59, 24), (100, 17), (60, 20)] {
        let mut too_small = AuthoringSession::new(built_in_commit_types(), Some(0));
        let buffer = rendered_buffer(&mut too_small, width, height)?;
        let text = buffer
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();
        assert!(too_small.too_small);
        assert!(text.contains("Terminal too small"));
        assert!(text.contains("esc/q to cancel"));
        assert_bold_yellow_heading(&buffer, width, height, "Terminal too small")?;
        assert_no_box_drawing(&buffer, width, height);
        assert_black_canvas(&buffer);
        assert_eq!(
            too_small.handle_event(key(KeyCode::Esc)),
            Some(CommitDraftAuthorOutcome::Cancelled)
        );
    }

    let mut small_confirmation = AuthoringSession::new(built_in_commit_types(), Some(0));
    paste(&mut small_confirmation, "dirty");
    press(&mut small_confirmation, KeyCode::Esc);
    let buffer = rendered_buffer(&mut small_confirmation, 59, 24)?;
    assert!(small_confirmation.too_small);
    assert_bold_yellow_heading(&buffer, 59, 24, "Confirm discard")?;
    assert!(find_ascii(&buffer, 59, 24, "Discard this draft?").is_some());
    assert!(find_ascii(&buffer, 59, 24, "y: discard | enter/n: keep editing").is_some());
    assert_no_box_drawing(&buffer, 59, 24);
    assert_black_canvas(&buffer);
    Ok(())
}

#[test]
fn editor_and_navigation_styles_match_terminal_editor_conventions() -> Result<(), Box<dyn Error>> {
    let mut composer = AuthoringSession::new(built_in_commit_types(), Some(0));
    assert_eq!(
        composer.composer.editor.cursor_line_style(),
        Style::default()
    );
    assert_eq!(
        composer.composer.editor.cursor_style(),
        Style::default().add_modifier(Modifier::REVERSED)
    );
    assert_eq!(
        composer.composer.editor.cursor_render_mode(),
        CursorRenderMode::Hidden
    );
    paste(&mut composer, "hello");
    let buffer = rendered_buffer(&mut composer, 120, 32)?;
    assert_eq!(reversed_positions(&buffer, 120, 32).len(), 1);
    let scope = find_ascii(&buffer, 120, 32, "scope:").ok_or("missing scope label")?;
    for offset in 0..u16::try_from("scope:".len())? {
        let cell = &buffer[(scope.0 + offset, scope.1)];
        assert_eq!(cell.fg, Color::Yellow);
        assert!(cell.modifier.contains(Modifier::BOLD));
        assert!(!cell.modifier.contains(Modifier::UNDERLINED));
    }
    let authored = find_ascii(&buffer, 120, 32, "hello").ok_or("missing authored value")?;
    assert!(!buffer[authored].modifier.contains(Modifier::UNDERLINED));

    let footer = row_text(&buffer, 120, 31);
    assert!(footer.contains("↑/↓: move | esc: back | ctrl+s: review"));
    assert!(footer.contains("▌ col 6/80 "));
    assert!(!footer.contains("Ctrl"));
    assert!(!footer.contains("ctrl+n"));
    assert!(!footer.contains("ctrl+d"));
    let status = find_ascii(&buffer, 120, 32, "col 6/80").ok_or("missing column status")?;
    let padding = [(status.0 - 1, status.1), (status.0 + 8, status.1)];
    assert_eq!(buffer[(status.0 - 2, status.1)].symbol(), "▌");
    for position in padding {
        assert_eq!(buffer[position].symbol(), " ");
        assert_eq!(buffer[position].fg, Color::Black);
        assert_eq!(buffer[position].bg, Color::Yellow);
    }
    assert_eq!(buffer[(status.0 - 2, status.1)].fg, Color::Black);
    assert_eq!(buffer[(status.0 - 2, status.1)].bg, Color::Yellow);
    let key = find_ascii(&buffer, 120, 32, "ctrl+s").ok_or("missing key hint")?;
    let action = find_ascii(&buffer, 120, 32, "review").ok_or("missing action hint")?;
    assert!(buffer[key].modifier.contains(Modifier::BOLD));
    assert!(!buffer[action].modifier.contains(Modifier::BOLD));
    assert_eq!(buffer[key].bg, Color::Yellow);
    assert_eq!(buffer[action].bg, Color::Yellow);

    let mut picker = AuthoringSession::new(built_in_commit_types(), None);
    let buffer = rendered_buffer(&mut picker, 100, 24)?;
    let footer = row_text(&buffer, 100, 23);
    assert!(footer.contains("↑/↓: move | enter: select | esc/q: cancel"));
    assert!(!footer.contains("/j"));
    assert!(!footer.contains("/k"));
    assert!(!footer.contains("Home/End"));

    let mut review = valid_feat_session();
    let buffer = rendered_buffer(&mut review, 100, 24)?;
    let footer = row_text(&buffer, 100, 23);
    assert!(footer.contains("enter: commit | esc: edit | ↑/↓: scroll | q/ctrl+c: cancel"));
    assert!(!footer.contains("Ctrl"));
    assert!(!footer.contains('·'));
    Ok(())
}

#[test]
fn returning_to_scope_restores_subject_context_after_vertical_scrolling()
-> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    let pristine = session.composer.editor.lines().to_vec();
    let mut buffer = rendered_buffer(&mut session, 72, 24)?;

    for _ in 0..6 {
        press(&mut session, KeyCode::Down);
        buffer = rendered_buffer(&mut session, 72, 24)?;
    }
    assert_eq!(session.composer.editor.cursor(), (23, 0));
    assert!(find_ascii(&buffer, 72, 24, "Message Subject").is_none());

    for _ in 0..6 {
        press(&mut session, KeyCode::Up);
        buffer = rendered_buffer(&mut session, 72, 24)?;
    }

    assert_eq!(session.composer.editor.cursor(), (3, 0));
    assert_eq!(session.composer.editor.lines(), pristine);
    assert!(!session.composer.dirty());
    let subject =
        find_ascii(&buffer, 72, 24, "Message Subject").ok_or("missing subject context")?;
    let scope = find_ascii(&buffer, 72, 24, "scope:").ok_or("missing scope context")?;
    assert_eq!(subject.0, 1);
    assert_eq!(scope, (1, subject.1 + 2));
    assert_eq!(reversed_positions(&buffer, 72, 24), [(1, scope.1 + 1)]);
    assert_eq!(terminal_edge_cursor_positions(&buffer, 72, 24), []);
    Ok(())
}

#[test]
fn oversized_scope_keeps_its_cursor_and_document_when_subject_context_cannot_fit()
-> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    let scope = "x".repeat(481);
    paste(&mut session, &scope);
    let cursor = session.composer.editor.cursor();
    let document = session.composer.editor.lines().to_vec();

    let buffer = rendered_buffer(&mut session, 72, 24)?;

    assert_eq!(session.composer.editor.cursor(), cursor);
    assert_eq!(session.composer.editor.lines(), document);
    assert_eq!(session.composer.editor.lines()[3], scope);
    assert!(find_ascii(&buffer, 72, 24, "Message Subject").is_none());
    assert!(find_ascii(&buffer, 72, 24, "col 2/80").is_some());
    assert_eq!(reversed_positions(&buffer, 72, 24).len(), 1);
    Ok(())
}

#[test]
fn composer_renders_one_cell_cursor_without_terminal_edge_bleed() -> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    let mut buffer = rendered_buffer(&mut session, 120, 32)?;
    assert_eq!(reversed_positions(&buffer, 120, 32).len(), 1);
    assert_eq!(terminal_edge_cursor_positions(&buffer, 120, 32), []);

    paste(&mut session, "a");
    buffer = rendered_buffer(&mut session, 120, 32)?;
    assert_eq!(session.composer.editor.cursor(), (3, 1));
    assert_eq!(reversed_positions(&buffer, 120, 32).len(), 1);
    assert_eq!(terminal_edge_cursor_positions(&buffer, 120, 32), []);

    for expected_line in [6, 11, 14, 17, 20, 23] {
        press(&mut session, KeyCode::Down);
        buffer = rendered_buffer(&mut session, 120, 32)?;
        assert_eq!(session.composer.editor.cursor(), (expected_line, 0));
        assert_eq!(reversed_positions(&buffer, 120, 32).len(), 1);
        assert_eq!(terminal_edge_cursor_positions(&buffer, 120, 32), []);
    }
    Ok(())
}

#[test]
fn composer_uses_one_framed_context_editor_and_validation_surface() -> Result<(), Box<dyn Error>> {
    let mut wide = AuthoringSession::new(built_in_commit_types(), Some(0));
    let pristine = wide.composer.editor.lines().to_vec();
    let buffer = rendered_buffer(&mut wide, 120, 32)?;
    let properties =
        find_ascii(&buffer, 120, 32, "Message Properties").ok_or("missing properties heading")?;
    let description = find_ascii(&buffer, 120, 32, "Property Description")
        .ok_or("missing description heading")?;
    let guidance =
        find_ascii(&buffer, 120, 32, "Optional affected area").ok_or("missing guidance")?;

    assert_eq!(properties, (1, 4));
    assert_eq!(description, (48, 4));
    assert_blank_row(&buffer, 120, 2);
    assert_eq!(buffer[(0, 3)].symbol(), "┌");
    assert_eq!(buffer[(47, 3)].symbol(), "┬");
    assert_eq!(buffer[(119, 3)].symbol(), "┐");
    assert_eq!(buffer[(0, 5)].symbol(), "├");
    assert_eq!(buffer[(47, 5)].symbol(), "┼");
    assert_eq!(buffer[(119, 5)].symbol(), "┤");
    assert_eq!(buffer[(47, 6)].symbol(), "│");
    assert_eq!(buffer[(0, 13)].symbol(), "├");
    assert_eq!(buffer[(47, 13)].symbol(), "┴");
    assert_eq!(buffer[(119, 13)].symbol(), "┤");
    assert_eq!(buffer[(0, 28)].symbol(), "├");
    assert_eq!(buffer[(119, 28)].symbol(), "┤");
    assert_eq!(buffer[(0, 30)].symbol(), "└");
    assert_eq!(buffer[(119, 30)].symbol(), "┘");
    assert_eq!(find_ascii(&buffer, 120, 32, "scope:"), Some((1, 16)));
    assert_eq!(buffer[(1, 6)].symbol(), "○");
    assert!(guidance.0 >= description.0);
    assert!(guidance.1 > description.1);
    assert!(find_ascii(&buffer, 120, 32, "type(scope):").is_some());
    assert!(find_ascii(&buffer, 120, 32, "col 1/80").is_some());
    for heading in ["Message Properties", "Property Description"] {
        assert_bold_yellow_heading(&buffer, 120, 32, heading)?;
    }
    assert!(find_ascii(&buffer, 120, 32, "Message form").is_none());
    assert_eq!(wide.composer.editor.lines(), pristine);
    assert!(!wide.composer.dirty());
    assert_eq!(wide.composer.editor.wrap_mode(), WrapMode::WordOrGlyph);

    let scope = find_ascii(&buffer, 120, 32, "scope:").ok_or("missing scope")?;
    for column in scope.0 + 7..119 {
        assert_eq!(buffer[(column, scope.1)].symbol(), "─");
        assert_eq!(buffer[(column, scope.1)].fg, Color::DarkGray);
    }

    wide.composer.editor.move_cursor(CursorMove::Jump(6, 0));
    let buffer = rendered_buffer(&mut wide, 120, 32)?;
    assert!(find_ascii(&buffer, 120, 32, "Required concise").is_some());
    assert!(find_ascii(&buffer, 120, 32, "type(scope):").is_some());

    let mut narrow = AuthoringSession::new(built_in_commit_types(), Some(0));
    let buffer = rendered_buffer(&mut narrow, 72, 24)?;
    assert!(!narrow.too_small);
    assert!(find_ascii(&buffer, 72, 24, "col 1/80").is_some());
    assert_eq!(
        find_ascii(&buffer, 72, 24, "scope:").map(|position| position.0),
        Some(1)
    );
    assert_eq!(buffer[(0, 3)].symbol(), "┌");
    assert_eq!(buffer[(71, 22)].symbol(), "┘");

    let mut boundary = AuthoringSession::new(built_in_commit_types(), Some(0));
    let buffer = rendered_buffer(&mut boundary, 60, 21)?;
    assert!(!boundary.too_small);
    assert_eq!(buffer[(0, 13)].symbol(), "├");
    assert_eq!(buffer[(0, 17)].symbol(), "├");
    assert_eq!(buffer[(0, 19)].symbol(), "└");
    Ok(())
}

#[test]
fn decorative_rules_are_box_drawing_chrome_and_preserve_editor_state() -> Result<(), Box<dyn Error>>
{
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    paste(&mut session, "draft");
    let document = session.composer.editor.lines().to_vec();
    let cursor = session.composer.editor.cursor();
    let buffer = rendered_buffer(&mut session, 120, 40)?;

    assert_eq!(session.composer.editor.lines(), document);
    assert_eq!(session.composer.editor.cursor(), cursor);
    let scope = find_ascii(&buffer, 120, 40, "scope:").ok_or("missing scope")?;
    for column in scope.0 + 7..119 {
        assert_eq!(buffer[(column, scope.1)].symbol(), "─");
        assert_ne!(buffer[(column, scope.1)].symbol(), "—");
    }
    let body = find_ascii(&buffer, 120, 40, "Message Body").ok_or("missing body")?;
    assert_eq!(buffer[(0, body.1 - 1)].symbol(), "├");
    assert!((1..119).all(|column| buffer[(column, body.1 - 1)].symbol() == "─"));
    assert_eq!(buffer[(119, body.1 - 1)].symbol(), "┤");

    session.composer.editor.move_cursor(CursorMove::Jump(25, 0));
    let footer_buffer = rendered_buffer(&mut session, 120, 40)?;
    let footer = find_ascii(&footer_buffer, 120, 40, "Message Footer").ok_or("missing footer")?;
    assert_eq!(footer_buffer[(0, footer.1 - 1)].symbol(), "├");
    assert!((1..119).all(|column| footer_buffer[(column, footer.1 - 1)].symbol() == "─"));
    assert_eq!(footer_buffer[(119, footer.1 - 1)].symbol(), "┤");

    session.composer.editor.move_cursor(CursorMove::Jump(3, 5));
    let _ = rendered_buffer(&mut session, 72, 24)?;
    modified_press(&mut session, KeyCode::Char('u'), KeyModifiers::CONTROL);
    assert_eq!(session.composer.editor.lines()[3], "");
    assert!(!session.composer.dirty());
    Ok(())
}

#[test]
fn composer_context_height_hugs_the_taller_of_fields_and_wrapped_guidance()
-> Result<(), Box<dyn Error>> {
    let short_definitions = vec![guidance_definition("Short guidance.")?];
    let mut short = AuthoringSession::new(&short_definitions, Some(0));
    short.composer.editor.move_cursor(CursorMove::Jump(11, 0));
    let short_buffer = rendered_buffer(&mut short, 60, 32)?;
    let short_separator =
        find_symbol(&short_buffer, 60, 32, "┴").ok_or("missing short context separator")?;

    let long_definitions = vec![guidance_definition(
        "This guidance deliberately wraps across several visual rows so the context group follows its real content height without fixed pane slack.",
    )?];
    let mut long = AuthoringSession::new(&long_definitions, Some(0));
    long.composer.editor.move_cursor(CursorMove::Jump(11, 0));
    let long_buffer = rendered_buffer(&mut long, 60, 32)?;
    let long_separator =
        find_symbol(&long_buffer, 60, 32, "┴").ok_or("missing long context separator")?;
    let properties = assert_bold_yellow_heading(&long_buffer, 60, 32, "Message Properties")?;
    let guidance = assert_bold_yellow_heading(&long_buffer, 60, 32, "Property Description")?;
    let guidance_content =
        find_ascii(&long_buffer, 60, 32, "This guidance").ok_or("missing guidance content")?;

    assert!(long_separator.1 > short_separator.1);
    assert_eq!(properties, (1, 4));
    assert_eq!(guidance, (24, 4));
    assert!(guidance_content.0 >= 24);
    assert_eq!(guidance_content.1, guidance.1 + 3);
    assert_eq!(long_buffer[(23, long_separator.1)].symbol(), "┴");
    assert_eq!(short_buffer[(23, short_separator.1)].symbol(), "┴");
    Ok(())
}

#[test]
fn fields_hud_uses_spaced_content_hugging_columns_and_clips_names_before_requirements()
-> Result<(), Box<dyn Error>> {
    let definitions = vec![presentation_definition()?];
    let mut wide = AuthoringSession::new(&definitions, Some(0));
    let buffer = rendered_buffer(&mut wide, 120, 32)?;
    let scope = find_ascii(&buffer, 120, 32, "scope").ok_or("missing scope")?;
    let description = find_ascii(&buffer, 120, 32, "description").ok_or("missing description")?;
    let required_field =
        find_ascii(&buffer, 120, 32, "required-field").ok_or("missing required field")?;
    let conditional_field =
        find_ascii(&buffer, 120, 32, "conditional-field").ok_or("missing conditional field")?;
    assert_eq!(scope.0, description.0);
    assert_eq!(scope.0, required_field.0);
    assert_eq!(scope.0, conditional_field.0);

    let requirement_column = [
        (scope.1, "optional"),
        (description.1, "required"),
        (required_field.1, "required"),
        (conditional_field.1, "conditional"),
    ]
    .into_iter()
    .map(|(row, requirement)| {
        find_ascii_on_row_from_right(&buffer, 47, row, requirement)
            .ok_or("missing aligned requirement")
    })
    .collect::<Result<Vec<_>, _>>()?;
    assert!(requirement_column.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(buffer[(scope.0 - 3, scope.1)].symbol(), "○");
    assert_eq!(buffer[(scope.0 - 3, scope.1)].fg, Color::DarkGray);
    assert_eq!(buffer[(scope.0 - 2, scope.1)].symbol(), " ");
    assert_eq!(buffer[(scope.0 - 1, scope.1)].symbol(), " ");
    let name_end = scope.0 + u16::try_from("conditional-field".len())?;
    assert_eq!(requirement_column[0], name_end + 2);
    assert_eq!(buffer[(name_end, scope.1)].symbol(), " ");
    assert_eq!(buffer[(name_end + 1, scope.1)].symbol(), " ");
    assert_eq!(buffer[scope].fg, Color::Yellow);
    assert!(buffer[scope].modifier.contains(Modifier::BOLD));
    assert_eq!(buffer[(requirement_column[0], scope.1)].fg, Color::Yellow);

    let mut narrow = AuthoringSession::new(&definitions, Some(0));
    let buffer = rendered_buffer(&mut narrow, 60, 24)?;
    let requirement =
        find_ascii(&buffer, 60, 24, "conditional").ok_or("missing conditional requirement")?;
    assert!(requirement.1 > 5);
    assert!(requirement.0 < 24);
    assert_eq!(requirement.0, 12);
    assert_eq!(buffer[(1, requirement.1)].symbol(), "○");
    assert_eq!(buffer[(2, requirement.1)].symbol(), " ");
    assert_eq!(buffer[(3, requirement.1)].symbol(), " ");
    assert_eq!(buffer[(4, requirement.1)].symbol(), "c");
    assert_eq!(buffer[(23, requirement.1)].symbol(), "│");
    Ok(())
}

#[test]
fn narrow_editor_scrolls_until_fixed_width_wrap_then_returns_to_column_one()
-> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    let initial = rendered_buffer(&mut session, 72, 24)?;
    let scope = find_ascii(&initial, 72, 24, "scope:").ok_or("missing scope")?;
    let first_seventy_two = format!("{}01", "0123456789".repeat(7));
    paste(&mut session, &first_seventy_two);
    let buffer = rendered_buffer(&mut session, 72, 24)?;
    let visible = (1..71)
        .map(|column| buffer[(column, scope.1 + 1)].symbol())
        .collect::<String>();
    assert_eq!(visible, format!("{} ", &first_seventy_two[3..]));
    assert!(find_ascii(&buffer, 72, 24, "col 73/80").is_some());

    press(&mut session, KeyCode::Left);
    let buffer = rendered_buffer(&mut session, 72, 24)?;
    assert_eq!(buffer[(1, scope.1 + 1)].symbol(), "2");
    assert!(find_ascii(&buffer, 72, 24, "col 72/80").is_some());
    press(&mut session, KeyCode::Right);

    paste(&mut session, &"x".repeat(9));
    let buffer = rendered_buffer(&mut session, 72, 24)?;
    assert_eq!(
        session.composer.editor.lines()[3],
        format!("{first_seventy_two}{}", "x".repeat(9))
    );
    let wrapped_scope = find_ascii(&buffer, 72, 24, "scope:").ok_or("missing scrolled scope")?;
    assert_eq!(buffer[(1, wrapped_scope.1 + 2)].symbol(), "x");
    assert!(find_ascii(&buffer, 72, 24, "col 2/80").is_some());
    Ok(())
}

#[test]
fn fixed_width_soft_wrap_reports_visual_columns_without_changing_authored_lines()
-> Result<(), Box<dyn Error>> {
    let mut word_wrap = AuthoringSession::new(built_in_commit_types(), Some(0));
    let value = format!("{} word", "x".repeat(76));
    paste(&mut word_wrap, &value);
    let buffer = rendered_buffer(&mut word_wrap, 72, 24)?;
    assert_eq!(word_wrap.composer.editor.lines()[3], value);
    assert!(find_ascii(&buffer, 72, 24, "col 5/80").is_some());

    let mut glyph_wrap = AuthoringSession::new(built_in_commit_types(), Some(0));
    let value = "x".repeat(81);
    paste(&mut glyph_wrap, &value);
    let buffer = rendered_buffer(&mut glyph_wrap, 72, 24)?;
    assert_eq!(glyph_wrap.composer.editor.lines()[3], value);
    assert!(find_ascii(&buffer, 72, 24, "col 2/80").is_some());

    let mut unicode = AuthoringSession::new(built_in_commit_types(), Some(0));
    let value = "🦀".repeat(21);
    paste(&mut unicode, &value);
    let buffer = rendered_buffer(&mut unicode, 72, 24)?;
    assert_eq!(unicode.composer.editor.lines()[3], value);
    assert!(buffer.content().iter().any(|cell| cell.symbol() == "🦀"));
    assert!(find_ascii(&buffer, 72, 24, "col 43/80").is_some());
    Ok(())
}

#[test]
fn wrapped_and_explicit_lines_keep_a_blank_row_before_the_next_field() -> Result<(), Box<dyn Error>>
{
    let mut wrapped = AuthoringSession::new(built_in_commit_types(), Some(0));
    paste(&mut wrapped, &"x".repeat(81));
    let buffer = rendered_buffer(&mut wrapped, 72, 24)?;
    let scope = find_ascii(&buffer, 72, 24, "scope:").ok_or("missing scope")?;
    assert_eq!(wrapped.composer.editor.lines()[5], "description:");
    assert_inner_blank_row(&buffer, 72, scope.1 + 3);

    let mut explicit = AuthoringSession::new(built_in_commit_types(), Some(0));
    explicit
        .composer
        .editor
        .move_cursor(CursorMove::Jump(11, 0));
    paste(&mut explicit, "first");
    press(&mut explicit, KeyCode::Enter);
    let buffer = rendered_buffer(&mut explicit, 120, 40)?;
    let intent = find_ascii(&buffer, 120, 40, "intent:").ok_or("missing intent")?;
    let behavior = find_ascii(&buffer, 120, 40, "behavior:").ok_or("missing behavior")?;
    assert_eq!(behavior.1, intent.1 + 4);
    assert_inner_blank_row(&buffer, 120, intent.1 + 3);
    Ok(())
}

#[test]
fn adapter_rejects_catalog_and_terminal_preconditions_before_running_ratatui()
-> Result<(), Box<dyn Error>> {
    let empty = RatatuiCommitDraftAuthor
        .author(&[], None)
        .err()
        .ok_or("empty catalog should fail")?;
    assert!(matches!(empty, RatatuiCommitDraftAuthorError::EmptyCatalog));
    assert!(empty.source().is_none());

    let custom = presentation_definition()?;
    let unknown = RatatuiCommitDraftAuthor
        .author(built_in_commit_types(), Some(&custom))
        .err()
        .ok_or("unknown preselection should fail")?;
    assert!(matches!(
        unknown,
        RatatuiCommitDraftAuthorError::UnknownPreselection
    ));

    let non_terminal = RatatuiCommitDraftAuthor
        .author(built_in_commit_types(), None)
        .err()
        .ok_or("captured test streams should not be terminals")?;
    assert!(matches!(
        non_terminal,
        RatatuiCommitDraftAuthorError::NotTerminal
    ));
    assert_eq!(
        non_terminal.to_string(),
        "commit authoring requires an interactive terminal"
    );
    Ok(())
}

#[test]
fn non_press_resize_and_paste_events_obey_stage_boundaries() {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    let release = Event::Key(KeyEvent::new_with_kind(
        KeyCode::Char('x'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    ));
    assert!(session.handle_event(release).is_none());
    assert!(!session.composer.dirty());
    assert!(session.handle_event(Event::Resize(120, 40)).is_none());

    let mut picker = AuthoringSession::new(built_in_commit_types(), None);
    assert!(
        picker
            .handle_event(Event::Paste("ignored".to_owned()))
            .is_none()
    );
    assert!(!picker.composer.dirty());

    let mut review = valid_feat_session();
    let document = review.composer.editor.lines().to_vec();
    assert!(
        review
            .handle_event(Event::Paste("ignored".to_owned()))
            .is_none()
    );
    assert_eq!(review.composer.editor.lines(), document);
}
