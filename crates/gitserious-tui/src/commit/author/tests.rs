use std::error::Error;

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

use super::cards::CardId;
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
/// turn, moving down to the next card the way a user does.
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

/// A type with one property of each policy, in policy order.
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
    let edited = session.cards.subject.lines().to_vec();
    press(&mut session, KeyCode::Esc);
    assert_eq!(session.stage, Stage::Confirm);
    press(&mut session, KeyCode::Char('n'));
    assert_eq!(session.cards.subject.lines(), edited);
    press(&mut session, KeyCode::Esc);
    press(&mut session, KeyCode::Char('y'));
    assert_eq!(session.stage, Stage::SelectType);
    press(&mut session, KeyCode::Right);
    assert_eq!(session.taxonomy().ok_or("missing taxonomy")?.id(), &ids[2]);
    assert!(!session.cards.dirty());
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
    session.cards.focus = super::cards::CardId::Subject;
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
    assert_eq!(session.cards.subject.lines(), [": compose durable message"]);
    assert_eq!(
        session.cards.properties[1].body.lines(),
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
    assert_eq!(session.cards.subject.lines(), ["(tui editor): ship editor"]);
    assert_eq!(
        session.cards.breaking_change.body.lines(),
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
    assert_eq!(session.cards.subject.lines(), ["(api \t): ship it  "]);
    assert_eq!(
        session.cards.properties[0].body.lines(),
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
        .ok_or("missing focused decision card")?;
    let intent_heading = compose
        .iter()
        .position(|row| row.starts_with(" ✓ intent:"))
        .ok_or("missing intent card")?;
    assert!(intent_heading < heading);
    assert_eq!(
        compose[intent_heading + 1],
        format!("   {}", "x".repeat(width - 1)),
        "the card wraps where the message does"
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
    assert!(!unpinned.cards.dirty());
    press(&mut unpinned, KeyCode::Esc);
    assert_eq!(unpinned.stage, Stage::SelectType);

    press(&mut unpinned, KeyCode::Enter);
    paste(&mut unpinned, ": dirty");
    press(&mut unpinned, KeyCode::Esc);
    assert_eq!(unpinned.stage, Stage::Confirm);
    assert_eq!(unpinned.confirmation, ConfirmationAction::ChangeType);
    press(&mut unpinned, KeyCode::Enter);
    assert_eq!(unpinned.stage, Stage::Compose);
    assert_eq!(unpinned.cards.subject.lines(), [": dirty"]);
    press(&mut unpinned, KeyCode::Esc);
    press(&mut unpinned, KeyCode::Char('y'));
    assert_eq!(unpinned.stage, Stage::SelectType);
    assert!(!unpinned.cards.dirty());

    let mut decided = AuthoringSession::new(definitions, Some(0));
    for _ in 0..3 {
        press(&mut decided, KeyCode::Down);
    }
    alt(&mut decided, 'n');
    assert!(decided.cards.dirty(), "a decision is work to lose");

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
    assert_eq!(session.cards.subject.lines(), [": compose durable message"]);
    assert_eq!(
        session.cards.properties[0].body.lines(),
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
        Some((3, 4)),
    );
    let selected = find_ascii(&picker_buffer, 100, 24, "feat").ok_or("missing selection")?;
    assert_eq!(selected, (3, 6));
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
    assert!(text.contains("intent:"), "the cards stay under the dialog");
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
        "the cards scroll, so they fit the picker's minimum"
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
    assert!(!session.cards.dirty());
    assert!(session.handle_event(Event::Resize(120, 40)).is_none());

    let mut picker = AuthoringSession::new(built_in_commit_types(), None);
    paste(&mut picker, "ignored");
    assert!(!picker.cards.dirty());

    let mut review = valid_feat_session();
    let message = review_text(&review)?;
    paste(&mut review, "ignored");
    assert_eq!(review_text(&review)?, message);
    press(&mut review, KeyCode::Esc);
    assert_eq!(review.cards.subject.lines(), [": compose durable message"]);
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
    assert_eq!(session.cards.definition().id().as_str(), "feat");
    assert_eq!(session.cards.subject.lines(), ["(net): retry sooner"]);
    assert_eq!(
        session.cards.text(CardId::Property(0)).lines(),
        ["why it exists"]
    );

    control(&mut session, 't');
    typed(&mut session, "fi");
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.stage, Stage::Compose);
    assert_eq!(session.cards.definition().id().as_str(), "fix");
    assert_eq!(session.cards.subject.lines(), ["(net): retry sooner"]);
    assert_eq!(
        session
            .cards
            .properties
            .iter()
            .map(|card| card.body.lines().join("\n"))
            .collect::<Vec<_>>(),
        vec!["", "this repair", "a bound"]
    );
    assert_eq!(
        session.cards.properties[2].policy.word(),
        "applies",
        "carried constraint text answers that the property applies"
    );
    assert_eq!(
        session.cards.text(CardId::BreakingChange).lines(),
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
            .cards
            .notes
            .iter()
            .any(|note| note.message.contains("cause"))
    );
    assert!(
        session
            .cards
            .notes
            .iter()
            .all(|note| !note.message.contains("intent"))
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
    assert_eq!(session.cards.definition().id().as_str(), "feat");
    assert_eq!(
        session.cards.text(CardId::Property(0)).lines(),
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
    assert_eq!(session.cards.definition().id().as_str(), "feat");
    assert_eq!(
        session.cards.text(CardId::Property(0)).lines(),
        ["why it exists"]
    );
    Ok(())
}
