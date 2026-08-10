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
use tui_textarea::{CursorMove, TextArea, WrapMode};

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
    AuthoringSession, ConfirmationAction, FieldId, FieldKind, FieldStatus, Keymap, Stage, VimMode,
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

fn row_text(buffer: &Buffer, width: u16, row: u16) -> String {
    (0..width)
        .map(|column| buffer[(column, row)].symbol())
        .collect()
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
    "scope:\n\n\nsubject:\ncompose durable message\n\nintent:\nexplain intent 🦀\n\nbehavior:\nfirst line\nsecond line\n\nconstraints:\n\n\ninvariants:\n\n\nvalidation:"
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
fn schema_form_is_prepopulated_in_order_and_starts_under_subject() -> Result<(), Box<dyn Error>> {
    let definitions = vec![presentation_definition()?];
    let session = AuthoringSession::new(&definitions, Some(0));
    assert_eq!(session.stage, Stage::Compose);
    assert!(session.preselected);
    assert_eq!(session.composer.editor.cursor(), (4, 0));
    assert!(!session.composer.dirty());
    assert_eq!(
        session.composer.editor.lines(),
        [
            "scope:",
            "",
            "",
            "subject:",
            "",
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
        ]
    );
    Ok(())
}

#[test]
fn invalid_review_marks_every_blocker_and_moves_to_the_first() {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    modified_press(&mut session, KeyCode::Char('s'), KeyModifiers::CONTROL);

    assert_eq!(session.stage, Stage::Compose);
    assert_eq!(session.composer.editor.cursor(), (4, 0));
    assert_eq!(session.composer.issues.len(), 3);
    for field in [FieldId::Subject, FieldId::Property(0), FieldId::Property(1)] {
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
fn conventional_document_editing_preserves_ctrl_k_unicode_paste_and_history() {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    paste(&mut session, "alpha beta 🦀");
    let end = session.composer.editor.cursor();
    modified_press(&mut session, KeyCode::Left, KeyModifiers::CONTROL);
    assert!(session.composer.editor.cursor().1 < end.1);

    modified_press(&mut session, KeyCode::Char('k'), KeyModifiers::CONTROL);
    assert_eq!(session.composer.editor.lines()[4], "alpha beta ");
    modified_press(&mut session, KeyCode::Char('u'), KeyModifiers::CONTROL);
    assert_eq!(session.composer.editor.lines()[4], "alpha beta 🦀");
    modified_press(&mut session, KeyCode::Char('r'), KeyModifiers::CONTROL);
    assert_eq!(session.composer.editor.lines()[4], "alpha beta ");

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
fn bounded_vim_mode_uses_ctrl_t_and_supports_document_commands() {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    paste(&mut session, "one two\nthree");
    modified_press(&mut session, KeyCode::Char('t'), KeyModifiers::CONTROL);
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
    press(&mut session, KeyCode::Char('i'));
    assert_eq!(session.vim_mode, VimMode::Insert);
    paste(&mut session, "!");
    press(&mut session, KeyCode::Esc);
    press(&mut session, KeyCode::Char('0'));
    press(&mut session, KeyCode::Char('x'));
    press(&mut session, KeyCode::Char('u'));
    modified_press(&mut session, KeyCode::Char('r'), KeyModifiers::CONTROL);
    press(&mut session, KeyCode::Char('a'));
    assert_eq!(session.vim_mode, VimMode::Insert);
    paste(&mut session, "X");

    modified_press(&mut session, KeyCode::Char('t'), KeyModifiers::CONTROL);
    assert_eq!(session.keymap, Keymap::Conventional);
    press(&mut session, KeyCode::F(2));
    assert_eq!(session.keymap, Keymap::Conventional);
}

#[test]
fn schema_headings_are_immutable_across_editing_modes() -> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    let pristine = session.composer.editor.lines().to_vec();
    let subject_line = pristine
        .iter()
        .position(|line| line == "subject:")
        .ok_or("subject heading")?;
    let subject_line = u16::try_from(subject_line)?;

    session
        .composer
        .editor
        .move_cursor(CursorMove::Jump(subject_line, 0));
    press(&mut session, KeyCode::Right);
    assert_eq!(
        session.composer.editor.cursor(),
        (usize::from(subject_line), "subject:".chars().count())
    );
    session
        .composer
        .editor
        .move_cursor(CursorMove::Jump(subject_line, 0));
    press(&mut session, KeyCode::Char('x'));
    assert_eq!(session.composer.editor.lines(), pristine);

    press(&mut session, KeyCode::Delete);
    assert_eq!(session.composer.editor.lines(), pristine);

    session
        .composer
        .editor
        .move_cursor(CursorMove::Jump(subject_line.saturating_add(1), 0));
    press(&mut session, KeyCode::Backspace);
    assert_eq!(session.composer.editor.lines(), pristine);

    paste(&mut session, "scope:\n");
    assert_eq!(session.composer.editor.lines(), pristine);

    session.composer.editor.start_selection();
    session
        .composer
        .editor
        .move_cursor(CursorMove::Jump(subject_line, 0));
    press(&mut session, KeyCode::Backspace);
    assert_eq!(session.composer.editor.lines(), pristine);

    modified_press(&mut session, KeyCode::Char('t'), KeyModifiers::CONTROL);
    session
        .composer
        .editor
        .move_cursor(CursorMove::Jump(subject_line, 0));
    press(&mut session, KeyCode::Char('x'));
    assert_eq!(session.composer.editor.lines(), pristine);
    Ok(())
}

#[test]
fn multiple_schema_values_still_compile_when_present() -> Result<(), Box<dyn Error>> {
    let definitions = vec![repeatable_definition()?];
    let mut session = AuthoringSession::new(&definitions, Some(0));
    set_document(
        &mut session,
        "scope:\n\nsubject:\ncollect evidence\n\nevidence:\nfirst\n\nevidence:\nsecond\nline\n",
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
    assert_eq!(session.composer.editor.lines()[4], "bc");
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
        "scope:\n\nsubject:\nvalid\n\nrequired-field:\ncomplete\n\nmystery:\nvalue\n",
        "scope:\n\nsubject:\nvalid\n\nrequired-field\ncomplete\n",
        "scope:\n\nsubject:\nvalid\n\nsubject:\nduplicate\n\nrequired-field:\ncomplete\n",
        "scope:\n\nsubject:\nvalid\n\nrequired-field:\ncomplete\n\nrequired-field:\nduplicate\n",
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
        "scope:\n\nsubject:\nvalid\n\nrequired-field:\n  note:\n  value\n",
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
        "scope:\n\nsubject:\ncover contracts\n\nrequired-field:\ncomplete\n\nrecommended-field:\n\n\noptional-field:\n\n\nconditional-field:\n\n",
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
        "scope:\n\nsubject:\ntwo\nlines\n\nrequired-field:\ncomplete\n",
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
    assert_eq!(unpinned.composer.editor.lines()[4], "dirty");
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
    assert!(text.contains("gitserious commit"));
    assert!(text.contains("Commit types"));
    assert!(text.contains("Enter select"));
    assert_highlighted_footer(&mut picker, 100, 24)?;

    let definitions = vec![presentation_definition()?];
    let mut composer = AuthoringSession::new(&definitions, Some(0));
    composer
        .composer
        .editor
        .move_cursor(CursorMove::Jump(16, 0));
    let text = rendered(&mut composer, 120, 32)?;
    assert!(text.contains("Compose commit"));
    assert!(text.contains("Keymap: conventional"));
    assert!(text.contains("○ subject · required"));
    assert!(text.contains("conditional-field · conditional"));
    assert!(text.contains("required when the condition applies"));
    assert!(text.contains("ctrl+t vim"));
    assert!(!text.contains("repeatable"));
    assert_highlighted_footer(&mut composer, 120, 32)?;

    modified_press(&mut composer, KeyCode::Char('s'), KeyModifiers::CONTROL);
    let text = rendered(&mut composer, 120, 32)?;
    assert!(text.contains("! subject · required"));
    assert!(text.contains("! required-field · required"));

    let mut review = valid_feat_session();
    let text = rendered(&mut review, 100, 24)?;
    assert!(text.contains("Review commit"));
    assert!(text.contains("feat: compose durable message"));
    assert!(text.contains("Enter commit"));
    assert_highlighted_footer(&mut review, 100, 24)?;

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
    paste(&mut composer, "hello");
    let buffer = rendered_buffer(&mut composer, 120, 32)?;
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
    assert!(footer.contains("ctrl+t vim · ctrl+s review · Esc back"));
    assert!(footer.contains("· col 6/80"));
    assert!(!footer.contains("Ctrl"));
    assert!(!footer.contains("ctrl+n"));
    assert!(!footer.contains("ctrl+d"));

    modified_press(&mut composer, KeyCode::Char('t'), KeyModifiers::CONTROL);
    let buffer = rendered_buffer(&mut composer, 120, 32)?;
    let footer = row_text(&buffer, 120, 31);
    assert!(footer.contains("ctrl+t conventional · ctrl+s review · i insert · q back"));
    press(&mut composer, KeyCode::Char('i'));
    let buffer = rendered_buffer(&mut composer, 120, 32)?;
    let footer = row_text(&buffer, 120, 31);
    assert!(footer.contains("ctrl+t conventional · ctrl+s review · Esc normal"));

    let mut picker = AuthoringSession::new(built_in_commit_types(), None);
    let buffer = rendered_buffer(&mut picker, 100, 24)?;
    let footer = row_text(&buffer, 100, 23);
    assert!(footer.contains("↑/k · ↓/j move · Home/End jump · Enter select · Esc/q cancel"));

    let mut review = valid_feat_session();
    let buffer = rendered_buffer(&mut review, 100, 24)?;
    let footer = row_text(&buffer, 100, 23);
    assert!(footer.contains("Enter commit · Esc edit · ↑/↓ scroll · q/ctrl+c cancel"));
    assert!(!footer.contains("Ctrl"));
    Ok(())
}

#[test]
fn composer_layout_caps_at_eighty_columns_and_adapts_on_narrow_terminals()
-> Result<(), Box<dyn Error>> {
    let mut wide = AuthoringSession::new(built_in_commit_types(), Some(0));
    let buffer = rendered_buffer(&mut wide, 120, 32)?;
    let edit = find_ascii(&buffer, 120, 32, "Edit form").ok_or("missing editor")?;
    let fields = find_ascii(&buffer, 120, 32, "Fields").ok_or("missing fields")?;
    let description = find_ascii(&buffer, 120, 32, "Description").ok_or("missing description")?;
    let guidance = find_ascii(&buffer, 120, 32, "Concise").ok_or("missing guidance")?;

    assert_eq!(edit.1, fields.1);
    assert!(edit.0 < fields.0);
    assert_eq!(fields.0, description.0);
    assert!(description.1 > fields.1);
    assert!(guidance.0 >= description.0.saturating_sub(1));
    assert!(guidance.1 > description.1);
    assert!(find_ascii(&buffer, 120, 32, "col 1/80").is_some());
    assert_eq!(wide.composer.editor.wrap_mode(), WrapMode::WordOrGlyph);

    let mut narrow = AuthoringSession::new(built_in_commit_types(), Some(0));
    let buffer = rendered_buffer(&mut narrow, 72, 24)?;
    assert!(!narrow.too_small);
    assert!(find_ascii(&buffer, 72, 24, "col 1/80").is_some());
    Ok(())
}

#[test]
fn narrow_editor_scrolls_until_fixed_width_wrap_then_returns_to_column_one()
-> Result<(), Box<dyn Error>> {
    let mut session = AuthoringSession::new(built_in_commit_types(), Some(0));
    let initial = rendered_buffer(&mut session, 72, 24)?;
    let subject = find_ascii(&initial, 72, 24, "subject:").ok_or("missing subject")?;
    let first_forty = "0123456789012345678901234567890123456789";
    paste(&mut session, first_forty);
    let buffer = rendered_buffer(&mut session, 72, 24)?;
    let visible = (1..=40)
        .map(|column| buffer[(column, subject.1 + 1)].symbol())
        .collect::<String>();
    assert_eq!(visible.trim_end(), &first_forty[1..]);
    assert!(find_ascii(&buffer, 72, 24, "col 41/80").is_some());

    press(&mut session, KeyCode::Left);
    let buffer = rendered_buffer(&mut session, 72, 24)?;
    assert_eq!(buffer[(1, subject.1 + 1)].symbol(), "0");
    assert!(find_ascii(&buffer, 72, 24, "col 40/80").is_some());
    press(&mut session, KeyCode::Right);

    paste(&mut session, &"x".repeat(41));
    let buffer = rendered_buffer(&mut session, 72, 24)?;
    assert_eq!(
        session.composer.editor.lines()[4],
        format!("{first_forty}{}", "x".repeat(41))
    );
    assert_eq!(buffer[(1, subject.1 + 2)].symbol(), "x");
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
    assert_eq!(word_wrap.composer.editor.lines()[4], value);
    assert!(find_ascii(&buffer, 72, 24, "col 5/80").is_some());

    let mut glyph_wrap = AuthoringSession::new(built_in_commit_types(), Some(0));
    let value = "x".repeat(81);
    paste(&mut glyph_wrap, &value);
    let buffer = rendered_buffer(&mut glyph_wrap, 72, 24)?;
    assert_eq!(glyph_wrap.composer.editor.lines()[4], value);
    assert!(find_ascii(&buffer, 72, 24, "col 2/80").is_some());

    let mut unicode = AuthoringSession::new(built_in_commit_types(), Some(0));
    let value = "🦀".repeat(21);
    paste(&mut unicode, &value);
    let buffer = rendered_buffer(&mut unicode, 72, 24)?;
    assert_eq!(unicode.composer.editor.lines()[4], value);
    assert!(buffer.content().iter().any(|cell| cell.symbol() == "🦀"));
    assert!(find_ascii(&buffer, 72, 24, "col 43/80").is_some());
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

    modified_press(&mut session, KeyCode::Char('t'), KeyModifiers::CONTROL);
    assert_eq!(session.vim_mode, VimMode::Normal);
    assert!(
        session
            .handle_event(Event::Paste("ignored".to_owned()))
            .is_none()
    );
    assert!(!session.composer.dirty());
}
