use super::*;
use gitserious_app::{
    ConfigurationDestination, ConfigurationEditor, ConfigurationSession, ConfigurationWorkspace,
    GlobalConfigurationStore,
};

#[test]
fn related_schema_edits_can_be_staged_before_complete_validation() -> Result<(), Box<dyn Error>> {
    let mut session = ConfigurationSession::global(source_configuration()?)?;
    let old = session.custom().taxonomies()[0].clone();
    let mut types = old.change_types().to_vec();
    types.push(gitserious_core::ChangeTypeDefinition::new(
        gitserious_core::ChangeTypeId::new("extra")?,
        Description::new("Extra category.")?,
    ));
    session.stage([ConfigurationEdit::UpdateTaxonomy(TaxonomyDefinition::new(
        old.id().clone(),
        TaxonomyVersion::new(2)?,
        old.description().clone(),
        types,
    )?)])?;
    assert!(session.validate().is_err());
    let old = session.custom().typesets()[0].clone();
    let mut schemas = old.schemas().to_vec();
    schemas.push(gitserious_core::ChangeTypeSchema::new(
        gitserious_core::ChangeTypeId::new("extra")?,
        vec![],
    )?);
    session.stage([ConfigurationEdit::UpdateTypeset(
        gitserious_core::TypesetDefinition::new(
            old.taxonomy().clone(),
            old.id().clone(),
            gitserious_core::TypesetVersion::new(2)?,
            old.description().clone(),
            schemas,
        )?,
    )])?;
    session.validate()?;
    assert!(session.is_dirty());
    let before = session.custom().clone();
    assert!(
        session
            .stage([ConfigurationEdit::DeleteTemplate(TemplateId::new(
                "default"
            )?)])
            .is_err()
    );
    assert_eq!(session.custom(), &before);
    Ok(())
}

struct GlobalStore(RefCell<CustomConfiguration>);

#[test]
fn imports_are_atomic_idempotent_and_independent_of_global_changes() -> Result<(), Box<dyn Error>> {
    let config = ProjectConfig::default_channel()?;
    let mut project = ConfigurationSession::project(
        repository_root()?,
        config.clone(),
        resolve_project_lock(&config)?,
    )?;
    let mut global = ConfigurationSession::global(source_configuration()?)?;
    let source = ConfigurationCatalog::new(global.custom())?;
    let platform = TemplateId::new("platform")?;
    project.import_template(&source, &platform, false)?;
    project.validate()?;
    assert_eq!(
        project.active_template().map(TemplateId::as_str),
        Some("default")
    );
    let before = project.custom().clone();
    project.import_template(&source, &platform, true)?;
    assert_eq!(project.custom(), &before);
    assert_eq!(project.active_template(), Some(&platform));
    assert_eq!(
        project.original_active_template().map(TemplateId::as_str),
        Some("default")
    );
    assert!(global.import_template(&source, &platform, false).is_err());
    assert!(global.select_template(platform.clone()).is_err());
    global.stage([ConfigurationEdit::DeleteTemplate(platform.clone())])?;
    assert_eq!(project.custom(), &before);
    project.stage([ConfigurationEdit::DeleteTemplate(platform)])?;
    assert!(
        project
            .validate()
            .err()
            .ok_or("deleted active template was accepted")?
            .contains("select another")
    );
    project.select_template(TemplateId::new("default")?)?;
    project.validate()?;
    Ok(())
}

#[test]
fn conflicting_import_preserves_definitions_and_selection() -> Result<(), Box<dyn Error>> {
    let source = source_catalog()?;
    let custom = source_configuration()?;
    let old = custom.taxonomies()[0].clone();
    let changed = apply_custom_configuration_edits::<FakeError>(
        &custom,
        [ConfigurationEdit::UpdateTaxonomy(TaxonomyDefinition::new(
            old.id().clone(),
            TaxonomyVersion::new(2)?,
            Description::new("Local meaning must survive.")?,
            old.change_types().to_vec(),
        )?)],
    )?;
    let config = ProjectConfig::new(1, TemplateId::new("default")?, changed.clone())?;
    let mut session = ConfigurationSession::project(
        repository_root()?,
        config.clone(),
        resolve_project_lock(&config)?,
    )?;
    assert!(
        session
            .import_template(&source, &TemplateId::new("platform")?, true)
            .is_err()
    );
    assert_eq!(session.custom(), &changed);
    assert_eq!(
        session.active_template().map(TemplateId::as_str),
        Some("default")
    );
    assert!(!session.is_dirty());
    Ok(())
}

