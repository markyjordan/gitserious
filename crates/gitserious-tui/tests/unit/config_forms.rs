use super::taxonomy_form::TaxonomyForm;
use super::tests::{Workspace, key};
use super::*;
use gitserious_app::ConfigurationEdit;
use ratatui::crossterm::event::KeyEvent;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn filled_typeset() -> Result<super::typeset_form::TypesetForm, Box<dyn std::error::Error>> {
    let changes = filled_taxonomy().submit()?;
    let ConfigurationEdit::CreateTaxonomy(taxonomy) = &changes[0] else {
        return Err("wrong edit".into());
    };
    let mut editor = super::typeset_form::TypesetForm::new(vec![taxonomy.clone()], None)?;
    editor.form.fields[0].set_value("context");
    editor.form.fields[2].set_value("Context worth preserving.");
    editor.form.focus = 3;
    Ok(editor)
}

#[test]
fn typeset_form_covers_empty_schemas_and_validates_conditional_guidance() -> TestResult {
    use gitserious_core::{PropertyMultiplicity, PropertyRequirement};
    let mut editor = filled_typeset()?;
    let changes = editor.submit()?;
    let ConfigurationEdit::CreateTypeset(empty) = &changes[0] else {
        return Err("wrong edit".into());
    };
    assert_eq!(empty.schemas().len(), 1);
    assert!(empty.schemas()[0].properties().is_empty());
    editor.key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL))?;
    editor.form.fields[4].set_value("scope");
    editor.form.fields[5].set_value("Non-obvious bounds.\nIncluding external requirements.");
    editor.form.fields[7].set_value("multiple");
    editor.form.focus = 6;
    for _ in 0..3 {
        editor.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))?;
    }
    assert_eq!(editor.form.fields[6].value(), "conditional");
    assert!(!editor.form.fields[8].readonly);
    assert!(editor.submit().is_err());
    editor.form.fields[8].set_value("external-bounds");
    assert!(editor.submit().is_err());
    editor.form.fields[9].set_value("Required when external constraints apply.");
    let changes = editor.submit()?;
    let ConfigurationEdit::CreateTypeset(value) = &changes[0] else {
        return Err("wrong edit".into());
    };
    let property = &value.schemas()[0].properties()[0];
    assert_eq!(property.key().as_str(), "scope");
    assert_eq!(property.multiplicity(), PropertyMultiplicity::Multiple);
    let PropertyRequirement::Conditional(condition) = property.requirement() else {
        return Err("missing condition".into());
    };
    assert_eq!(condition.id().as_str(), "external-bounds");
    editor.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))?;
    assert!(editor.form.fields[8].readonly);
    let changes = editor.submit()?;
    let ConfigurationEdit::CreateTypeset(value) = &changes[0] else {
        return Err("wrong edit".into());
    };
    assert_eq!(
        value.schemas()[0].properties()[0].requirement(),
        &PropertyRequirement::Required
    );
    Ok(())
}

#[test]
fn typeset_properties_can_be_ordered_removed_and_repaired_without_losing_values() -> TestResult {
    let mut editor = filled_typeset()?;
    editor.key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL))?;
    editor.form.fields[4].set_value("description");
    editor.form.fields[5].set_value("First property meaning.");
    editor.key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL))?;
    editor.form.fields[10].set_value("description");
    editor.form.fields[11].set_value("Second property meaning.");
    assert!(editor.submit().is_err());
    editor.form.fields[10].set_value("intent");
    editor.key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT))?;
    let changes = editor.submit()?;
    let ConfigurationEdit::CreateTypeset(value) = &changes[0] else {
        return Err("wrong edit".into());
    };
    assert_eq!(
        value.schemas()[0]
            .properties()
            .iter()
            .map(|value| value.key().as_str())
            .collect::<Vec<_>>(),
        ["intent", "description"]
    );
    editor.key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL))?;
    editor.form.focus = 4;
    editor.key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL))?;
    let changes = editor.submit()?;
    let ConfigurationEdit::CreateTypeset(value) = &changes[0] else {
        return Err("wrong edit".into());
    };
    assert!(value.schemas()[0].properties().is_empty());
    Ok(())
}

