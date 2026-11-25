use std::error::Error;

use gitserious_app::CitedCommit;
use gitserious_core::{
    COMMIT_MESSAGE_WIDTH, CommitTypeDefinition, CommitTypeId, ConditionId, PropertyCondition,
    PropertyDefinition, PropertyKey, PropertyRequirement, SchemaVersion, built_in_commit_types,
    built_in_taxonomies, validate_commit_draft,
};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use ratatui::text::Line;
use ratatui::{Frame, Terminal};

use crate::theme::{ACCENT, JET_BLACK, theme};

use super::composer::MessageFieldId;
use super::render::{hints, render};
use super::state::{AuthoringOutcome, AuthoringSession, ConfirmationAction, Stage};

pub(super) fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

pub(super) fn modified_key(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, modifiers))
}

fn left_click(column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

pub(super) fn press(session: &mut AuthoringSession, code: KeyCode) {
    assert!(session.handle_event(key(code)).is_none());
}

pub(super) fn modified_press(
    session: &mut AuthoringSession,
    code: KeyCode,
    modifiers: KeyModifiers,
) {
    assert!(
        session
            .handle_event(modified_key(code, modifiers))
            .is_none()
    );
}

pub(super) fn control(session: &mut AuthoringSession, character: char) {
    modified_press(session, KeyCode::Char(character), KeyModifiers::CONTROL);
}

pub(super) fn alt(session: &mut AuthoringSession, character: char) {
    modified_press(session, KeyCode::Char(character), KeyModifiers::ALT);
}

pub(super) fn paste(session: &mut AuthoringSession, text: &str) {
    assert!(
        session
            .handle_event(Event::Paste(text.to_owned()))
            .is_none()
    );
}

pub(super) fn typed(session: &mut AuthoringSession, text: &str) {
    for character in text.chars() {
        press(session, KeyCode::Char(character));
    }
}

/// Writes what follows the type key on the subject line, then each body in
/// turn, moving down to the next field the way a user does.
pub(super) fn write(session: &mut AuthoringSession, subject: &str, bodies: &[&str]) {
    paste(session, subject);
    for body in bodies {
        press(session, KeyCode::Down);
        paste(session, body);
    }
}

pub(super) const VALID_FEAT: &str = "feat: compose durable message\n\nintent:\nexplain intent 🦀\n\ndecision:\nfirst line\nsecond line\n";

/// A feat whose constraints do not apply, written to be reviewed.
pub(super) fn write_valid_feat(session: &mut AuthoringSession) {
    write(
        session,
        ": compose durable message",
        &["explain intent 🦀", "first line\nsecond line"],
    );
    press(session, KeyCode::Down);
    alt(session, 'n');
}

/// The valid feat, on its review.
pub(super) fn valid_feat_session() -> AuthoringSession {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    write_valid_feat(&mut session);
    control(&mut session, 's');
    session
}

pub(super) fn review_text(session: &AuthoringSession) -> Result<String, Box<dyn Error>> {
    Ok(session
        .review
        .as_ref()
        .ok_or("missing review")?
        .message
        .as_str()
        .to_owned())
}

/// Draws `session` where the Commit page puts it: below the page's header and above
/// the footer, which these tests leave as the first and last rows.
fn draw(frame: &mut Frame<'_>, session: &mut AuthoringSession) {
    let area = frame.area();
    render(
        frame,
        Rect::new(0, 1, area.width, area.height.saturating_sub(2)),
        session,
        "commit to main",
        None,
    );
    crate::theme::normalize_background(frame, area);
}

pub(super) fn rendered_buffer(
    session: &mut AuthoringSession,
    width: u16,
    height: u16,
) -> Result<Buffer, Box<dyn Error>> {
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    terminal.draw(|frame| draw(frame, session))?;
    Ok(terminal.backend().buffer().clone())
}

pub(super) fn rendered(
    session: &mut AuthoringSession,
    width: u16,
    height: u16,
) -> Result<String, Box<dyn Error>> {
    Ok(rendered_buffer(session, width, height)?
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect())
}

/// Every row of the drawn `session`, without trailing spaces.
pub(super) fn screen(
    session: &mut AuthoringSession,
    width: u16,
    height: u16,
) -> Result<Vec<String>, Box<dyn Error>> {
    let buffer = rendered_buffer(session, width, height)?;
    Ok((0..height)
        .map(|row| row_text(&buffer, width, row).trim_end().to_owned())
        .collect())
}

pub(super) fn row_text(buffer: &Buffer, width: u16, row: u16) -> String {
    (0..width)
        .map(|column| buffer[(column, row)].symbol())
        .collect()
}

pub(super) fn find_ascii(
    buffer: &Buffer,
    width: u16,
    height: u16,
    needle: &str,
) -> Option<(u16, u16)> {
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

pub(super) fn reversed_positions(buffer: &Buffer, width: u16, height: u16) -> Vec<(u16, u16)> {
    (0..height)
        .flat_map(|row| (0..width).map(move |column| (column, row)))
        .filter(|position| buffer[*position].modifier.contains(Modifier::REVERSED))
        .collect()
}

/// The text of `rows` inside the picker panel, without its padding.
fn picker_rows(buffer: &Buffer, width: u16, rows: std::ops::Range<u16>) -> Vec<String> {
    rows.map(|row| row_text(buffer, width, row).trim().to_owned())
        .collect()
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

fn find_symbol(buffer: &Buffer, width: u16, height: u16, symbol: &str) -> Option<(u16, u16)> {
    (0..height)
        .flat_map(|row| (0..width).map(move |column| (column, row)))
        .find(|position| buffer[*position].symbol() == symbol)
}

pub(super) fn assert_black_canvas(buffer: &Buffer) {
    for (index, cell) in buffer.content().iter().enumerate() {
        let width = usize::from(buffer.area.width);
        let x = index % width;
        let continuation = x > 0 && Line::from(buffer.content()[index - 1].symbol()).width() > 1;
        assert!(
            continuation
                || matches!(cell.bg, JET_BLACK | ACCENT)
                || (index / width == usize::from(buffer.area.height - 1)
                    && cell.bg == theme().band),
            "unexpected canvas background {:?} on symbol {:?} at ({}, {})",
            cell.bg,
            cell.symbol(),
            x,
            index / width
        );
    }
    assert!(buffer.content().iter().any(|cell| cell.bg == JET_BLACK));
}

pub(super) fn assert_no_box_drawing(buffer: &Buffer, width: u16, height: u16) {
    const BOX_DRAWING: [&str; 11] = ["┌", "┐", "└", "┘", "│", "─", "├", "┤", "┬", "┴", "┼"];
    assert!(
        buffer
            .content()
            .iter()
            .all(|cell| !BOX_DRAWING.contains(&cell.symbol())),
        "rendered {width}x{height} buffer contained a box-drawing border"
    );
}

fn assert_bold_blue_heading(
    buffer: &Buffer,
    width: u16,
    height: u16,
    heading: &str,
) -> Result<(u16, u16), Box<dyn Error>> {
    let position = find_ascii(buffer, width, height, heading).ok_or("missing section heading")?;
    for offset in 0..u16::try_from(heading.chars().count())? {
        let cell = &buffer[(position.0 + offset, position.1)];
        assert_eq!(cell.fg, Color::Rgb(80, 160, 255));
        assert!(cell.modifier.contains(Modifier::BOLD));
    }
    Ok(position)
}

/// The editor leaves the page's header row and the footer row to the Commit page.
pub(super) fn assert_rows_untouched(buffer: &Buffer, width: u16, height: u16) {
    for row in [0, height - 1] {
        assert!(row_text(buffer, width, row).trim().is_empty(), "row {row}");
    }
}

pub(super) fn property(
    key: &str,
    requirement: PropertyRequirement,
) -> Result<PropertyDefinition, Box<dyn Error>> {
    Ok(PropertyDefinition::new(
        PropertyKey::new(key)?,
        format!("Description for {key}."),
        requirement,
    )?)
}

/// A type with one property of each requirement, in requirement order.
pub(super) fn presentation_definition() -> Result<CommitTypeDefinition, Box<dyn Error>> {
    Ok(CommitTypeDefinition::new(
        Some(SchemaVersion::V1),
        CommitTypeId::new("custom")?,
        "Exercise every property presentation.",
        vec![
            property("required-field", PropertyRequirement::Required)?,
            property("recommended-field", PropertyRequirement::Recommended)?,
            property("optional-field", PropertyRequirement::Optional)?,
            property(
                "conditional-field",
                PropertyRequirement::Conditional(PropertyCondition::new(
                    ConditionId::new("when-needed")?,
                    "required when the condition applies",
                )?),
            )?,
        ],
    )?)
}

mod picker;

#[test]
fn taxonomy_picker_switches_schema_and_confirms_discard() -> Result<(), Box<dyn Error>> {
    let taxonomies = built_in_taxonomies();
    let ids = taxonomies
        .iter()
        .map(|taxonomy| taxonomy.id().clone())
        .collect::<Vec<_>>();
    let context = gitserious_app::CommitAuthoringContext::new(taxonomies.to_vec(), &ids[0], None)?;
    let mut session = AuthoringSession::with_context(context.clone());
    assert!(rendered(&mut session, 100, 32)?.contains("research"));
    press(&mut session, KeyCode::Right);
    assert_eq!(session.taxonomy().ok_or("missing taxonomy")?.id(), &ids[1]);
    assert_eq!(session.definitions, context.taxonomies()[1].definitions());
    press(&mut session, KeyCode::Enter);
    press(&mut session, KeyCode::Char('x'));
    let edited = session.composer.subject.lines().to_vec();
    press(&mut session, KeyCode::Esc);
    assert_eq!(session.stage, Stage::Confirm);
    press(&mut session, KeyCode::Char('n'));
    assert_eq!(session.composer.subject.lines(), edited);
    press(&mut session, KeyCode::Esc);
    press(&mut session, KeyCode::Char('y'));
    assert_eq!(session.stage, Stage::SelectType);
    press(&mut session, KeyCode::Right);
    assert_eq!(session.taxonomy().ok_or("missing taxonomy")?.id(), &ids[2]);
    assert!(!session.composer.dirty());
    press(&mut session, KeyCode::Right);
    assert_eq!(session.taxonomy().ok_or("missing taxonomy")?.id(), &ids[0]);
    Ok(())
}

#[test]
fn context_review_hides_provenance_the_approved_bytes_keep() -> Result<(), Box<dyn Error>> {
    let taxonomy = built_in_taxonomies()[0].clone();
    let id = taxonomy.id().clone();
    let context = gitserious_app::CommitAuthoringContext::new(
        vec![taxonomy],
        &id,
        Some(&CommitTypeId::new("feat")?),
    )?;
    let mut session = AuthoringSession::with_context(context.clone());
    write_valid_feat(&mut session);
    control(&mut session, 's');
    let review = session.review.as_ref().ok_or("review missing")?;
    assert_eq!(
        review.message,
        context
            .initial_taxonomy()
            .validate(&review.draft)
            .into_message()?
    );
    assert!(
        review
            .message
            .as_str()
            .contains("Gitserious-Taxonomy: conventional-v1\n")
    );
    let frame = rendered(&mut session, 100, 32)?;
    assert!(frame.contains("feat: compose durable message"));
    assert!(!frame.contains("Gitserious-"), "{frame}");
    press(&mut session, KeyCode::Esc);
    assert!(session.approved_message.is_none());
    session.composer.focus = super::composer::MessageFieldId::Subject;
    for _ in "compose durable message".chars() {
        press(&mut session, KeyCode::Backspace);
    }
    paste(&mut session, "revise the message");
    control(&mut session, 's');
    let expected = session
        .review
        .as_ref()
        .ok_or("review missing")?
        .message
        .clone();
    let outcome = session.handle_event(key(KeyCode::Enter));
    assert!(
        matches!(outcome, Some(AuthoringOutcome::Authored(draft)) if draft.subject().as_str() == "revise the message")
    );
    assert_eq!(session.approved_message, Some(expected));
    Ok(())
}

#[test]
fn review_is_exact_backtracking_is_lossless_and_enter_returns_the_typed_draft()
-> Result<(), Box<dyn Error>> {
    let mut session = valid_feat_session();
    assert_eq!(session.stage, Stage::Review);
    assert_eq!(review_text(&session)?, VALID_FEAT);

    press(&mut session, KeyCode::Esc);
    assert_eq!(session.stage, Stage::Compose);
    assert_eq!(
        session.composer.subject.lines(),
        [": compose durable message"]
    );
    assert_eq!(
        session.composer.properties[1].body.lines(),
        ["first line", "second line"]
    );
    control(&mut session, 's');
    let outcome = session
        .handle_event(key(KeyCode::Enter))
        .ok_or("review confirmation did not finish")?;
    let AuthoringOutcome::Authored(draft) = outcome else {
        return Err("review unexpectedly cancelled".into());
    };
    assert_eq!(
        validate_commit_draft(&built_in_commit_types()[0], &draft)
            .into_message()?
            .as_str(),
        VALID_FEAT
    );
    Ok(())
}

#[test]
fn breaking_change_is_optional_multiline_and_normalizes_scope_only_in_output()
-> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    write(
        &mut session,
        "(tui editor): ship editor",
        &["reduce cost", "compose once"],
    );
    press(&mut session, KeyCode::Down);
    alt(&mut session, 'n');
    press(&mut session, KeyCode::Down);
    paste(&mut session, "remove old fields\nuse fixed form");
    control(&mut session, 's');

    let review = session.review.as_ref().ok_or("missing review")?;
    assert_eq!(
        review.message.as_str(),
        "feat(tui-editor)!: ship editor\n\nintent:\nreduce cost\n\ndecision:\ncompose once\n\nBREAKING CHANGE: remove old fields\nuse fixed form\n"
    );
    assert_eq!(
        review
            .draft
            .scope()
            .map(gitserious_core::CommitScope::as_str),
        Some("tui editor")
    );
    assert_eq!(
        review
            .draft
            .breaking_change()
            .map(gitserious_core::PropertyValue::as_str),
        Some("remove old fields\nuse fixed form")
    );

    press(&mut session, KeyCode::Esc);
    assert_eq!(
        session.composer.subject.lines(),
        ["(tui editor): ship editor"]
    );
    assert_eq!(
        session.composer.breaking_change.body.lines(),
        ["remove old fields", "use fixed form"]
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
    write(
        &mut session,
        ": compose durable message",
        &["explain intent", &behavior],
    );
    press(&mut session, KeyCode::Down);
    alt(&mut session, 'n');
    control(&mut session, 's');

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
    write(
        &mut session,
        "(api \t): ship it  ",
        &["\n  leading reason \t\n", "first  \nsecond\t"],
    );
    press(&mut session, KeyCode::Down);
    alt(&mut session, 'n');
    control(&mut session, 's');

    assert_eq!(session.stage, Stage::Review);
    assert_eq!(
        review_text(&session)?,
        "feat(api): ship it\n\nintent:\n  leading reason\n\ndecision:\nfirst\nsecond\n"
    );
    press(&mut session, KeyCode::Esc);
    assert_eq!(session.composer.subject.lines(), ["(api \t): ship it  "]);
    assert_eq!(
        session.composer.properties[0].body.lines(),
        ["", "  leading reason \t", ""]
    );
    Ok(())
}

#[test]
fn compose_review_and_final_encoding_share_the_80_column_boundary() -> Result<(), Box<dyn Error>> {
    let width = usize::from(COMMIT_MESSAGE_WIDTH);
    let intent = format!("{} x", "x".repeat(width - 1));
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    write(
        &mut session,
        ": compose durable message",
        &[&intent, "first line\nsecond line"],
    );
    press(&mut session, KeyCode::Down);
    alt(&mut session, 'n');
    press(&mut session, KeyCode::Up);
    press(&mut session, KeyCode::Up);

    let compose = screen(&mut session, 120, 32)?;
    let heading = compose
        .iter()
        .position(|row| row.ends_with("▎● decision:                                                                                                   required"))
        .ok_or("missing focused decision field")?;
    let intent_heading = compose
        .iter()
        .position(|row| row.starts_with(" ✓ intent:"))
        .ok_or("missing intent field")?;
    assert!(intent_heading < heading);
    assert_eq!(
        compose[intent_heading + 1],
        format!("   {}", "x".repeat(width - 1)),
        "the field wraps where the message does"
    );
    assert_eq!(compose[intent_heading + 2], "   x");

    control(&mut session, 's');
    let expected = format!(
        "feat: compose durable message\n\nintent:\n{}\nx\n\ndecision:\nfirst line\nsecond line\n",
        "x".repeat(width - 1),
    );
    assert_eq!(review_text(&session)?, expected);

    let review = rendered_buffer(&mut session, 120, 32)?;
    let intent_label = find_ascii(&review, 120, 32, "intent:").ok_or("missing intent label")?;
    assert_eq!(intent_label.0, 1);
    assert_eq!(
        find_ascii_on_row_from_right(&review, 120, intent_label.1 + 2, "x"),
        Some(1)
    );

    let outcome = session
        .handle_event(key(KeyCode::Enter))
        .ok_or("review confirmation did not finish")?;
    let AuthoringOutcome::Authored(draft) = outcome else {
        return Err("review unexpectedly cancelled".into());
    };
    assert_eq!(draft.properties()[0].value().as_str(), intent);
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
    paste(&mut unpinned, ": dirty");
    press(&mut unpinned, KeyCode::Esc);
    assert_eq!(unpinned.stage, Stage::Confirm);
    assert_eq!(unpinned.confirmation, ConfirmationAction::ChangeType);
    press(&mut unpinned, KeyCode::Enter);
    assert_eq!(unpinned.stage, Stage::Compose);
    assert_eq!(unpinned.composer.subject.lines(), [": dirty"]);
    press(&mut unpinned, KeyCode::Esc);
    press(&mut unpinned, KeyCode::Char('y'));
    assert_eq!(unpinned.stage, Stage::SelectType);
    assert!(!unpinned.composer.dirty());

    let mut decided = AuthoringSession::new(definitions, Some(0));
    for _ in 0..3 {
        press(&mut decided, KeyCode::Down);
    }
    alt(&mut decided, 'n');
    assert!(decided.composer.dirty(), "a decision is work to lose");

    let mut pinned = AuthoringSession::new(definitions, Some(0));
    assert_eq!(
        pinned.handle_event(key(KeyCode::Esc)),
        Some(AuthoringOutcome::Cancelled)
    );
    let mut pinned = AuthoringSession::new(definitions, Some(0));
    paste(&mut pinned, ": dirty");
    press(&mut pinned, KeyCode::Esc);
    assert_eq!(pinned.confirmation, ConfirmationAction::Cancel);
    assert_eq!(
        pinned.handle_event(key(KeyCode::Char('y'))),
        Some(AuthoringOutcome::Cancelled)
    );
}

#[test]
fn review_ignores_q_and_esc_keeps_editing_the_same_text() -> Result<(), Box<dyn Error>> {
    let mut session = valid_feat_session();
    let message = review_text(&session)?;
    press(&mut session, KeyCode::Char('q'));
    assert_eq!(session.stage, Stage::Review);
    assert_eq!(review_text(&session)?, message);
    press(&mut session, KeyCode::Esc);
    assert_eq!(session.stage, Stage::Compose);
    assert_eq!(
        session.composer.subject.lines(),
        [": compose durable message"]
    );
    assert_eq!(
        session.composer.properties[0].body.lines(),
        ["explain intent 🦀"]
    );
    Ok(())
}

#[test]
fn picker_and_review_draw_without_frames_inside_the_page() -> Result<(), Box<dyn Error>> {
    let mut picker = AuthoringSession::new(built_in_commit_types(), None);
    let text = rendered(&mut picker, 100, 24)?;
    assert!(!text.contains("gitserious commit"));
    assert!(!text.contains("Choose the semantic contract for this commit."));
    assert!(!text.contains("Commit types"));
    assert_eq!(
        text.matches("An addition or expansion of capability.")
            .count(),
        1
    );
    let picker_buffer = rendered_buffer(&mut picker, 100, 24)?;
    assert_rows_untouched(&picker_buffer, 100, 24);
    assert_no_box_drawing(&picker_buffer, 100, 24);
    assert_eq!(
        find_ascii(&picker_buffer, 100, 24, "conventional"),
        Some((3, 2)),
    );
    let selected = find_ascii(&picker_buffer, 100, 24, "feat").ok_or("missing selection")?;
    assert_eq!(selected, (3, 4));
    assert_eq!(picker_buffer[selected].bg, theme().cursor);

    let mut review = valid_feat_session();
    let text = rendered(&mut review, 100, 24)?;
    assert!(!text.contains("Review the exact canonical message before Git creates the commit."));
    assert!(text.contains("feat: compose durable message"));
    let review_buffer = rendered_buffer(&mut review, 100, 24)?;
    assert_rows_untouched(&review_buffer, 100, 24);
    assert_eq!(
        find_ascii(&review_buffer, 100, 24, "feat: compose durable message"),
        Some((1, 1)),
    );
    assert!(find_ascii(&review_buffer, 100, 24, "Final Git Commit Message").is_none());
    assert_no_box_drawing(&review_buffer, 100, 24);
    assert_eq!(row_text(&review_buffer, 100, 9).trim(), "");
    assert_eq!(
        row_text(&review_buffer, 100, 10),
        format!("  commit to main{}enter  ", " ".repeat(77))
    );
    for column in 0..100 {
        let cell = &review_buffer[(column, 10)];
        if (1..99).contains(&column) {
            assert_eq!((cell.fg, cell.bg), (theme().ground, theme().cursor));
            assert!(cell.modifier.contains(Modifier::BOLD));
        } else {
            assert_ne!(cell.bg, theme().cursor);
        }
    }
    assert_eq!(
        hints(&review),
        [
            ("enter", "commit"),
            ("esc", "keep editing"),
            ("↑/↓", "scroll")
        ]
    );
    Ok(())
}

#[test]
fn confirmation_uses_double_frame_clickable_buttons_and_preserves_keyboard()
-> Result<(), Box<dyn Error>> {
    let assert_bone_dialog = |buffer: &Buffer,
                              width,
                              height,
                              buttons: &[ratatui::layout::Rect]|
     -> Result<(), Box<dyn Error>> {
        let (x, y) = find_symbol(buffer, width, height, "╔").ok_or("modal border")?;
        for row in y..y + 9 {
            for column in x..x + 58 {
                let chip = buttons
                    .iter()
                    .any(|area| area.contains((column, row).into()));
                assert_eq!(
                    buffer[(column, row)].bg,
                    if chip {
                        theme().pending
                    } else {
                        theme().ground
                    }
                );
                if chip {
                    assert_eq!(buffer[(column, row)].fg, theme().ground);
                }
            }
        }
        assert_eq!(buffer[(x, y)].fg, theme().pending);
        let (x, y) = find_ascii(buffer, width, height, "Discard Message").ok_or("title")?;
        for column in x..x + 15 {
            assert_eq!(buffer[(column, y)].fg, theme().text);
            assert!(buffer[(column, y)].modifier.contains(Modifier::BOLD));
        }
        Ok(())
    };
    let mut review = valid_feat_session();
    press(&mut review, KeyCode::Esc);
    press(&mut review, KeyCode::Esc);
    let text = rendered(&mut review, 100, 24)?;
    assert!(text.contains("Discard Message"));
    assert!(text.contains("Discard this draft and cancel"));
    assert!(text.contains("y discard"));
    assert!(text.contains("enter/esc/n keep editing"));
    assert!(
        text.contains("intent:"),
        "the composer stays under the dialog"
    );
    let buffer = rendered_buffer(&mut review, 100, 24)?;
    let buttons = review
        .discard_dialog
        .buttons
        .ok_or("missing button hitboxes")?;
    assert_bone_dialog(&buffer, 100, 24, &[buttons.confirm, buttons.keep])?;
    let top_left = find_symbol(&buffer, 100, 24, "╔").ok_or("missing popup frame")?;
    assert_eq!(buffer[(top_left.0 + 57, top_left.1)].symbol(), "╗");
    assert_eq!(buffer[(top_left.0, top_left.1 + 8)].symbol(), "╚");
    assert_eq!(buffer[(top_left.0 + 57, top_left.1 + 8)].symbol(), "╝");
    for area in [buttons.confirm, buttons.keep] {
        assert!((area.x..area.right()).all(|x| buffer[(x, area.y)].bg == theme().pending));
    }
    assert!(review.handle_event(left_click(0, 0)).is_none());
    assert_eq!(review.stage, Stage::Confirm);
    assert!(
        review
            .handle_event(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Right),
                column: buttons.confirm.x,
                row: buttons.confirm.y,
                modifiers: KeyModifiers::NONE,
            }))
            .is_none()
    );
    assert_eq!(review.stage, Stage::Confirm);
    assert!(
        review
            .handle_event(left_click(buttons.keep.x, buttons.keep.y))
            .is_none()
    );
    assert_eq!(review.stage, Stage::Compose);

    press(&mut review, KeyCode::Esc);
    let _ = rendered_buffer(&mut review, 100, 24)?;
    let buttons = review
        .discard_dialog
        .buttons
        .ok_or("missing rerendered hitboxes")?;
    assert_eq!(
        review.handle_event(left_click(buttons.confirm.x, buttons.confirm.y)),
        Some(AuthoringOutcome::Cancelled)
    );

    for (width, height) in [(59, 24), (100, 17), (60, 17)] {
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
        assert_bold_blue_heading(&buffer, width, height, "Terminal too small")?;
        assert_no_box_drawing(&buffer, width, height);
        assert_black_canvas(&buffer);
        assert_eq!(
            too_small.handle_event(key(KeyCode::Esc)),
            Some(AuthoringOutcome::Cancelled)
        );
    }
    let mut fits = AuthoringSession::new(built_in_commit_types(), Some(0));
    let _ = rendered_buffer(&mut fits, 60, 18)?;
    assert!(
        !fits.too_small,
        "the composer scrolls, so it fits the picker's minimum"
    );

    let mut small_confirmation = AuthoringSession::new(built_in_commit_types(), Some(0));
    paste(&mut small_confirmation, ": dirty");
    press(&mut small_confirmation, KeyCode::Esc);
    let buffer = rendered_buffer(&mut small_confirmation, 59, 24)?;
    assert!(small_confirmation.too_small);
    let buttons = small_confirmation
        .discard_dialog
        .buttons
        .ok_or("small dialog buttons")?;
    assert_bone_dialog(&buffer, 59, 24, &[buttons.confirm, buttons.keep])?;
    assert!(find_ascii(&buffer, 59, 24, "Discard this draft?").is_some());
    assert!(find_ascii(&buffer, 59, 24, "y discard").is_some());
    assert!(find_ascii(&buffer, 59, 24, "enter/esc/n keep editing").is_some());
    assert!(find_symbol(&buffer, 59, 24, "╔").is_some());
    Ok(())
}

