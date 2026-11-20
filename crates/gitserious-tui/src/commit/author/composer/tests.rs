use super::*;
use crate::commit::author::state::{AuthoringSession, Stage};
use crate::commit::author::tests::{
    alt, control, find_ascii, modified_press, paste, press, rendered, rendered_buffer,
    reversed_positions, typed, write, write_valid_feat,
};
use gitserious_core::built_in_commit_types;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use std::error::Error;

fn session() -> AuthoringSession {
    AuthoringSession::new(built_in_commit_types(), Some(0))
}

#[test]
fn typing_a_scope_wraps_it_and_space_moves_to_the_description() -> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), None);
    typed(&mut session, "fe");
    press(&mut session, KeyCode::Enter);
    modified_press(&mut session, KeyCode::Char('N'), KeyModifiers::SHIFT);
    typed(&mut session, "ét");

    assert!(rendered(&mut session, 80, 24)?.contains("feat(Nét): "));
    let buffer = rendered_buffer(&mut session, 80, 24)?;
    let (x, y) = find_ascii(&buffer, 80, 24, "feat(").ok_or("missing subject")?;
    assert_eq!(reversed_positions(&buffer, 80, 24), [(x + 8, y)]);
    assert_eq!(buffer[(x + 8, y)].symbol(), ")");

    typed(&mut session, " handle empty rows");
    write(
        &mut session,
        "",
        &["Explain the intent", "Record the decision"],
    );
    press(&mut session, KeyCode::Down);
    alt(&mut session, 'n');
    control(&mut session, 's');

    assert_eq!(session.stage, Stage::Review);
    assert!(rendered(&mut session, 80, 24)?.contains("feat(Nét): handle empty rows"));
    Ok(())
}

#[test]
fn typing_a_subject_allows_no_scope_or_explicit_punctuation() {
    for (keys, tail) in [
        (" describe the change", ": describe the change"),
        (": describe the change", ": describe the change"),
        (
            "(net stack): describe the change",
            "(net stack): describe the change",
        ),
    ] {
        let mut session = session();
        typed(&mut session, keys);
        assert_eq!(session.composer.subject.lines(), &[tail]);
        assert!(session.composer.subject_parts().is_ok());
    }
}

#[test]
fn space_can_extend_the_scope_of_a_complete_pasted_subject() {
    let mut session = session();
    paste(&mut session, "(net): handle empty rows");
    press(&mut session, KeyCode::Home);
    for _ in 0..4 {
        press(&mut session, KeyCode::Right);
    }
    typed(&mut session, " stack");
    assert_eq!(
        session.composer.subject.lines(),
        &["(net stack): handle empty rows"]
    );
}

#[test]
fn the_wrapped_first_letter_undoes_as_one_edit_and_a_cleared_scope_can_be_skipped() {
    let mut session = session();
    typed(&mut session, "n");
    control(&mut session, 'u');
    assert!(session.composer.subject.is_empty());

    typed(&mut session, "x");
    press(&mut session, KeyCode::Backspace);
    typed(&mut session, " describe the change");
    assert_eq!(session.composer.subject.lines(), &[": describe the change"]);
}

#[test]
fn the_type_is_fixed_and_the_subject_refuses_multiline_paste() {
    let mut session = session();
    paste(&mut session, ": a subject");
    press(&mut session, KeyCode::Home);
    press(&mut session, KeyCode::Backspace);
    assert_eq!(session.composer.definition().id().as_str(), "feat");
    assert_eq!(session.composer.subject.lines(), &[": a subject"]);
    paste(&mut session, "first\nsecond");
    assert_eq!(session.composer.subject.lines(), &[": a subject"]);
    control(&mut session, 'j');
    assert_eq!(session.composer.subject.lines(), &[": a subject"]);
    paste(&mut session, "first\rsecond");
    assert_eq!(session.composer.subject.lines(), &[": a subject"]);
}

#[test]
fn subjects_parse_scope_and_description_and_reject_malformed_tails() -> Result<(), Box<dyn Error>> {
    for tail in [
        "",
        "plain description",
        "(net: missing close",
        "(): empty scope",
        ": ",
    ] {
        assert!(parse_subject(tail).is_err(), "{tail:?}");
    }
    let (scope, subject) = parse_subject("(net): retry sooner")?;
    assert_eq!(scope.ok_or("scope")?.as_str(), "net");
    assert_eq!(subject.as_str(), "retry sooner");
    assert!(parse_subject(": retry sooner")?.0.is_none());
    Ok(())
}

#[test]
fn every_field_keeps_its_own_undo_and_redo_history() {
    let mut session = session();
    paste(&mut session, ": a subject");
    press(&mut session, KeyCode::Enter);
    paste(&mut session, "intent answer");
    press(&mut session, KeyCode::Down);
    paste(&mut session, "decision answer");
    control(&mut session, 'u');
    assert!(
        session
            .composer
            .text(MessageFieldId::Property(1))
            .is_empty()
    );
    assert_eq!(
        session.composer.text(MessageFieldId::Property(0)).lines(),
        &["intent answer"]
    );
    assert_eq!(session.composer.subject.lines(), &[": a subject"]);
    control(&mut session, 'r');
    assert_eq!(
        session.composer.text(MessageFieldId::Property(1)).lines(),
        &["decision answer"]
    );
}