#[test]
fn changing_taxonomy_retains_each_unsaved_property_draft() -> TestResult {
    let taxonomies = gitserious_core::built_in_configuration().taxonomies()[..2].to_vec();
    let mut editor = super::typeset_form::TypesetForm::new(taxonomies, None)?;
    editor.form.fields[0].set_value("context");
    editor.form.fields[2].set_value("Durable context.");
    editor.form.focus = 3;
    editor.key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL))?;
    editor.form.fields[4].set_value("intent");
    editor.form.fields[5].set_value("Why this change exists.");
    editor.form.focus = 1;
    editor.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))?;
    assert_eq!(editor.form.fields[1].value(), "ml-research");
    editor.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))?;
    assert_eq!(editor.form.fields[1].value(), "conventional");
    assert_eq!(editor.form.fields[4].value(), "intent");
    let changes = editor.submit()?;
    let ConfigurationEdit::CreateTypeset(value) = &changes[0] else {
        return Err("wrong edit".into());
    };
    assert_eq!(
        value.schemas()[0].properties()[0].description(),
        "Why this change exists."
    );
    Ok(())
}

#[test]
fn edited_typesets_reconcile_staged_taxonomy_coverage_and_advance_version() -> TestResult {
    use gitserious_core::{
        ChangeTypeDefinition, ChangeTypeId, Description, TaxonomyDefinition, TaxonomyVersion,
    };
    let changes = filled_typeset()?.submit()?;
    let ConfigurationEdit::CreateTypeset(typeset) = &changes[0] else {
        return Err("wrong edit".into());
    };
    let changes = filled_taxonomy().submit()?;
    let ConfigurationEdit::CreateTaxonomy(taxonomy) = &changes[0] else {
        return Err("wrong edit".into());
    };
    let unchanged =
        super::typeset_form::TypesetForm::new(vec![taxonomy.clone()], Some(typeset.clone()))?;
    assert!(unchanged.submit()?.is_empty());
    let mut types = taxonomy.change_types().to_vec();
    types.push(ChangeTypeDefinition::new(
        ChangeTypeId::new("finding")?,
        Description::new("A finding.")?,
    ));
    let taxonomy = TaxonomyDefinition::new(
        taxonomy.id().clone(),
        TaxonomyVersion::new(2)?,
        taxonomy.description().clone(),
        types,
    )?;
    let mut editor = super::typeset_form::TypesetForm::new(vec![taxonomy], Some(typeset.clone()))?;
    editor.form.focus = 0;
    editor.key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))?;
    assert_eq!(editor.form.fields[0].value(), "context");
    let changes = editor.submit()?;
    let ConfigurationEdit::UpdateTypeset(value) = &changes[0] else {
        return Err("wrong edit".into());
    };
    assert_eq!(value.version().get(), 2);
    assert_eq!(value.schemas().len(), 2);
    assert_eq!(value.schemas()[1].change_type().as_str(), "finding");
    assert!(value.schemas()[1].properties().is_empty());
    Ok(())
}

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
    state.editor = Some(editor::Editor::Taxonomy(filled_taxonomy()));
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
    state.editor = Some(editor::Editor::Taxonomy(filled_taxonomy()));
    state.event(&key(KeyCode::Esc), &workspace);
    assert!(
        state
            .editor
            .as_ref()
            .ok_or("missing form")?
            .form()
            .confirming_discard()
    );
    state.event(&key(KeyCode::Char('y')), &workspace);
    assert!(matches!(state.screen, Screen::Browse));
    assert_eq!(workspace.saves.get(), 0);
    Ok(())
}