#[test]
fn non_press_resize_and_paste_events_obey_stage_boundaries() -> Result<(), Box<dyn Error>> {
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
    paste(&mut picker, "ignored");
    assert!(!picker.composer.dirty());

    let mut review = valid_feat_session();
    let message = review_text(&review)?;
    paste(&mut review, "ignored");
    assert_eq!(review_text(&review)?, message);
    press(&mut review, KeyCode::Esc);
    assert_eq!(
        review.composer.subject.lines(),
        [": compose durable message"]
    );
    Ok(())
}

#[test]
fn ctrl_t_keeps_answers_the_new_type_asks_for_and_esc_restores_the_old_one()
-> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    write(
        &mut session,
        "(net): retry sooner",
        &["why it exists", "this repair", "a bound"],
    );
    press(&mut session, KeyCode::Down);
    paste(&mut session, "callers must migrate");
    control(&mut session, 't');
    assert_eq!(session.stage, Stage::SelectType);
    press(&mut session, KeyCode::Down);
    press(&mut session, KeyCode::Esc);
    assert_eq!(session.stage, Stage::Compose);
    assert_eq!(session.composer.definition().id().as_str(), "feat");
    assert_eq!(session.composer.subject.lines(), ["(net): retry sooner"]);
    assert_eq!(
        session.composer.text(MessageFieldId::Property(0)).lines(),
        ["why it exists"]
    );

    control(&mut session, 't');
    press(&mut session, KeyCode::Down);
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.stage, Stage::Compose);
    assert_eq!(session.composer.definition().id().as_str(), "fix");
    assert_eq!(session.composer.subject.lines(), ["(net): retry sooner"]);
    assert_eq!(
        session
            .composer
            .properties
            .iter()
            .map(|field| field.body.lines().join("\n"))
            .collect::<Vec<_>>(),
        vec!["", "this repair", "a bound"]
    );
    assert_eq!(
        session.composer.properties[2].requirement.word(),
        "applies",
        "carried constraint text answers that the property applies"
    );
    assert_eq!(
        session
            .composer
            .text(MessageFieldId::BreakingChange)
            .lines(),
        ["callers must migrate"]
    );
    let text = rendered(&mut session, 100, 30)?;
    assert!(text.contains("fix(net): retry sooner"), "{text}");
    assert!(!text.contains("intent:"), "{text}");
    assert!(text.contains("cause:"), "{text}");

    control(&mut session, 's');
    assert_eq!(session.stage, Stage::Compose);
    assert!(
        session
            .composer
            .feedback
            .iter()
            .any(|feedback| feedback.message.contains("cause"))
    );
    assert!(
        session
            .composer
            .feedback
            .iter()
            .all(|feedback| !feedback.message.contains("intent"))
    );
    Ok(())
}

