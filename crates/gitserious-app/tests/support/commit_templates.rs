use super::*;
use gitserious_app::{
    AuthoredCommit, CommitAuthoringContext, CommitAuthoringOutcome, ConfigurationCatalog,
    create_commit_with_template,
};

#[derive(Clone, Copy, Default)]
enum ReviewMode {
    #[default]
    Canonical,
    Missing,
    Plain,
    WrongFingerprint,
}

#[derive(Default)]
struct ContextAuthor<'a> {
    choose: Option<TemplateId>,
    forged: Option<TemplateId>,
    initial_schema: bool,
    review_mode: ReviewMode,
    change_policy: Option<&'a FakeStore>,
    reviewed: RefCell<Vec<String>>,
    seen: RefCell<Vec<CommitAuthoringContext>>,
}

#[test]
fn missing_or_mismatched_reviewed_bytes_never_reach_git() -> Result<(), Box<dyn Error>> {
    for review_mode in [
        ReviewMode::Missing,
        ReviewMode::Plain,
        ReviewMode::WrongFingerprint,
    ] {
        let harness = Harness::new(initialized_state()?);
        let author = ContextAuthor {
            review_mode,
            ..Default::default()
        };
        let error = create_commit_with_template(
            &harness.locator,
            &harness.store,
            &author,
            &harness.writer,
            &repository_path(),
            None,
            None,
        )
        .err()
        .ok_or("unreviewed message accepted")?;
        if matches!(review_mode, ReviewMode::Missing) {
            assert!(matches!(error, CreateCommitError::MissingReviewedMessage));
        } else {
            assert!(matches!(error, CreateCommitError::ReviewedMessageMismatch));
        }
        assert!(harness.writer.messages.borrow().is_empty());
    }
    Ok(())
}

#[test]
fn approved_message_uses_the_captured_policy_even_if_the_project_changes()
-> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    let author = ContextAuthor {
        change_policy: Some(&harness.store),
        ..Default::default()
    };
    create_commit_with_template(
        &harness.locator,
        &harness.store,
        &author,
        &harness.writer,
        &repository_path(),
        None,
        None,
    )?;
    assert_eq!(*harness.writer.messages.borrow(), *author.reviewed.borrow());
    assert!(harness.writer.messages.borrow()[0].contains("Gitserious-Template: default@1\n"));
    assert!(
        matches!(&*harness.store.state.borrow(), Ok(ProjectState::Initialized { config, .. }) if config.active_template().as_str() == "ml-research")
    );
    assert_eq!(
        harness
            .trace
            .borrow()
            .iter()
            .filter(|call| **call == "inspect")
            .count(),
        1
    );
    Ok(())
}

struct LegacyAuthor;
impl CommitDraftAuthor for LegacyAuthor {
    type Error = FakeError;
    fn author(
        &self,
        definitions: &[CommitTypeDefinition],
        _: Option<&CommitTypeDefinition>,
    ) -> Result<CommitDraftAuthorOutcome, Self::Error> {
        valid_draft(&definitions[0])
            .map(CommitDraftAuthorOutcome::Authored)
            .map_err(|_| FakeError("invalid legacy draft"))
    }
}

#[test]
fn legacy_draft_only_adapter_cannot_write_unreviewed_provenance() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    assert!(matches!(
        create_commit(
            &harness.locator,
            &harness.store,
            &LegacyAuthor,
            &harness.writer,
            &repository_path(),
            None
        ),
        Err(CreateCommitError::MissingReviewedMessage)
    ));
    assert!(harness.writer.messages.borrow().is_empty());
    Ok(())
}

impl CommitDraftAuthor for ContextAuthor<'_> {
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
        let message = match self.review_mode {
            ReviewMode::Missing => None,
            ReviewMode::Plain => gitserious_core::render_commit_message(definition, &draft).ok(),
            ReviewMode::WrongFingerprint => gitserious_core::render_commit_message_with_provenance(
                &gitserious_core::CommitProvenance::new(
                    template.schema().clone(),
                    gitserious_core::Fingerprint::from_bytes([0; 32]),
                ),
                &draft,
            )
            .ok(),
            ReviewMode::Canonical => template.render(&draft).ok(),
        };
        let id = self.forged.clone().unwrap_or_else(|| template.id().clone());
        let authored = match message {
            Some(message) => {
                self.reviewed.borrow_mut().push(message.as_str().to_owned());
                AuthoredCommit::reviewed(id, draft, message)
            }
            None => AuthoredCommit::new(id, draft),
        };
        if let Some(store) = self.change_policy {
            let config = ProjectConfig::new(
                1,
                TemplateId::new("ml-research").map_err(|_| FakeError("invalid id"))?,
                CustomConfiguration::default(),
            )
            .map_err(|_| FakeError("invalid policy"))?;
            let lock = resolve_project_lock(&config).map_err(|_| FakeError("invalid lock"))?;
            *store.state.borrow_mut() = Ok(ProjectState::Initialized { config, lock });
        }
        Ok(CommitAuthoringOutcome::Authored(authored))
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
