use super::*;
use gitserious_app::{
    AuthoredCommit, CommitAuthoringContext, CommitAuthoringOutcome, ConfigurationCatalog,
    create_commit_with_template,
};

#[derive(Default)]
struct ContextAuthor {
    choose: Option<TemplateId>,
    forged: Option<TemplateId>,
    initial_schema: bool,
    seen: RefCell<Vec<CommitAuthoringContext>>,
}

impl CommitDraftAuthor for ContextAuthor {
    type Error = FakeError;
    fn author(
        &self,
        _: &[CommitTypeDefinition],
        _: Option<&CommitTypeDefinition>,
    ) -> Result<CommitDraftAuthorOutcome, Self::Error> {
        Err(FakeError("unexpected flat authoring"))
    }
    fn author_with_context(
        &self,
        context: &CommitAuthoringContext,
    ) -> Result<CommitAuthoringOutcome, Self::Error> {
        self.seen.borrow_mut().push(context.clone());
        let selected = self
            .choose
            .as_ref()
            .unwrap_or(context.initial_template().id());
        let template = context
            .find_template(selected)
            .ok_or(FakeError("missing template"))?;
        let schema = if self.initial_schema {
            context.initial_template()
        } else {
            template
        };
        let definition = schema
            .definitions()
            .iter()
            .find(|definition| definition.id().as_str() == "fix")
            .ok_or(FakeError("missing fix"))?;
        let draft = valid_draft(definition).map_err(|_| FakeError("invalid draft"))?;
        Ok(CommitAuthoringOutcome::Authored(AuthoredCommit::new(
            self.forged.clone().unwrap_or_else(|| template.id().clone()),
            draft,
        )))
    }
}

#[test]
fn explicit_and_authored_template_choices_use_their_own_schema_without_policy_writes()
-> Result<(), Box<dyn Error>> {
    for explicit in [true, false] {
        let initial = initialized_state()?;
        let harness = Harness::new(initial.clone());
        let ml = TemplateId::new("ml-research")?;
        let author = ContextAuthor {
            choose: Some(ml.clone()),
            ..Default::default()
        };
        create_commit_with_template(
            &harness.locator,
            &harness.store,
            &author,
            &harness.writer,
            &repository_path(),
            explicit.then_some(&ml),
            None,
        )?;
        let seen = author.seen.borrow();
        assert_eq!(
            seen[0].initial_template().id().as_str(),
            if explicit { "ml-research" } else { "default" }
        );
        assert_eq!(seen[0].templates().len(), 3);
        let messages = harness.writer.messages.borrow();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("symptom:\n"));
        assert_eq!(*harness.store.state.borrow(), Ok(initial));
    }
    Ok(())
}

#[test]
fn template_mismatch_unknown_template_and_cross_schema_drafts_never_reach_git()
-> Result<(), Box<dyn Error>> {
    let fix = CommitTypeId::new("fix")?;
    let harness = Harness::new(initialized_state()?);
    let author = ContextAuthor {
        choose: Some(TemplateId::new("ml-research")?),
        ..Default::default()
    };
    assert!(matches!(
        create_commit_with_template(
            &harness.locator,
            &harness.store,
            &author,
            &harness.writer,
            &repository_path(),
            None,
            Some(&fix)
        ),
        Err(CreateCommitError::AuthoredTemplateMismatch { .. })
    ));
    assert!(harness.writer.messages.borrow().is_empty());
    let author = ContextAuthor {
        forged: Some(TemplateId::new("global-only")?),
        ..Default::default()
    };
    assert!(matches!(
        create_commit_with_template(
            &harness.locator,
            &harness.store,
            &author,
            &harness.writer,
            &repository_path(),
            None,
            None
        ),
        Err(CreateCommitError::UnknownTemplate { .. })
    ));
    let author = ContextAuthor {
        choose: Some(TemplateId::new("ml-research")?),
        initial_schema: true,
        ..Default::default()
    };
    assert!(matches!(
        create_commit_with_template(
            &harness.locator,
            &harness.store,
            &author,
            &harness.writer,
            &repository_path(),
            None,
            None
        ),
        Err(CreateCommitError::InvalidDraft(_))
    ));
    assert!(harness.writer.messages.borrow().is_empty());
    Ok(())
}

#[test]
fn project_custom_templates_are_available_but_unknown_requests_fail_before_authoring()
-> Result<(), Box<dyn Error>> {
    let template = TemplateDefinition::new(
        TemplateId::new("research-policy")?,
        TemplateVersion::V1,
        Description::new("Project research policy.")?,
        TaxonomyId::new("ml-research")?,
        TypesetId::new("default")?,
    );
    let config = ProjectConfig::new(
        1,
        TemplateId::new("default")?,
        CustomConfiguration::new(vec![], vec![], vec![template])?,
    )?;
    let lock = resolve_project_lock(&config)?;
    let harness = Harness::new(ProjectState::Initialized { config, lock });
    let author = ContextAuthor::default();
    let requested = TemplateId::new("research-policy")?;
    create_commit_with_template(
        &harness.locator,
        &harness.store,
        &author,
        &harness.writer,
        &repository_path(),
        Some(&requested),
        None,
    )?;
    assert_eq!(author.seen.borrow()[0].initial_template().id(), &requested);
    assert_eq!(author.seen.borrow()[0].templates().len(), 4);
    let missing = TemplateId::new("global-only")?;
    assert!(matches!(
        create_commit_with_template(
            &harness.locator,
            &harness.store,
            &author,
            &harness.writer,
            &repository_path(),
            Some(&missing),
            None
        ),
        Err(CreateCommitError::UnknownTemplate { .. })
    ));
    assert_eq!(author.seen.borrow().len(), 1);
    Ok(())
}

#[test]
fn authoring_context_rejects_ambiguous_or_missing_initial_selections() -> Result<(), Box<dyn Error>>
{
    let catalog = ConfigurationCatalog::new(&CustomConfiguration::default())?;
    let id = TemplateId::new("default")?;
    let schema = catalog.resolve(&id)?;
    assert!(CommitAuthoringContext::new(vec![], &id, None).is_err());
    assert!(CommitAuthoringContext::new(vec![schema.clone(), schema.clone()], &id, None).is_err());
    assert!(
        CommitAuthoringContext::new(vec![schema.clone()], &TemplateId::new("absent")?, None)
            .is_err()
    );
    assert!(
        CommitAuthoringContext::new(vec![schema], &id, Some(&CommitTypeId::new("absent")?))
            .is_err()
    );
    Ok(())
}