#[test]
fn ctrl_t_restores_the_taxonomy_when_the_picker_is_cancelled() -> Result<(), String> {
    let mut session = AuthoringSession::with_context(super::built_in_context()?);
    press(&mut session, KeyCode::Enter);
    write(&mut session, ": keep this", &["why it exists"]);
    let taxonomy = session
        .taxonomy()
        .ok_or("missing taxonomy")?
        .id()
        .to_string();
    control(&mut session, 't');
    press(&mut session, KeyCode::Right);
    assert_ne!(
        session
            .taxonomy()
            .ok_or("missing switched taxonomy")?
            .id()
            .as_str(),
        taxonomy
    );
    assert_eq!(session.composer.definition().id().as_str(), "feat");
    assert_eq!(
        session.composer.text(MessageFieldId::Property(0)).lines(),
        ["why it exists"]
    );
    press(&mut session, KeyCode::Esc);
    assert_eq!(session.stage, Stage::Compose);
    assert_eq!(
        session
            .taxonomy()
            .ok_or("missing restored taxonomy")?
            .id()
            .as_str(),
        taxonomy
    );
    assert_eq!(session.composer.definition().id().as_str(), "feat");
    assert_eq!(
        session.composer.text(MessageFieldId::Property(0)).lines(),
        ["why it exists"]
    );
    Ok(())
}

