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
