use super::taxonomy_form::TaxonomyForm;
use super::tests::{Workspace, key};
use super::*;
use gitserious_app::ConfigurationEdit;
use ratatui::crossterm::event::KeyEvent;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn filled_taxonomy() -> TaxonomyForm {
    let mut editor = TaxonomyForm::new(None);
    for (index, value) in [
        "research",
        "Research decisions.",
        "trial",
        "A controlled trial.",
    ]
    .into_iter()
    .enumerate()
    {
        editor.form.fields[index].set_value(value);
    }
    editor
}

#[test]
fn taxonomy_form_adds_orders_and_removes_types_without_writing() -> TestResult {
    let mut editor = filled_taxonomy();
    editor.key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL))?;
    editor.form.fields[4].set_value("observation");
    editor.form.fields[5].set_value("An observed result.");
    editor.key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT))?;
    let changes = editor.submit()?;
    let ConfigurationEdit::CreateTaxonomy(value) = &changes[0] else {
        return Err("wrong edit".into());
    };
    assert_eq!(
        value
            .change_types()
            .iter()
            .map(|value| value.id().as_str())
            .collect::<Vec<_>>(),
        ["observation", "trial"]
    );
    editor.key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL))?;
    let changes = editor.submit()?;
    let ConfigurationEdit::CreateTaxonomy(value) = &changes[0] else {
        return Err("wrong edit".into());
    };
    assert_eq!(value.change_types().len(), 1);
    assert_eq!(value.change_types()[0].id().as_str(), "trial");
    assert!(
        editor
            .key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL))
            .is_err()
    );
    Ok(())
}

#[test]
fn taxonomy_editor_validates_ids_and_versions_and_preserves_multiline_values() -> TestResult {
    assert!(TaxonomyForm::new(None).submit().is_err());
    let mut editor = filled_taxonomy();
    editor.form.focus = 0;
    assert!(editor.form.paste("bad\nname").is_err());
    assert_eq!(editor.form.fields[0].value(), "research");
    editor.form.fields[1].set_value("Research 🦀\nPreserve leading assumptions.");
    let changes = editor.submit()?;
    let ConfigurationEdit::CreateTaxonomy(value) = &changes[0] else {
        return Err("wrong edit".into());
    };
    let mut edited = TaxonomyForm::new(Some(value.clone()));
    assert!(edited.submit()?.is_empty());
    edited.key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))?;
    assert_eq!(edited.form.fields[0].value(), "research");
    edited.form.fields[1].set_value("An updated meaning.");
    let changes = edited.submit()?;
    let ConfigurationEdit::UpdateTaxonomy(value) = &changes[0] else {
        return Err("wrong edit".into());
    };
    assert_eq!(value.version().get(), 2);
    assert_eq!(value.id().as_str(), "research");
    Ok(())
}

#[test]
fn form_submission_stages_but_does_not_save_and_builtins_are_readonly() -> TestResult {
    let workspace = Workspace::default();
    let mut state = State::new(&workspace);
    state.event(&key(KeyCode::Char('e')), &workspace);
    assert!(state.status.contains("read-only"));
    state.event(&key(KeyCode::Char('n')), &workspace);
    state.editor = Some(filled_taxonomy());
    state.event(
        &Event::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)),
        &workspace,
    );
    assert!(matches!(state.screen, Screen::Browse));
    assert_eq!(
        state
            .session
            .as_ref()
            .ok_or("missing session")?
            .custom()
            .taxonomies()
            .len(),
        1
    );
    assert_eq!(workspace.saves.get(), 0);
    state.event(&key(KeyCode::Char('n')), &workspace);
    state.editor = Some(filled_taxonomy());
    state.event(&key(KeyCode::Esc), &workspace);
    assert!(
        state
            .editor
            .as_ref()
            .ok_or("missing form")?
            .form
            .confirming_discard()
    );
    state.event(&key(KeyCode::Char('y')), &workspace);
    assert!(matches!(state.screen, Screen::Browse));
    assert_eq!(workspace.saves.get(), 0);
    Ok(())
}