fn citing(paths: &[&str]) -> AuthoringSession {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    session.set_citation_paths(paths.iter().map(|path| (*path).to_owned()).collect());
    session
}

fn focused_text(session: &AuthoringSession) -> String {
    session
        .composer
        .text(session.composer.focus)
        .lines()
        .join("\n")
}

#[test]
fn at_opens_a_list_and_does_not_write_at() {
    let mut session = citing(&["src/net/retry.rs"]);
    paste(&mut session, "see ");
    press(&mut session, KeyCode::Char('@'));
    assert!(session.citation_picker.is_some());
    assert_eq!(focused_text(&session), "see ");
    assert_eq!(
        hints(&session),
        [
            ("enter", "cite"),
            ("↑/↓", "move"),
            ("←/→", "kind"),
            ("esc", "cancel"),
        ]
    );
}

#[test]
fn enter_inserts_the_path_and_esc_writes_nothing() {
    let mut session = citing(&["src/net/retry.rs", "README.md"]);
    press(&mut session, KeyCode::Char('@'));
    press(&mut session, KeyCode::Enter);
    assert!(session.citation_picker.is_none());
    assert_eq!(focused_text(&session), "@src/net/retry.rs");

    let mut session = citing(&["src/net/retry.rs"]);
    paste(&mut session, "see ");
    press(&mut session, KeyCode::Char('@'));
    typed(&mut session, "nope");
    press(&mut session, KeyCode::Esc);
    assert!(session.citation_picker.is_none());
    assert_eq!(focused_text(&session), "see ");
}