#[test]
fn arrows_move_through_body_lines_then_across_field_edges() {
    let mut session = session();
    press(&mut session, KeyCode::Down);
    paste(&mut session, "first\nsecond");
    press(&mut session, KeyCode::Up);
    assert_eq!(session.composer.focus, MessageFieldId::Property(0));
    assert_eq!(session.composer.text(session.composer.focus).cursor().0, 0);
    press(&mut session, KeyCode::Up);
    assert_eq!(session.composer.focus, MessageFieldId::Subject);
    press(&mut session, KeyCode::Down);
    assert_eq!(
        session.composer.text(session.composer.focus).cursor(),
        (0, 0)
    );
    press(&mut session, KeyCode::Down);
    assert_eq!(session.composer.focus, MessageFieldId::Property(0));
    press(&mut session, KeyCode::Down);
    assert_eq!(session.composer.focus, MessageFieldId::Property(1));
    press(&mut session, KeyCode::Up);
    assert_eq!(
        session.composer.text(session.composer.focus).cursor(),
        (1, 6)
    );
}

#[test]
fn arrows_follow_wrapped_rows_before_leaving_a_body() -> Result<(), Box<dyn Error>> {
    let mut session = session();
    press(&mut session, KeyCode::Down);
    paste(&mut session, &"word ".repeat(60));
    rendered(&mut session, 80, 24)?;
    press(&mut session, KeyCode::Home);
    modified_press(&mut session, KeyCode::Home, KeyModifiers::CONTROL);
    press(&mut session, KeyCode::Down);
    assert_eq!(session.composer.focus, MessageFieldId::Property(0));
    assert!(session.composer.text(session.composer.focus).cursor().1 > 0);
    Ok(())
}

#[test]
fn enter_leaves_the_subject_and_inserts_newlines_in_bodies() {
    let mut session = session();
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.composer.focus, MessageFieldId::Property(0));
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.composer.focus, MessageFieldId::Property(0));
    assert_eq!(
        session.composer.text(session.composer.focus).lines(),
        &["", ""]
    );
    paste(&mut session, "a paragraph");
    press(&mut session, KeyCode::Enter);
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.composer.focus, MessageFieldId::Property(0));
    assert_eq!(
        session.composer.text(session.composer.focus).lines(),
        &["", "a paragraph", "", ""]
    );
}

#[test]
fn conditional_answers_control_status_blockers_and_footer() {
    let mut session = session();
    write(&mut session, ": a subject", &["intent", "decision"]);
    assert_eq!(
        session.composer.marker(MessageFieldId::Property(2)),
        FieldMarker::Undecided
    );
    assert_eq!(session.composer.to_go(), [MessageFieldId::Property(2)]);
    press(&mut session, KeyCode::Down);
    assert!(crate::commit::author::render::hints(&session).contains(&("alt+a", "applies")));
    alt(&mut session, 'a');
    assert_eq!(session.composer.to_go(), [MessageFieldId::Property(2)]);
    paste(&mut session, "a constraint");
    assert!(session.composer.to_go().is_empty());
    alt(&mut session, 'n');
    assert!(
        session
            .composer
            .text(MessageFieldId::Property(2))
            .is_empty()
    );
    assert!(session.composer.to_go().is_empty());
    assert_eq!(
        session.composer.properties[2].requirement.word(),
        "doesn't apply"
    );
    press(&mut session, KeyCode::Down);
    assert_eq!(
        session.composer.marker(MessageFieldId::Property(2)),
        FieldMarker::Done
    );
    assert!(
        !crate::commit::author::render::hints(&session)
            .iter()
            .any(|(key, _)| *key == "alt+a")
    );
}

#[test]
fn writing_a_conditional_answer_marks_it_as_applicable() {
    let mut session = session();
    session.composer.focus = MessageFieldId::Property(2);
    paste(&mut session, "a constraint");
    assert_eq!(session.composer.properties[2].requirement.word(), "applies");
    alt(&mut session, 'n');
    control(&mut session, 'u');
    assert_eq!(
        session.composer.text(MessageFieldId::Property(2)).lines(),
        &["a constraint"]
    );
    assert_eq!(session.composer.properties[2].requirement.word(), "applies");
}

#[test]
fn review_errors_focus_the_first_blocked_field_and_clear_when_edited() {
    let mut session = session();
    paste(&mut session, ": a subject");
    control(&mut session, 's');
    assert_eq!(session.stage, Stage::Compose);
    assert_eq!(session.composer.focus, MessageFieldId::Property(0));
    assert!(
        session
            .composer
            .feedback
            .iter()
            .any(|feedback| feedback.field == MessageFieldId::Property(0))
    );
    paste(&mut session, "intent answer");
    assert!(
        session
            .composer
            .feedback
            .iter()
            .all(|feedback| feedback.field != MessageFieldId::Property(0))
    );
    control(&mut session, 's');
    assert_eq!(session.composer.focus, MessageFieldId::Property(1));
}

