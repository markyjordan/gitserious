use std::error::Error;

use gitserious_app::{CommitDraftAuthor, CommitDraftAuthorOutcome};
use gitserious_core::{
    CommitTypeDefinition, CommitTypeId, ConditionId, PropertyCondition, PropertyDefinition,
    PropertyKey, PropertyMultiplicity, PropertyRequirement, SchemaVersion, built_in_commit_types,
    render_commit_message,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

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
    AuthoringSession, ConfirmationAction, FieldKind, Keymap, Stage, VimMode,
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

fn valid_feat_session() -> AuthoringSession<'static> {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    press(&mut session, KeyCode::Tab);
    paste(&mut session, "compose durable message");
    press(&mut session, KeyCode::Tab);
    paste(&mut session, "explain intent 🦀");
    press(&mut session, KeyCode::Tab);
    paste(&mut session, "first line\nsecond line");
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
    press(&mut session, KeyCode::Char('j'));
    assert_eq!(session.selected_type, 1);
    press(&mut session, KeyCode::Char('k'));
    press(&mut session, KeyCode::End);
    assert_eq!(session.selected_type, definitions.len() - 1);
    press(&mut session, KeyCode::Home);
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
fn preselection_skips_only_the_picker_and_field_navigation_wraps() {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(1));
    assert_eq!(session.stage, Stage::Compose);
    assert!(session.preselected);
    assert_eq!(session.definition().id().as_str(), "fix");
    assert_eq!(session.composer.focused, 0);

    press(&mut session, KeyCode::BackTab);
    assert_eq!(session.composer.focused, session.composer.fields.len() - 1);
    press(&mut session, KeyCode::Tab);
    assert_eq!(session.composer.focused, 0);
    press(&mut session, KeyCode::Enter);
    assert_eq!(session.composer.focused, 1);
}

#[test]
fn invalid_review_submission_reports_all_blocking_fields_and_focuses_the_first() {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    modified_press(&mut session, KeyCode::Char('s'), KeyModifiers::CONTROL);

    assert_eq!(session.stage, Stage::Compose);
    assert_eq!(session.composer.focused, 1);
    assert_eq!(session.composer.issues.len(), 3);
    assert!(session.composer.issues.iter().any(|issue| issue.field == 1));
    assert!(session.composer.issues.iter().any(|issue| issue.field == 2));
    assert!(session.composer.issues.iter().any(|issue| issue.field == 3));
    assert!(session.review.is_none());
}