#[test]
fn typing_keeps_paths_that_contain_the_query() -> Result<(), Box<dyn Error>> {
    let mut session = citing(&["src/net/retry.rs", "README.md"]);
    press(&mut session, KeyCode::Char('@'));
    typed(&mut session, "retry");
    let screen = rendered(&mut session, 80, 24)?;
    assert!(screen.contains("src/net/retry.rs"), "{screen}");
    assert!(!screen.contains("README.md"), "{screen}");
    assert!(!screen.contains("Typing filters"), "{screen}");
    assert!(!screen.contains("tab kind"), "{screen}");
    press(&mut session, KeyCode::Enter);
    assert_eq!(focused_text(&session), "@src/net/retry.rs");
    Ok(())
}

#[test]
fn down_and_up_wrap_the_list() {
    let mut session = citing(&["README.md", "src/net/retry.rs"]);
    press(&mut session, KeyCode::Char('@'));
    press(&mut session, KeyCode::Down);
    press(&mut session, KeyCode::Enter);
    assert_eq!(focused_text(&session), "@src/net/retry.rs");

    let mut session = citing(&["README.md", "src/net/retry.rs"]);
    press(&mut session, KeyCode::Char('@'));
    press(&mut session, KeyCode::Up);
    press(&mut session, KeyCode::Enter);
    assert_eq!(focused_text(&session), "@src/net/retry.rs");
}