#[test]
fn fields_show_markers_requirements_guidance_counters_and_no_metadata() -> Result<(), Box<dyn Error>>
{
    for (width, height) in [(80, 24), (100, 30)] {
        let mut session = session();
        write(&mut session, "(net): retry sooner", &["why it matters"]);
        press(&mut session, KeyCode::Down);
        let buffer = rendered_buffer(&mut session, width, height)?;
        let text: String = buffer
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        for expected in [
            "✓",
            "●",
            "◐",
            "○",
            "required",
            "conditional",
            "optional",
            "intent:",
            "decision:",
            "constraints:",
            "breaking-change:",
            "23/72",
            "2 to go: decision, constraints",
            "col 1/80",
        ] {
            assert!(
                text.contains(expected),
                "missing {expected:?} at {width}x{height}: {text}"
            );
        }
        assert!(text.contains("Why this implementation beat reasonable alternatives."));
        for forbidden in [
            "Gitserious-Taxonomy",
            "Gitserious-Schema",
            "alt+a",
            "# cite",
            "ctrl+t",
        ] {
            assert!(!text.contains(forbidden));
        }
        let (x, y) = find_ascii(&buffer, width, height, "decision:").ok_or("decision heading")?;
        assert_eq!(buffer[(x - 3, y)].symbol(), "▎");
        assert!(!reversed_positions(&buffer, width, height).is_empty());
    }
    Ok(())
}

#[test]
fn long_body_scrolls_with_the_cursor_and_ready_status_survives() -> Result<(), Box<dyn Error>> {
    for (width, height) in [(80, 24), (100, 30)] {
        let mut session = session();
        write_valid_feat(&mut session);
        assert!(rendered(&mut session, width, height)?.contains("ready for review"));
        session.composer.focus = MessageFieldId::Property(0);
        paste(&mut session, &"\nbody line".repeat(50));
        let buffer = rendered_buffer(&mut session, width, height)?;
        assert!(session.composer.layout().scroll_offset > 0);
        assert!(!reversed_positions(&buffer, width, height).is_empty());
        modified_press(&mut session, KeyCode::Home, KeyModifiers::CONTROL);
        let buffer = rendered_buffer(&mut session, width, height)?;
        assert!(!reversed_positions(&buffer, width, height).is_empty());
    }
    Ok(())
}

#[test]
fn every_requirement_word_is_visible_and_only_required_and_undecided_fields_block_review()
-> Result<(), Box<dyn Error>> {
    let definition = crate::commit::author::tests::presentation_definition()?;
    let mut session = AuthoringSession::new(&[definition], Some(0));
    paste(&mut session, ": describe the change");
    let text = rendered(&mut session, 100, 30)?;
    for word in ["required", "recommended", "optional", "conditional"] {
        assert!(text.contains(word));
    }
    assert_eq!(
        session.composer.to_go(),
        [MessageFieldId::Property(0), MessageFieldId::Property(3)]
    );
    Ok(())
}

#[test]
fn a_long_subject_keeps_its_counter_and_cursor_visible() -> Result<(), Box<dyn Error>> {
    let mut session = session();
    paste(&mut session, &format!(": {}", "x".repeat(160)));
    let buffer = rendered_buffer(&mut session, 80, 24)?;
    assert!(find_ascii(&buffer, 80, 24, "166/72").is_some());
    assert!(!reversed_positions(&buffer, 80, 24).is_empty());
    Ok(())
}

#[test]
fn a_resize_rewraps_the_body_so_down_and_the_caret_follow_the_new_rows()
-> Result<(), Box<dyn Error>> {
    let mut session = session();
    press(&mut session, KeyCode::Down);
    paste(&mut session, &"word ".repeat(30));
    let down_from_the_start = |session: &mut AuthoringSession,
                               width: u16,
                               height: u16|
     -> Result<usize, Box<dyn Error>> {
        modified_press(session, KeyCode::Home, KeyModifiers::CONTROL);
        rendered(session, width, height)?;
        press(session, KeyCode::Down);
        Ok(session.composer.text(session.composer.focus).cursor().1)
    };
    let wide = down_from_the_start(&mut session, 100, 30)?;
    let narrow = down_from_the_start(&mut session, 60, 24)?;
    assert!(narrow > 0 && narrow < wide, "{narrow} then {wide}");
    assert_eq!(narrow % 5, 0, "a row starts at a word");

    let buffer = rendered_buffer(&mut session, 60, 24)?;
    let caret = reversed_positions(&buffer, 60, 24);
    assert_eq!(caret.len(), 1, "{caret:?}");
    assert_eq!(
        buffer[caret[0]].symbol(),
        "w",
        "the caret sits on the row's first word"
    );
    let first = find_ascii(&buffer, 60, 24, "word").ok_or("missing body")?;
    assert_eq!(
        caret[0],
        (first.0, first.1 + 1),
        "the caret starts the second row"
    );
    assert_eq!(
        down_from_the_start(&mut session, 100, 30)?,
        wide,
        "widening again restores the wide rows"
    );
    Ok(())
}