#[test]
fn review_is_exact_backtracking_is_lossless_and_enter_returns_the_typed_draft()
-> Result<(), Box<dyn Error>> {
    let mut session = valid_feat_session();
    let expected = "feat: compose durable message\n\nintent:\n  explain intent 🦀\n\nbehavior:\n  first line\n  second line\n";
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
    assert_eq!(session.composer.fields[1].text(), "compose durable message");
    assert_eq!(session.composer.fields[3].text(), "first line\nsecond line");
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
fn conventional_editing_supports_unicode_selection_words_paste_and_history() {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    press(&mut session, KeyCode::Tab);
    paste(&mut session, "alpha beta 🦀");
    let end = session.composer.current().editor.cursor();
    modified_press(&mut session, KeyCode::Left, KeyModifiers::CONTROL);
    let word = session.composer.current().editor.cursor();
    assert!(word.1 < end.1);

    modified_press(&mut session, KeyCode::Home, KeyModifiers::SHIFT);
    press(&mut session, KeyCode::Backspace);
    let selected_deleted = session.composer.current().text();
    assert!(selected_deleted.len() < "alpha beta 🦀".len());
    modified_press(&mut session, KeyCode::Char('u'), KeyModifiers::CONTROL);
    assert_eq!(session.composer.current().text(), "alpha beta 🦀");
    modified_press(&mut session, KeyCode::Char('r'), KeyModifiers::CONTROL);
    assert_eq!(session.composer.current().text(), selected_deleted);

    press(&mut session, KeyCode::Tab);
    paste(&mut session, "line one\nline two 🦀");
    assert_eq!(session.composer.current().text(), "line one\nline two 🦀");
}

#[test]
fn bounded_vim_mode_supports_all_documented_normal_and_insert_commands() {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    press(&mut session, KeyCode::Tab);
    press(&mut session, KeyCode::Tab);
    paste(&mut session, "one two\nthree");
    press(&mut session, KeyCode::F(2));
    assert_eq!(session.keymap, Keymap::Vim);
    assert_eq!(session.vim_mode, VimMode::Normal);

    press(&mut session, KeyCode::Char('0'));
    press(&mut session, KeyCode::Char('k'));
    press(&mut session, KeyCode::Char('l'));
    press(&mut session, KeyCode::Char('h'));
    press(&mut session, KeyCode::Char('j'));
    press(&mut session, KeyCode::Char('b'));
    press(&mut session, KeyCode::Char('w'));
    press(&mut session, KeyCode::Char('$'));
    assert_eq!(session.composer.current().editor.cursor(), (1, 5));

    press(&mut session, KeyCode::Char('i'));
    assert_eq!(session.vim_mode, VimMode::Insert);
    paste(&mut session, "!");
    press(&mut session, KeyCode::Esc);
    press(&mut session, KeyCode::Char('0'));
    press(&mut session, KeyCode::Char('x'));
    assert_eq!(session.composer.current().text(), "one two\nhree!");
    press(&mut session, KeyCode::Char('u'));
    assert_eq!(session.composer.current().text(), "one two\nthree!");
    modified_press(&mut session, KeyCode::Char('r'), KeyModifiers::CONTROL);
    assert_eq!(session.composer.current().text(), "one two\nhree!");

    press(&mut session, KeyCode::Char('a'));
    assert_eq!(session.vim_mode, VimMode::Insert);
    paste(&mut session, "X");
    assert_eq!(session.composer.current().text(), "one two\nhXree!");
    press(&mut session, KeyCode::F(2));
    assert_eq!(session.keymap, Keymap::Conventional);
}

#[test]
fn repeatable_properties_keep_independent_ordered_buffers() -> Result<(), Box<dyn Error>> {
    let definitions = vec![repeatable_definition()?];
    let mut session = AuthoringSession::new(&definitions, Some(0));
    press(&mut session, KeyCode::Tab);
    paste(&mut session, "collect evidence");
    press(&mut session, KeyCode::Tab);
    paste(&mut session, "first");
    modified_press(&mut session, KeyCode::Char('n'), KeyModifiers::CONTROL);
    paste(&mut session, "second\nline");
    modified_press(&mut session, KeyCode::Char('n'), KeyModifiers::CONTROL);
    paste(&mut session, "discard me");
    modified_press(&mut session, KeyCode::Char('d'), KeyModifiers::CONTROL);

    assert_eq!(session.composer.fields.len(), 4);
    assert_eq!(session.composer.fields[2].text(), "first");
    assert_eq!(session.composer.fields[3].text(), "second\nline");
    assert_eq!(
        session.composer.fields[3].kind,
        FieldKind::Property {
            definition_index: 0,
            value_index: 1,
        }
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
        "custom: collect evidence\n\nevidence:\n  first\n\nevidence:\n  second\n  line\n"
    );
    Ok(())
}

#[test]
fn untouched_and_dirty_cancellation_follow_distinct_confirmation_paths() {
    let definitions = built_in_commit_types();
    let mut unpinned = AuthoringSession::new(definitions, None);
    press(&mut unpinned, KeyCode::Enter);
    press(&mut unpinned, KeyCode::Esc);
    assert_eq!(unpinned.stage, Stage::SelectType);

    press(&mut unpinned, KeyCode::Enter);
    paste(&mut unpinned, "dirty");
    press(&mut unpinned, KeyCode::Esc);
    assert_eq!(unpinned.stage, Stage::Confirm);
    assert_eq!(unpinned.confirmation, ConfirmationAction::ChangeType);
    press(&mut unpinned, KeyCode::Enter);
    assert_eq!(unpinned.stage, Stage::Compose);
    assert_eq!(unpinned.composer.current().text(), "dirty");
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
fn property_metadata_and_validation_markers_render_from_the_schema() -> Result<(), Box<dyn Error>> {
    let definitions = vec![presentation_definition()?];
    let mut session = AuthoringSession::new(&definitions, Some(0));
    let expectations = [
        (2, "required-field", "required"),
        (3, "recommended-field", "recommended"),
        (4, "optional-field", "optional"),
        (
            5,
            "conditional-field",
            "required when the condition applies",
        ),
    ];
    for (focused, key, metadata) in expectations {
        session.composer.focused = focused;
        let text = rendered(&mut session, 120, 32)?;
        assert!(text.contains(key));
        assert!(text.contains(metadata));
        assert!(text.contains(&format!("Description for {key}.")));
    }

    modified_press(&mut session, KeyCode::Char('s'), KeyModifiers::CONTROL);
    let text = rendered(&mut session, 120, 32)?;
    assert!(text.contains("! subject"));
    assert!(text.contains("! required-field"));
    assert!(text.contains("○ recommended-field"));
    assert!(text.contains("○ optional-field"));
    assert!(text.contains("○ conditional-field"));
    Ok(())
}

#[test]
fn every_stage_and_responsive_boundary_render_with_test_backend() -> Result<(), Box<dyn Error>> {
    let mut picker = AuthoringSession::new(built_in_commit_types(), None);
    let text = rendered(&mut picker, 100, 24)?;
    assert!(text.contains("gitserious commit"));
    assert!(text.contains("Commit types"));
    assert!(text.contains("Enter select"));

    let mut composer = AuthoringSession::new(built_in_commit_types(), Some(0));
    let text = rendered(&mut composer, 60, 18)?;
    assert!(text.contains("Compose commit"));
    assert!(text.contains("Keymap: conventional"));
    assert!(!composer.too_small);

    let mut review = valid_feat_session();
    let text = rendered(&mut review, 100, 24)?;
    assert!(text.contains("Review commit"));
    assert!(text.contains("feat: compose durable message"));
    assert!(text.contains("Enter commit"));

    press(&mut review, KeyCode::Char('q'));
    let text = rendered(&mut review, 100, 24)?;
    assert!(text.contains("Confirm discard"));
    assert!(text.contains("Discard this draft and cancel"));

    for (width, height) in [(59, 24), (100, 17)] {
        let mut too_small = AuthoringSession::new(built_in_commit_types(), Some(0));
        let text = rendered(&mut too_small, width, height)?;
        assert!(too_small.too_small);
        assert!(text.contains("Terminal too small"));
        assert!(text.contains("Esc/q to cancel"));
        assert_eq!(
            too_small.handle_event(key(KeyCode::Esc)),
            Some(CommitDraftAuthorOutcome::Cancelled)
        );
    }
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
fn non_press_resize_and_paste_events_obey_stage_and_mode_boundaries() {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    let release = Event::Key(KeyEvent::new_with_kind(
        KeyCode::Char('x'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    ));
    assert!(session.handle_event(release).is_none());
    assert!(!session.composer.dirty());
    assert!(session.handle_event(Event::Resize(120, 40)).is_none());

    press(&mut session, KeyCode::F(2));
    assert_eq!(session.vim_mode, VimMode::Normal);
    assert!(
        session
            .handle_event(Event::Paste("ignored".to_owned()))
            .is_none()
    );
    assert!(!session.composer.dirty());
}