#[test]
fn a_directory_keeps_its_trailing_slash() {
    let mut session = citing(&["src/net/retry.rs", "src/net/"]);
    press(&mut session, KeyCode::Char('@'));
    press(&mut session, KeyCode::Down);
    press(&mut session, KeyCode::Enter);
    assert_eq!(focused_text(&session), "@src/net/");
}

#[test]
fn enter_on_no_match_leaves_the_list_open() -> Result<(), Box<dyn Error>> {
    let mut session = citing(&["src/net/retry.rs"]);
    press(&mut session, KeyCode::Char('@'));
    typed(&mut session, "zzz");
    let screen = rendered(&mut session, 80, 24)?;
    assert!(screen.contains("no match"), "{screen}");
    press(&mut session, KeyCode::Enter);
    assert!(session.citation_picker.is_some());
    assert_eq!(focused_text(&session), "");
    Ok(())
}

#[test]
fn cite_lands_at_the_cursor() {
    let mut session = citing(&["src/net/retry.rs"]);
    press(&mut session, KeyCode::Down);
    paste(&mut session, "abXcd");
    press(&mut session, KeyCode::Left);
    press(&mut session, KeyCode::Left);
    press(&mut session, KeyCode::Char('@'));
    press(&mut session, KeyCode::Enter);
    assert_eq!(focused_text(&session), "abX@src/net/retry.rscd");
}

#[test]
fn the_list_takes_the_keys_that_would_leave_the_composer() {
    let mut session = citing(&["src/a.rs"]);
    press(&mut session, KeyCode::Down);
    press(&mut session, KeyCode::Down);
    press(&mut session, KeyCode::Down);
    press(&mut session, KeyCode::Char('@'));
    control(&mut session, 's');
    control(&mut session, 't');
    alt(&mut session, 'n');
    assert_eq!(session.stage, Stage::Compose);
    assert!(session.citation_picker.is_some());
    assert!(session.review.is_none());
    assert_eq!(
        session
            .composer
            .field(session.composer.focus)
            .map(|field| field.requirement.word()),
        Some("conditional")
    );
}

#[test]
fn the_selected_path_uses_the_popup_row() -> Result<(), Box<dyn Error>> {
    let mut session = citing(&["src/net/retry.rs", "README.md"]);
    press(&mut session, KeyCode::Char('@'));
    let buffer = rendered_buffer(&mut session, 80, 24)?;
    let (column, row) = find_ascii(&buffer, 80, 24, "src/net/retry.rs").ok_or("missing path")?;
    assert_eq!(buffer[(column, row)].bg, theme().cursor);
    let (column, row) = find_ascii(&buffer, 80, 24, "README.md").ok_or("missing readme")?;
    assert_ne!(buffer[(column, row)].bg, theme().cursor);
    assert_eq!(buffer[(column - 1, row)].bg, theme().band);
    assert!(!rendered(&mut session, 80, 24)?.contains("Cite"));
    Ok(())
}

#[test]
fn the_query_row_prompts_until_something_is_typed() -> Result<(), Box<dyn Error>> {
    let mut session = with_commits(&["src/net/retry.rs"], &[("abc1234", "back off")]);
    press(&mut session, KeyCode::Char('@'));
    let text = rendered(&mut session, 80, 24)?;
    assert!(text.contains("Type to filter files"), "{text}");
    press(&mut session, KeyCode::Right);
    let text = rendered(&mut session, 80, 24)?;
    assert!(text.contains("Type to filter commits"), "{text}");
    typed(&mut session, "back");
    let text = rendered(&mut session, 80, 24)?;
    assert!(!text.contains("Type to filter"), "{text}");
    let rows = screen(&mut session, 80, 24)?;
    let sha = rows
        .iter()
        .position(|row| row.contains("abc1234"))
        .ok_or("missing commit")?;
    assert_eq!(rows[sha + 2].trim(), "back", "{rows:#?}");
    Ok(())
}

#[test]
fn the_list_opens_under_the_line_being_written() -> Result<(), Box<dyn Error>> {
    let mut session = citing(&["src/net/retry.rs"]);
    press(&mut session, KeyCode::Down);
    paste(&mut session, "see ");
    press(&mut session, KeyCode::Char('@'));
    let buffer = rendered_buffer(&mut session, 80, 24)?;
    let (_, line) = find_ascii(&buffer, 80, 24, "see ").ok_or("missing body")?;
    let (_, kinds) = find_ascii(&buffer, 80, 24, "Files").ok_or("missing kinds")?;
    let (_, path) = find_ascii(&buffer, 80, 24, "src/net/retry.rs").ok_or("missing path")?;
    assert_eq!(kinds, line + 3);
    assert_eq!(path, kinds + 2);
    Ok(())
}

#[test]
fn the_list_opens_above_a_line_near_the_bottom() -> Result<(), Box<dyn Error>> {
    let mut session = citing(&["src/net/retry.rs"]);
    press(&mut session, KeyCode::Down);
    paste(&mut session, &format!("{}last ", "line\n".repeat(11)));
    press(&mut session, KeyCode::Char('@'));
    let buffer = rendered_buffer(&mut session, 80, 24)?;
    let (_, line) = find_ascii(&buffer, 80, 24, "last ").ok_or("missing body")?;
    let (_, path) = find_ascii(&buffer, 80, 24, "src/net/retry.rs").ok_or("missing path")?;
    assert!(path < line, "{}", rendered(&mut session, 80, 24)?);
    Ok(())
}

