use super::*;
use crate::commit::author::state::{AuthoringSession, Stage};
use crate::commit::author::tests::{
    alt, control, find_ascii, modified_press, paste, press, rendered, rendered_buffer,
    reversed_positions, write, write_valid_feat,
};
use gitserious_core::built_in_commit_types;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use std::error::Error;

fn session() -> AuthoringSession {
    AuthoringSession::new(built_in_commit_types(), Some(0))
}

#[test]
fn the_type_is_fixed_and_the_subject_refuses_multiline_paste() {
    let mut session = session();
    paste(&mut session, ": a subject");
    press(&mut session, KeyCode::Home);
    press(&mut session, KeyCode::Backspace);
    assert_eq!(session.cards.definition().id().as_str(), "feat");
    assert_eq!(session.cards.subject.lines(), &[": a subject"]);
    paste(&mut session, "first\nsecond");
    assert_eq!(session.cards.subject.lines(), &[": a subject"]);
    control(&mut session, 'j');
    assert_eq!(session.cards.subject.lines(), &[": a subject"]);
    paste(&mut session, "first\rsecond");
    assert_eq!(session.cards.subject.lines(), &[": a subject"]);
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
fn every_card_keeps_its_own_undo_and_redo_history() {
    let mut session = session();
    paste(&mut session, ": a subject");
    press(&mut session, KeyCode::Enter);
    paste(&mut session, "intent answer");
    press(&mut session, KeyCode::Down);
    paste(&mut session, "decision answer");
    control(&mut session, 'u');
    assert!(session.cards.text(CardId::Property(1)).is_empty());
    assert_eq!(
        session.cards.text(CardId::Property(0)).lines(),
        &["intent answer"]
    );
    assert_eq!(session.cards.subject.lines(), &[": a subject"]);
    control(&mut session, 'r');
    assert_eq!(
        session.cards.text(CardId::Property(1)).lines(),
        &["decision answer"]
    );
}

#[test]
fn arrows_move_through_body_lines_then_across_card_edges() {
    let mut session = session();
    press(&mut session, KeyCode::Down);
    paste(&mut session, "first\nsecond");
    press(&mut session, KeyCode::Up);
    assert_eq!(session.cards.focus, CardId::Property(0));
    assert_eq!(session.cards.text(session.cards.focus).cursor().0, 0);
    press(&mut session, KeyCode::Up);
    assert_eq!(session.cards.focus, CardId::Subject);
    press(&mut session, KeyCode::Down);
    assert_eq!(session.cards.text(session.cards.focus).cursor(), (0, 0));
    press(&mut session, KeyCode::Down);
    assert_eq!(session.cards.focus, CardId::Property(0));
    press(&mut session, KeyCode::Down);
    assert_eq!(session.cards.focus, CardId::Property(1));
    press(&mut session, KeyCode::Up);
    assert_eq!(session.cards.text(session.cards.focus).cursor(), (1, 6));
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
    assert_eq!(session.cards.focus, CardId::Property(0));
    assert!(session.cards.text(session.cards.focus).cursor().1 > 0);
    Ok(())
}

#[test]
fn enter_leaves_the_subject_and_inserts_newlines_in_bodies() {
    let mut session = session();
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.cards.focus, CardId::Property(0));
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.cards.focus, CardId::Property(0));
    assert_eq!(session.cards.text(session.cards.focus).lines(), &["", ""]);
    paste(&mut session, "a paragraph");
    press(&mut session, KeyCode::Enter);
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.cards.focus, CardId::Property(0));
    assert_eq!(
        session.cards.text(session.cards.focus).lines(),
        &["", "a paragraph", "", ""]
    );
}

#[test]
fn conditional_answers_control_status_blockers_and_footer() {
    let mut session = session();
    write(&mut session, ": a subject", &["intent", "decision"]);
    assert_eq!(session.cards.mark(CardId::Property(2)), Mark::Undecided);
    assert_eq!(session.cards.to_go(), [CardId::Property(2)]);
    press(&mut session, KeyCode::Down);
    assert!(crate::commit::author::render::hints(&session).contains(&("alt+a", "applies")));
    alt(&mut session, 'a');
    assert_eq!(session.cards.to_go(), [CardId::Property(2)]);
    paste(&mut session, "a constraint");
    assert!(session.cards.to_go().is_empty());
    alt(&mut session, 'n');
    assert!(session.cards.text(CardId::Property(2)).is_empty());
    assert!(session.cards.to_go().is_empty());
    assert_eq!(session.cards.properties[2].policy.word(), "doesn't apply");
    press(&mut session, KeyCode::Down);
    assert_eq!(session.cards.mark(CardId::Property(2)), Mark::Done);
    assert!(
        !crate::commit::author::render::hints(&session)
            .iter()
            .any(|(key, _)| *key == "alt+a")
    );
}

#[test]
fn writing_a_conditional_answer_marks_it_as_applicable() {
    let mut session = session();
    session.cards.focus = CardId::Property(2);
    paste(&mut session, "a constraint");
    assert_eq!(session.cards.properties[2].policy.word(), "applies");
    alt(&mut session, 'n');
    control(&mut session, 'u');
    assert_eq!(
        session.cards.text(CardId::Property(2)).lines(),
        &["a constraint"]
    );
    assert_eq!(session.cards.properties[2].policy.word(), "applies");
}

#[test]
fn review_errors_focus_the_first_blocked_card_and_clear_when_edited() {
    let mut session = session();
    paste(&mut session, ": a subject");
    control(&mut session, 's');
    assert_eq!(session.stage, Stage::Compose);
    assert_eq!(session.cards.focus, CardId::Property(0));
    assert!(
        session
            .cards
            .notes
            .iter()
            .any(|note| note.card == CardId::Property(0))
    );
    paste(&mut session, "intent answer");
    assert!(
        session
            .cards
            .notes
            .iter()
            .all(|note| note.card != CardId::Property(0))
    );
    control(&mut session, 's');
    assert_eq!(session.cards.focus, CardId::Property(1));
}

#[test]
fn cards_show_marks_policy_guidance_counters_and_no_metadata() -> Result<(), Box<dyn Error>> {
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
        session.cards.focus = CardId::Property(0);
        paste(&mut session, &"\nbody line".repeat(50));
        let buffer = rendered_buffer(&mut session, width, height)?;
        assert!(session.cards.scroll > 0);
        assert!(!reversed_positions(&buffer, width, height).is_empty());
        modified_press(&mut session, KeyCode::Home, KeyModifiers::CONTROL);
        let buffer = rendered_buffer(&mut session, width, height)?;
        assert!(!reversed_positions(&buffer, width, height).is_empty());
    }
    Ok(())
}

#[test]
fn every_policy_word_is_visible_and_only_required_and_undecided_cards_block_review()
-> Result<(), Box<dyn Error>> {
    let definition = crate::commit::author::tests::presentation_definition()?;
    let mut session = AuthoringSession::new(&[definition], Some(0));
    paste(&mut session, ": describe the change");
    let text = rendered(&mut session, 100, 30)?;
    for word in ["required", "recommended", "optional", "conditional"] {
        assert!(text.contains(word));
    }
    assert_eq!(
        session.cards.to_go(),
        [CardId::Property(0), CardId::Property(3)]
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