#[test]
fn draft_forks_keep_sources_immutable_and_reject_target_collisions() -> Result<(), Box<dyn Error>> {
    let mut session = ConfigurationSession::global(CustomConfiguration::default())?;
    session.fork_template(
        &TemplateId::new("ml-research")?,
        &TemplateId::new("research-copy")?,
        &gitserious_core::TaxonomyId::new("research-copy")?,
        &TypesetId::new("context")?,
    )?;
    session.validate()?;
    let before = session.custom().clone();
    let catalog = ConfigurationCatalog::new(&before)?;
    assert_eq!(
        catalog
            .resolve(&TemplateId::new("ml-research")?)?
            .change_types(),
        catalog
            .resolve(&TemplateId::new("research-copy")?)?
            .change_types()
    );
    assert!(
        session
            .fork_template(
                &TemplateId::new("infra-ops")?,
                &TemplateId::new("research-copy")?,
                &gitserious_core::TaxonomyId::new("other")?,
                &TypesetId::new("context")?
            )
            .is_err()
    );
    assert_eq!(session.custom(), &before);
    Ok(())
}

#[test]
fn custom_template_identity_can_match_the_conventional_taxonomy_name() -> Result<(), Box<dyn Error>>
{
    let config = ProjectConfig::default_channel()?;
    let mut session = ConfigurationSession::project(
        repository_root()?,
        config.clone(),
        resolve_project_lock(&config)?,
    )?;
    let template = gitserious_core::TemplateDefinition::new(
        TemplateId::new("conventional")?,
        gitserious_core::TemplateVersion::V1,
        Description::new("A custom template with a valid independent identity.")?,
        gitserious_core::TaxonomyId::new("ml-research")?,
        TypesetId::new("default")?,
    );
    session.stage([ConfigurationEdit::CreateTemplate(template)])?;
    session.validate()?;
    session.select_template(TemplateId::new("conventional")?)?;
    session.validate()?;
    Ok(())
}

#[test]
fn recreating_an_original_identity_cannot_reset_its_version() -> Result<(), Box<dyn Error>> {
    let mut session = ConfigurationSession::global(source_configuration()?)?;
    let original = session.original().clone();
    let taxonomy = original.taxonomies()[0].clone();
    assert!(
        session
            .stage([
                ConfigurationEdit::DeleteTaxonomy(taxonomy.id().clone()),
                ConfigurationEdit::CreateTaxonomy(taxonomy),
            ])
            .is_err()
    );
    assert_eq!(session.custom(), &original);
    assert!(!session.is_dirty());
    Ok(())
}
impl GlobalConfigurationStore for GlobalStore {
    type Error = FakeError;
    fn load(&self) -> Result<CustomConfiguration, Self::Error> {
        Ok(self.0.borrow().clone())
    }
    fn compare_and_swap(
        &self,
        expected: &CustomConfiguration,
        replacement: &CustomConfiguration,
    ) -> Result<(), Self::Error> {
        if *self.0.borrow() != *expected {
            return Err(FakeError("concurrent global change"));
        }
        *self.0.borrow_mut() = replacement.clone();
        Ok(())
    }
}

struct Editor<F>(F);
impl<F> ConfigurationEditor for Editor<F>
where
    F: Fn(&dyn ConfigurationWorkspace) -> Result<(), String>,
{
    fn edit(&self, workspace: &dyn ConfigurationWorkspace) -> Result<(), String> {
        self.0(workspace)
    }
}

#[test]
fn reviewed_sessions_save_one_snapshot_and_reject_concurrent_changes() -> Result<(), Box<dyn Error>>
{
    let initial = initialized(CustomConfiguration::default(), "default")?;
    let project = FakeStore::new(initial.clone());
    let global = GlobalStore(RefCell::new(CustomConfiguration::default()));
    let edits = fork_conventional_edits(
        &TemplateId::new("copy")?,
        &gitserious_core::TaxonomyId::new("copy-taxonomy")?,
        &TypesetId::new("copy-typeset")?,
    );
    let editor = Editor(|workspace: &dyn ConfigurationWorkspace| {
        let mut draft = workspace.load(ConfigurationDestination::Project)?;
        draft.stage(edits.clone())?;
        draft.validate()?;
        assert_eq!(*project.state.borrow(), initial);
        let clean = workspace.save(&draft)?;
        assert!(!clean.is_dirty());
        assert_eq!(project.replacements.get(), 1);
        assert_eq!(*global.0.borrow(), CustomConfiguration::default());
        let mut global_draft = workspace.load(ConfigurationDestination::Global)?;
        global_draft.stage(edits.clone())?;
        *global.0.borrow_mut() = clean.custom().clone();
        assert!(workspace.save(&global_draft).is_err());
        assert!(global_draft.is_dirty());
        Ok(())
    });
    gitserious_app::edit_configuration(&FakeLocator, &project, &global, Path::new("."), &editor)?;
    Ok(())
}