#[test]
fn at_before_a_type_is_chosen_stays_in_the_picker() {
    let mut session = AuthoringSession::new(built_in_commit_types(), None);
    press(&mut session, KeyCode::Char('@'));
    assert_eq!(session.stage, Stage::SelectType);
    assert!(session.citation_picker.is_none());
    assert!(!session.composer.dirty());
}

#[test]
fn a_pasted_query_filters_the_list() {
    let mut session = citing(&["src/net/retry.rs", "README.md"]);
    press(&mut session, KeyCode::Char('@'));
    paste(&mut session, "README");
    press(&mut session, KeyCode::Enter);
    assert_eq!(focused_text(&session), "@README.md");
}

fn with_commits(paths: &[&str], commits: &[(&str, &str)]) -> AuthoringSession {
    let mut session = citing(paths);
    session.set_citation_commits(
        commits
            .iter()
            .map(|(sha, subject)| CitedCommit {
                sha: (*sha).to_owned(),
                subject: (*subject).to_owned(),
            })
            .collect(),
    );
    session
}

#[test]
fn right_keeps_the_query_and_inserts_the_short_sha() {
    let mut session = with_commits(
        &["src/net/retry.rs"],
        &[
            ("def5678", "rename the pool"),
            ("abc1234", "back off on 429"),
        ],
    );
    press(&mut session, KeyCode::Char('@'));
    typed(&mut session, "429");
    press(&mut session, KeyCode::Right);
    press(&mut session, KeyCode::Enter);
    assert!(session.citation_picker.is_none());
    assert_eq!(focused_text(&session), "@abc1234");
}

#[test]
fn left_returns_to_files_and_esc_on_a_commit_writes_nothing() {
    let mut session = with_commits(&["README.md"], &[("abc1234", "back off")]);
    press(&mut session, KeyCode::Char('@'));
    press(&mut session, KeyCode::Right);
    press(&mut session, KeyCode::Left);
    press(&mut session, KeyCode::Enter);
    assert_eq!(focused_text(&session), "@README.md");

    let mut session = with_commits(&["README.md"], &[("abc1234", "back off")]);
    paste(&mut session, "see ");
    press(&mut session, KeyCode::Char('@'));
    press(&mut session, KeyCode::Right);
    press(&mut session, KeyCode::Esc);
    assert!(session.citation_picker.is_none());
    assert_eq!(focused_text(&session), "see ");
}

#[test]
fn the_commit_list_shows_the_sha_and_subject() -> Result<(), Box<dyn Error>> {
    let mut session = with_commits(&["README.md"], &[("abc1234", "back off on 429")]);
    press(&mut session, KeyCode::Char('@'));
    press(&mut session, KeyCode::Right);
    let screen = rendered(&mut session, 80, 24)?;
    assert!(screen.contains("Commits"), "{screen}");
    assert!(screen.contains("abc1234"), "{screen}");
    assert!(screen.contains("back off on 429"), "{screen}");
    assert!(!screen.contains("Typing filters"), "{screen}");
    assert!(!screen.contains("@abc1234"), "{screen}");
    Ok(())
}

/// The columns of `row` drawn on a panel, banded or highlighted.
fn panel_columns(buffer: &Buffer, width: u16, row: u16) -> Option<(u16, u16)> {
    let on_panel = |column: &u16| {
        let bg = buffer[(*column, row)].bg;
        bg == theme().band || bg == theme().cursor
    };
    let first = (0..width).find(on_panel)?;
    let last = (0..width).rev().find(on_panel)?;
    Some((first, last))
}

/// The selected row of a picker panel.
#[derive(Debug, PartialEq)]
struct SelectedRow {
    /// The first and last column the panel covers.
    columns: (u16, u16),
    /// The column the selected text starts at.
    text: u16,
    fg: Color,
    bg: Color,
    bold: bool,
}

fn selected_row(
    buffer: &Buffer,
    width: u16,
    height: u16,
    text: &str,
) -> Result<SelectedRow, Box<dyn Error>> {
    let (column, row) = find_ascii(buffer, width, height, text).ok_or("missing selected text")?;
    let columns = panel_columns(buffer, width, row).ok_or("the selected row has no panel")?;
    for x in 0..width {
        let highlighted = buffer[(x, row)].bg == theme().cursor;
        assert_eq!(
            highlighted,
            (columns.0..=columns.1).contains(&x),
            "the highlight spans the panel at ({x}, {row})"
        );
    }
    let cell = &buffer[(column, row)];
    Ok(SelectedRow {
        columns,
        text: column,
        fg: cell.fg,
        bg: cell.bg,
        bold: cell.modifier.contains(Modifier::BOLD),
    })
}

#[test]
fn the_citation_panel_is_banded_inset_and_highlights_its_whole_row() -> Result<(), Box<dyn Error>> {
    for (width, height) in [(80, 24), (100, 30)] {
        let mut session = citing(&["src/net/retry.rs", "README.md"]);
        press(&mut session, KeyCode::Char('@'));
        let buffer = rendered_buffer(&mut session, width, height)?;
        let row = selected_row(&buffer, width, height, "src/net/retry.rs")?;
        let columns = row.columns;
        assert_eq!(columns, (1, width - 2), "at {width}x{height}");
        assert_eq!(
            row.text,
            columns.0 + 2,
            "two columns of inset at {width}x{height}"
        );
        assert_eq!(
            (row.fg, row.bg, row.bold),
            (theme().ground, theme().cursor, true)
        );

        let (_, readme) =
            find_ascii(&buffer, width, height, "README.md").ok_or("missing readme")?;
        assert_eq!(panel_columns(&buffer, width, readme), Some(columns));
        for x in columns.0..=columns.1 {
            assert_eq!(buffer[(x, readme)].bg, theme().band, "({x}, {readme})");
        }
    }
    Ok(())
}

#[test]
fn the_citation_sources_underline_the_active_one() -> Result<(), Box<dyn Error>> {
    let mut session = with_commits(&["README.md"], &[("abc1234", "back off")]);
    press(&mut session, KeyCode::Char('@'));
    let styles = |session: &mut AuthoringSession| -> Result<_, Box<dyn Error>> {
        let buffer = rendered_buffer(session, 80, 24)?;
        let files = find_ascii(&buffer, 80, 24, "Files").ok_or("missing files")?;
        let commits = find_ascii(&buffer, 80, 24, "Commits").ok_or("missing commits")?;
        assert_eq!(commits.0, files.0 + 7, "two columns between the sources");
        Ok([files, commits].map(|position| {
            let cell = &buffer[position];
            (cell.fg, cell.modifier)
        }))
    };
    let active = (theme().text, Modifier::BOLD | Modifier::UNDERLINED);
    let inactive = (theme().dim, Modifier::empty());
    assert_eq!(styles(&mut session)?, [active, inactive]);
    press(&mut session, KeyCode::Right);
    assert_eq!(styles(&mut session)?, [inactive, active]);
    Ok(())
}

