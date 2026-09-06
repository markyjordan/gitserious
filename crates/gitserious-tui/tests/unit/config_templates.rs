use super::tests::{Workspace, key};
use super::*;
use gitserious_app::{
    ConfigurationCatalog, ConfigurationEdit, CustomConfiguration, ProjectConfig, RepositoryRoot,
    resolve_project_lock,
};
use gitserious_core::{
    Description, TemplateId, TypesetDefinition, TypesetId, TypesetVersion, built_in_configuration,
};
use ratatui::crossterm::event::KeyEvent;
use ratatui::{Terminal, backend::TestBackend};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn control_s() -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))
}

#[test]
fn template_form_filters_typesets_and_preserves_existing_identity() -> TestResult {
    let built = built_in_configuration();
    let mut typesets = built.typesets().to_vec();
    typesets.push(TypesetDefinition::new(
        built.taxonomy().id().clone(),
        TypesetId::new("extra")?,
        TypesetVersion::V1,
        Description::new("Extra requirements.")?,
        built.typeset().schemas().to_vec(),
    )?);
    let mut editor = template_form::TemplateForm::new(built.taxonomies(), typesets, None)?;
    assert!(editor.submit().is_err());
    editor.form.fields[0].set_value("my-template");
    editor.form.fields[1].set_value("My research policy.");
    assert!(editor.form.fields[3].options.contains(&"extra".to_owned()));
    editor.form.focus = 2;
    editor.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(editor.form.fields[2].value(), "ml-research");
    assert_eq!(editor.form.fields[3].options, ["default"]);
    let changes = editor.submit()?;
    let ConfigurationEdit::CreateTemplate(template) = &changes[0] else {
        return Err("wrong edit".into());
    };
    assert_eq!(template.taxonomy().as_str(), "ml-research");
    let mut editor = template_form::TemplateForm::new(
        built.taxonomies(),
        built.typesets().to_vec(),
        Some(template.clone()),
    )?;
    assert!(editor.submit()?.is_empty());
    editor.key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    assert_eq!(editor.form.fields[0].value(), "my-template");
    editor.form.fields[1].set_value("Updated policy.");
    let changes = editor.submit()?;
    let ConfigurationEdit::UpdateTemplate(template) = &changes[0] else {
        return Err("wrong edit".into());
    };
    assert_eq!(template.version().get(), 2);
    Ok(())
}

#[test]
fn browser_forks_a_complete_bundle_and_focuses_its_new_template() -> TestResult {
    let workspace = Workspace::default();
    let mut state = State::new(&workspace);
    state.event(&key(KeyCode::Char('f')), &workspace);
    assert!(matches!(state.editor, Some(editor::Editor::Fork(_))));
    state
        .editor
        .as_mut()
        .ok_or("missing fork form")?
        .form_mut()
        .fields[1]
        .set_value("copy");
    state.event(&control_s(), &workspace);
    let session = state.session.as_ref().ok_or("missing session")?;
    assert_eq!(session.custom().templates()[0].id().as_str(), "copy");
    assert_eq!(
        session.custom().taxonomies()[0].id().as_str(),
        "copy-taxonomy"
    );
    assert_eq!(session.custom().typesets()[0].id().as_str(), "copy-typeset");
    assert_eq!(
        state
            .selected()
            .ok_or("missing selected template")?
            .identity(),
        "copy"
    );
    assert_eq!(workspace.saves.get(), 0);
    state.event(&control_s(), &workspace);
    assert!(matches!(state.screen, Screen::Review(_)));
    state.event(&key(KeyCode::Enter), &workspace);
    assert_eq!(workspace.saves.get(), 1);
    state.event(&key(KeyCode::Char('s')), &workspace);
    assert!(state.status.contains("Only project"));
    Ok(())
}

#[test]
fn project_import_selection_and_active_deletion_use_the_review_gate() -> TestResult {
    let workspace = Workspace::default();
    let mut global = ConfigurationSession::global(CustomConfiguration::default())?;
    global.fork_template(
        &TemplateId::new("ml-research")?,
        &TemplateId::new("copy")?,
        &gitserious_core::TaxonomyId::new("copy-taxonomy")?,
        &TypesetId::new("copy-typeset")?,
    )?;
    *workspace.saved.borrow_mut() = global.custom().clone();
    let global_before = workspace.saved.borrow().clone();
    let config = ProjectConfig::default_channel()?;
    let root = RepositoryRoot::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")))?;
    *workspace.project.borrow_mut() = Some(ConfigurationSession::project(
        root,
        config.clone(),
        resolve_project_lock(&config)?,
    )?);
    let mut state = State::new(&workspace);
    state.event(&key(KeyCode::Tab), &workspace);
    assert_eq!(state.destination, ConfigurationDestination::Project);
    state.event(&key(KeyCode::Char('i')), &workspace);
    let form = state
        .editor
        .as_mut()
        .ok_or("missing import form")?
        .form_mut();
    form.fields[0].set_value("copy");
    form.fields[1].set_value("yes");
    state.event(&control_s(), &workspace);
    assert_eq!(
        state
            .session
            .as_ref()
            .and_then(ConfigurationSession::active_template)
            .map(TemplateId::as_str),
        Some("copy")
    );
    assert_eq!(workspace.saves.get(), 0);
    state.event(&control_s(), &workspace);
    let mut terminal = Terminal::new(TestBackend::new(100, 26))?;
    terminal.draw(|frame| state.render(frame))?;
    let Screen::Review(text) = &state.screen else {
        return Err("review not shown".into());
    };
    assert!(text.contains("Project default: default -> copy"));
    assert!(text.contains("copy-taxonomy"));
    state.event(&key(KeyCode::Enter), &workspace);
    assert_eq!(workspace.saves.get(), 1);
    assert_eq!(*workspace.saved.borrow(), global_before);
    state.event(&key(KeyCode::Char('d')), &workspace);
    state.event(&control_s(), &workspace);
    assert!(state.status.contains("select another"));
    state.selected = entries(
        state.session.as_ref().ok_or("missing session")?.custom(),
        Kind::Template,
    )
    .iter()
    .position(|value| value.identity() == "default")
    .ok_or("default missing")?;
    state.event(&key(KeyCode::Char('s')), &workspace);
    state.event(&control_s(), &workspace);
    state.event(&key(KeyCode::Enter), &workspace);
    assert_eq!(workspace.saves.get(), 2);
    assert_eq!(*workspace.saved.borrow(), global_before);
    Ok(())
}

#[test]
fn import_form_can_import_without_selecting() -> TestResult {
    let built = built_in_configuration();
    let mut global = ConfigurationSession::global(CustomConfiguration::default())?;
    global.fork_template(
        &TemplateId::new("default")?,
        &TemplateId::new("copy")?,
        &gitserious_core::TaxonomyId::new("copy-taxonomy")?,
        &TypesetId::new("copy-typeset")?,
    )?;
    let source = ConfigurationCatalog::new(global.custom())?;
    let mut form = template_form::ImportForm::new(source)?;
    form.form.fields[0].set_value("copy");
    let config = ProjectConfig::default_channel()?;
    let root = RepositoryRoot::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")))?;
    let mut project =
        ConfigurationSession::project(root, config.clone(), resolve_project_lock(&config)?)?;
    form.stage(&mut project)?;
    assert_eq!(project.active_template(), Some(built.template().id()));
    project.validate()?;
    assert!(
        matches!(project.custom().templates().first(), Some(template) if template.id().as_str() == "copy")
    );
    Ok(())
}

#[test]
fn an_undersized_review_cannot_apply_hidden_changes() -> TestResult {
    let workspace = Workspace::default();
    let mut state = State::new(&workspace);
    state.event(&key(KeyCode::Char('f')), &workspace);
    state
        .editor
        .as_mut()
        .ok_or("missing fork form")?
        .form_mut()
        .fields[1]
        .set_value("copy");
    state.event(&control_s(), &workspace);
    state.event(&control_s(), &workspace);
    let mut small = Terminal::new(TestBackend::new(50, 10))?;
    small.draw(|frame| state.render(frame))?;
    state.event(&key(KeyCode::Enter), &workspace);
    assert_eq!(workspace.saves.get(), 0);
    assert!(
        state
            .session
            .as_ref()
            .is_some_and(ConfigurationSession::is_dirty)
    );
    let mut usable = Terminal::new(TestBackend::new(100, 24))?;
    usable.draw(|frame| state.render(frame))?;
    state.event(&key(KeyCode::Enter), &workspace);
    assert_eq!(workspace.saves.get(), 1);
    Ok(())
}