#[test]
fn the_selected_path_stays_visible_when_the_list_scrolls() -> Result<(), Box<dyn Error>> {
    let paths: Vec<String> = (0..12)
        .map(|index| format!("src/file{index:02}.rs"))
        .collect();
    let mut session = citing(&paths.iter().map(String::as_str).collect::<Vec<_>>());
    press(&mut session, KeyCode::Char('@'));
    let shown = |session: &mut AuthoringSession| -> Result<Vec<String>, Box<dyn Error>> {
        let rows = screen(session, 80, 24)?;
        Ok(rows
            .iter()
            .filter_map(|row| row.split_whitespace().find(|word| word.starts_with("src/")))
            .map(str::to_owned)
            .collect())
    };
    assert_eq!(shown(&mut session)?, paths[..8]);
    press(&mut session, KeyCode::Up);
    assert_eq!(
        shown(&mut session)?,
        paths[4..],
        "up from the top wraps to the last"
    );
    let buffer = rendered_buffer(&mut session, 80, 24)?;
    let (column, row) = find_ascii(&buffer, 80, 24, "src/file11.rs").ok_or("missing last")?;
    assert_eq!(buffer[(column, row)].bg, theme().cursor);
    press(&mut session, KeyCode::Up);
    assert_eq!(
        shown(&mut session)?,
        paths[4..],
        "moving inside the view does not scroll"
    );
    press(&mut session, KeyCode::Down);
    press(&mut session, KeyCode::Down);
    assert_eq!(
        shown(&mut session)?,
        paths[..8],
        "down from the last wraps to the top"
    );
    Ok(())
}

#[test]
fn both_pickers_draw_the_same_band_inset_and_highlight() -> Result<(), Box<dyn Error>> {
    for (width, height) in [(80, 24), (100, 30)] {
        let mut types = AuthoringSession::new(built_in_commit_types(), None);
        let types_buffer = rendered_buffer(&mut types, width, height)?;
        let type_row = selected_row(&types_buffer, width, height, "feat")?;

        let mut citation = citing(&["src/net/retry.rs"]);
        press(&mut citation, KeyCode::Char('@'));
        let citation_buffer = rendered_buffer(&mut citation, width, height)?;
        let citation_row = selected_row(&citation_buffer, width, height, "src/net/retry.rs")?;

        assert_eq!(type_row, citation_row, "at {width}x{height}");
    }
    Ok(())
}

/// The rows the citation panel covers, read from its band at `column`.
fn panel_rows(buffer: &Buffer, column: u16, height: u16) -> Vec<u16> {
    (0..height)
        .filter(|row| {
            let bg = buffer[(column, *row)].bg;
            bg == theme().band || bg == theme().cursor
        })
        .collect()
}

#[test]
fn the_citation_panel_keeps_its_height_as_the_query_narrows() -> Result<(), Box<dyn Error>> {
    let paths = [
        "src/a.rs",
        "src/b.rs",
        "src/c.rs",
        "README.md",
        "Cargo.toml",
    ];
    let mut session = citing(&paths);
    press(&mut session, KeyCode::Char('@'));
    let rows = |session: &mut AuthoringSession| -> Result<Vec<u16>, Box<dyn Error>> {
        Ok(panel_rows(&rendered_buffer(session, 80, 24)?, 2, 24))
    };
    let full = rows(&mut session)?;
    assert_eq!(
        full.len(),
        11,
        "five rows, the source row, the query row, and padding"
    );
    typed(&mut session, "src");
    assert_eq!(rows(&mut session)?, full, "three matches");
    typed(&mut session, "/a");
    assert_eq!(rows(&mut session)?, full, "one match");
    typed(&mut session, "zz");
    assert_eq!(rows(&mut session)?, full, "no match");
    Ok(())
}

#[test]
fn no_composer_text_shows_through_the_citation_panel() -> Result<(), Box<dyn Error>> {
    let mut session = citing(&["src/a.rs", "src/b.rs", "src/c.rs"]);
    press(&mut session, KeyCode::Down);
    paste(
        &mut session,
        &format!(
            "{}\nsee ",
            "qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq\n".repeat(3)
        ),
    );
    for _ in 0..3 {
        press(&mut session, KeyCode::Up);
    }
    press(&mut session, KeyCode::Char('@'));
    let buffer = rendered_buffer(&mut session, 80, 24)?;
    let rows = panel_rows(&buffer, 2, 24);
    assert!(!rows.is_empty());
    let (first, last) = panel_columns(&buffer, 80, rows[0]).ok_or("missing panel")?;
    for row in &rows {
        let text: String = (first..=last).map(|x| buffer[(x, *row)].symbol()).collect();
        assert!(!text.contains('q'), "row {row}: {text}");
    }
    assert!(
        rendered(&mut session, 80, 24)?.contains("qqqq"),
        "the body still shows outside the panel"
    );
    Ok(())
}

#[test]
fn the_citation_panel_follows_the_caret_after_the_composer_scrolls() -> Result<(), Box<dyn Error>> {
    let mut session = citing(&["src/net/retry.rs"]);
    press(&mut session, KeyCode::Down);
    let lines: Vec<String> = (0..40).map(|index| format!("line {index:02}")).collect();
    paste(&mut session, &lines.join("\n"));
    let open_at = |session: &mut AuthoringSession,
                   ups: usize,
                   caret_text: &str|
     -> Result<(u16, Vec<u16>, u16), Box<dyn Error>> {
        for _ in 0..ups {
            press(session, KeyCode::Up);
        }
        rendered(session, 80, 24)?;
        press(session, KeyCode::Char('@'));
        let buffer = rendered_buffer(session, 80, 24)?;
        assert!(
            find_ascii(&buffer, 80, 24, "line 00").is_none(),
            "the composer has scrolled"
        );
        let (_, caret) = find_ascii(&buffer, 80, 24, caret_text).ok_or("missing caret line")?;
        let (_, sources) = find_ascii(&buffer, 80, 24, "Files").ok_or("missing panel")?;
        press(session, KeyCode::Esc);
        Ok((caret, panel_rows(&buffer, 2, 24), sources))
    };
    let (caret, panel, _) = open_at(&mut session, 12, "line 27")?;
    assert_eq!(
        panel.last(),
        Some(&(caret - 2)),
        "above, one blank row over the caret"
    );
    let (caret, _, sources) = open_at(&mut session, 8, "line 19")?;
    assert_eq!(sources, caret + 3, "under, one blank row below the caret");
    Ok(())
}
