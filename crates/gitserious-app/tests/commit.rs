use std::cell::RefCell;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gitserious_app::{
    AuthoredCommit, CommitAuthoringContext, CommitAuthoringOutcome, CommitDraftAuthor,
    CommitDraftAuthorOutcome, CommitOutcome, CommitOutput, CommitTaxonomy, CommitWriter,
    CreateCommitError, GlobalConfiguration, ProjectConfiguration, ProjectConfigurationStore,
    ProjectRevision, RepositoryLocator, RepositoryRoot, StoredProject, create_commit_with_taxonomy,
    fingerprint_taxonomy, initial_project_configuration,
};
use gitserious_core::{
    AuthoredProperty, CommitDraft, CommitMessage, CommitProvenance, CommitSubject,
    CommitTypeDefinition, CommitTypeId, ConditionalApplicability, Description, Fingerprint,
    PropertyDefinition, PropertyKey, PropertyRequirement, PropertyResponse, PropertyValue,
    SchemaVersion, Taxonomy, TaxonomyId, TaxonomyVersion, built_in_commit_types,
    built_in_taxonomies, validate_commit_draft, validate_commit_draft_with_provenance,
};

type Trace = Rc<RefCell<Vec<&'static str>>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FakeError(&'static str);

impl Display for FakeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for FakeError {}

struct FakeLocator {
    fail: bool,
    trace: Trace,
}

impl RepositoryLocator for FakeLocator {
    type Error = FakeError;

    fn is_not_repository(_: &Self::Error) -> bool {
        false
    }

    fn locate(&self, _start: &Path) -> Result<RepositoryRoot, Self::Error> {
        self.trace.borrow_mut().push("locate");
        if self.fail {
            Err(FakeError("locator failed"))
        } else {
            RepositoryRoot::new(repository_path()).map_err(|_| FakeError("invalid root"))
        }
    }
}

struct FakeStore {
    state: RefCell<Result<Option<ProjectConfiguration>, FakeError>>,
    trace: Trace,
}

impl ProjectConfigurationStore for FakeStore {
    type Error = FakeError;

    fn load(&self, _root: &RepositoryRoot) -> Result<Option<StoredProject>, Self::Error> {
        self.trace.borrow_mut().push("load");
        self.state.borrow().clone().map(|state| {
            state.map(|configuration| StoredProject {
                configuration,
                revision: ProjectRevision::of_bytes(b"fake"),
            })
        })
    }

    fn create(
        &self,
        _root: &RepositoryRoot,
        _configuration: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error> {
        Err(FakeError("unexpected create"))
    }

    fn replace(
        &self,
        _root: &RepositoryRoot,
        _expected: ProjectRevision,
        _configuration: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error> {
        Err(FakeError("unexpected replace"))
    }

    fn ensure_local_state(&self, _root: &RepositoryRoot) -> Result<(), Self::Error> {
        Err(FakeError("unexpected ensure local state"))
    }
}

struct FakeAuthor {
    result: RefCell<FakeAuthorResult>,
    seen: RefCell<Vec<(Vec<CommitTypeId>, Option<CommitTypeId>)>>,
    trace: Trace,
}

enum FakeAuthorResult {
    Valid(usize),
    Outcome(CommitDraftAuthorOutcome),
    Error(FakeError),
}

impl CommitDraftAuthor for FakeAuthor {
    type Error = FakeError;

    fn author_with_context(
        &self,
        context: &CommitAuthoringContext,
    ) -> Result<CommitAuthoringOutcome, Self::Error> {
        let taxonomy = context.initial_taxonomy();
        self.author(taxonomy.definitions(), context.preselected_type())
            .map(|outcome| match outcome {
                CommitDraftAuthorOutcome::Cancelled => CommitAuthoringOutcome::Cancelled,
                CommitDraftAuthorOutcome::Authored(draft) => {
                    let authored = match taxonomy.validate(&draft).into_message() {
                        Ok(message) => {
                            AuthoredCommit::reviewed(taxonomy.id().clone(), draft, message)
                        }
                        Err(_) => AuthoredCommit::new(taxonomy.id().clone(), draft),
                    };
                    CommitAuthoringOutcome::Authored(authored)
                }
            })
    }
    fn author(
        &self,
        definitions: &[CommitTypeDefinition],
        preselected: Option<&CommitTypeDefinition>,
    ) -> Result<CommitDraftAuthorOutcome, Self::Error> {
        self.trace.borrow_mut().push("author");
        self.seen.borrow_mut().push((
            definitions
                .iter()
                .map(|definition| definition.id().clone())
                .collect(),
            preselected.map(|definition| definition.id().clone()),
        ));
        match &*self.result.borrow() {
            FakeAuthorResult::Valid(index) => valid_draft(&definitions[*index])
                .map(CommitDraftAuthorOutcome::Authored)
                .map_err(|_| FakeError("invalid fake draft")),
            FakeAuthorResult::Outcome(outcome) => Ok(outcome.clone()),
            FakeAuthorResult::Error(error) => Err(*error),
        }
    }
}

struct FakeWriter {
    fail: bool,
    messages: RefCell<Vec<String>>,
    trace: Trace,
}

impl CommitWriter for FakeWriter {
    type Error = FakeError;

    fn commit(
        &self,
        _root: &RepositoryRoot,
        message: &CommitMessage,
    ) -> Result<CommitOutput, Self::Error> {
        self.trace.borrow_mut().push("write");
        self.messages.borrow_mut().push(message.as_str().to_owned());
        if self.fail {
            Err(FakeError("writer failed"))
        } else {
            Ok(CommitOutput::new(
                b"git stdout\n".to_vec(),
                b"git stderr\n".to_vec(),
            ))
        }
    }
}

fn repository_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fake-repository")
}

fn initialized_state() -> Result<Option<ProjectConfiguration>, Box<dyn Error>> {
    Ok(Some(initial_project_configuration(
        &GlobalConfiguration::default(),
    )?))
}

fn valid_draft(definition: &CommitTypeDefinition) -> Result<CommitDraft, Box<dyn Error>> {
    let properties = definition
        .properties()
        .iter()
        .filter(|property| property.requirement() == &PropertyRequirement::Required)
        .map(|property| {
            Ok(AuthoredProperty::new(
                property.key().clone(),
                PropertyValue::new(format!("authored {}", property.key()))?,
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    Ok(CommitDraft::new(
        definition.id().clone(),
        None,
        CommitSubject::new("create durable commit")?,
        properties,
    )?)
}

struct Harness {
    trace: Trace,
    locator: FakeLocator,
    store: FakeStore,
    author: FakeAuthor,
    writer: FakeWriter,
}

impl Harness {
    fn new(state: Option<ProjectConfiguration>) -> Self {
        let trace = Trace::default();
        Self {
            locator: FakeLocator {
                fail: false,
                trace: Rc::clone(&trace),
            },
            store: FakeStore {
                state: RefCell::new(Ok(state)),
                trace: Rc::clone(&trace),
            },
            author: FakeAuthor {
                result: RefCell::new(FakeAuthorResult::Valid(0)),
                seen: RefCell::default(),
                trace: Rc::clone(&trace),
            },
            writer: FakeWriter {
                fail: false,
                messages: RefCell::default(),
                trace: Rc::clone(&trace),
            },
            trace,
        }
    }

    fn run(
        &self,
    ) -> Result<CommitOutcome, CreateCommitError<FakeError, FakeError, FakeError, FakeError>> {
        create_commit_with_taxonomy(
            &self.locator,
            &self.store,
            &self.author,
            &self.writer,
            &repository_path(),
        )
    }

    fn run_with<A>(
        &self,
        author: &A,
    ) -> Result<CommitOutcome, CreateCommitError<FakeError, FakeError, FakeError, FakeError>>
    where
        A: CommitDraftAuthor<Error = FakeError>,
    {
        create_commit_with_taxonomy(
            &self.locator,
            &self.store,
            author,
            &self.writer,
            &repository_path(),
        )
    }
}

#[test]
fn staged_commit_is_authored_once_and_written_in_port_order() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);

    let outcome = harness.run()?;

    assert_eq!(
        outcome,
        CommitOutcome::Created(CommitOutput::new(
            b"git stdout\n".to_vec(),
            b"git stderr\n".to_vec()
        ))
    );
    assert_eq!(
        harness.trace.borrow().as_slice(),
        ["locate", "load", "author", "write"]
    );
    let seen = harness.author.seen.borrow();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0.len(), built_in_commit_types().len());
    let messages = harness.writer.messages.borrow();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].starts_with("feat: create durable commit\n"));
    assert!(messages[0].contains("Gitserious-Taxonomy: conventional-v1\n"));
    Ok(())
}

#[test]
fn author_selects_the_type_from_locked_policy_order() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);

    assert!(matches!(harness.run()?, CommitOutcome::Created(_)));

    let seen = harness.author.seen.borrow();
    assert_eq!(seen[0].1, None);
    assert_eq!(
        seen[0].0,
        built_in_commit_types()
            .iter()
            .map(|definition| definition.id().clone())
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn author_cancellation_never_invokes_the_writer() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    harness.author.result.replace(FakeAuthorResult::Outcome(
        CommitDraftAuthorOutcome::Cancelled,
    ));

    assert_eq!(harness.run()?, CommitOutcome::Cancelled);

    assert!(harness.writer.messages.borrow().is_empty());
    assert_eq!(
        harness.trace.borrow().as_slice(),
        ["locate", "load", "author"]
    );
    Ok(())
}

#[test]
fn uninitialized_project_is_rejected_before_authoring() {
    let harness = Harness::new(None);
    assert!(matches!(
        harness.run(),
        Err(error @ CreateCommitError::Policy(_))
            if error.to_string().contains("run `gitserious init` before committing")
    ));
    assert!(harness.author.seen.borrow().is_empty());
}

#[test]
fn project_owned_built_in_identity_replaces_the_compiled_schema() -> Result<(), Box<dyn Error>> {
    let conventional = Taxonomy::new(
        TaxonomyId::new("conventional")?,
        Some(TaxonomyVersion::new(7)?),
        Description::new("A project-owned conventional taxonomy.")?,
        None,
        vec![CommitTypeDefinition::new(
            Some(SchemaVersion::V1),
            CommitTypeId::new("ship")?,
            "Ships a change.",
            vec![PropertyDefinition::new(
                PropertyKey::new("intent")?,
                "Why the change ships.",
                PropertyRequirement::Required,
            )?],
        )?],
    )?;
    let project = ProjectConfiguration::new(1, conventional.id().clone(), vec![conventional])?;
    let harness = Harness::new(Some(project));

    assert!(matches!(harness.run()?, CommitOutcome::Created(_)));
    assert!(harness.writer.messages.borrow()[0].contains("Gitserious-Taxonomy: conventional-v7\n"));
    Ok(())
}

#[test]
fn authored_types_outside_the_taxonomy_never_reach_git() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    let custom = CommitDraft::new(
        CommitTypeId::new("custom")?,
        None,
        CommitSubject::new("escape policy")?,
        Vec::new(),
    )?;
    harness.author.result.replace(FakeAuthorResult::Outcome(
        CommitDraftAuthorOutcome::Authored(custom),
    ));
    assert!(matches!(
        harness.run(),
        Err(CreateCommitError::UnknownCommitType { .. })
    ));
    assert!(harness.writer.messages.borrow().is_empty());
    Ok(())
}

#[test]
fn invalid_authored_draft_is_revalidated_before_writing() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    let feat = built_in_commit_types()
        .iter()
        .find(|definition| definition.id().as_str() == "feat")
        .ok_or("conventional has no feat")?;
    let required = feat
        .properties()
        .iter()
        .filter(|property| property.requirement() == &PropertyRequirement::Required)
        .count();
    let incomplete = CommitDraft::new(
        feat.id().clone(),
        None,
        CommitSubject::new("missing properties")?,
        Vec::new(),
    )?;
    harness.author.result.replace(FakeAuthorResult::Outcome(
        CommitDraftAuthorOutcome::Authored(incomplete),
    ));

    assert!(matches!(
        harness.run(),
        Err(CreateCommitError::InvalidDraft(errors)) if errors.as_slice().len() == required
    ));
    assert!(required > 0);
    assert!(harness.writer.messages.borrow().is_empty());
    Ok(())
}

#[test]
fn every_port_failure_is_preserved_and_stops_downstream_calls() -> Result<(), Box<dyn Error>> {
    let mut locator = Harness::new(initialized_state()?);
    locator.locator.fail = true;
    assert!(matches!(
        locator.run(),
        Err(CreateCommitError::Repository(FakeError("locator failed")))
    ));
    assert_eq!(locator.trace.borrow().as_slice(), ["locate"]);

    let store = Harness::new(initialized_state()?);
    let _ = store.store.state.replace(Err(FakeError("store failed")));
    assert!(matches!(
        store.run(),
        Err(CreateCommitError::Store(FakeError("store failed")))
    ));
    assert_eq!(store.trace.borrow().as_slice(), ["locate", "load"]);

    let author = Harness::new(initialized_state()?);
    author
        .author
        .result
        .replace(FakeAuthorResult::Error(FakeError("author failed")));
    assert!(matches!(
        author.run(),
        Err(CreateCommitError::Author(FakeError("author failed")))
    ));
    assert!(author.writer.messages.borrow().is_empty());

    let mut writer = Harness::new(initialized_state()?);
    writer.writer.fail = true;
    assert!(matches!(
        writer.run(),
        Err(CreateCommitError::Writer(FakeError("writer failed")))
    ));
    assert_eq!(writer.writer.messages.borrow().len(), 1);
    Ok(())
}

#[test]
fn custom_taxonomy_commits_with_its_own_schema() -> Result<(), Box<dyn Error>> {
    let ops = Taxonomy::new(
        TaxonomyId::new("ops")?,
        Some(TaxonomyVersion::V1),
        Description::new("Operations change classes.")?,
        None,
        vec![CommitTypeDefinition::new(
            Some(SchemaVersion::V1),
            CommitTypeId::new("provision")?,
            "Infrastructure provisioning.",
            vec![PropertyDefinition::new(
                PropertyKey::new("intent")?,
                "Why the infrastructure must change.",
                PropertyRequirement::Required,
            )?],
        )?],
    )?;
    let harness = Harness::new(Some(ProjectConfiguration::new(
        1,
        ops.id().clone(),
        vec![ops],
    )?));

    assert!(matches!(harness.run()?, CommitOutcome::Created(_)));

    assert_eq!(
        harness.author.seen.borrow()[0].0,
        vec![CommitTypeId::new("provision")?]
    );
    let message = &harness.writer.messages.borrow()[0];
    assert!(message.starts_with("provision: create durable commit\n"));
    assert!(message.contains("intent:\nauthored intent\n"));
    assert!(message.contains("Gitserious-Taxonomy: ops\n"));
    Ok(())
}

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
    choose: Option<TaxonomyId>,
    forged: Option<TaxonomyId>,
    initial_schema: bool,
    review_mode: ReviewMode,
    change_policy: Option<&'a FakeStore>,
    reviewed: RefCell<Vec<String>>,
    seen: RefCell<Vec<CommitAuthoringContext>>,
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
            .unwrap_or(context.initial_taxonomy().id());
        let taxonomy = context
            .find_taxonomy(selected)
            .ok_or(FakeError("missing taxonomy"))?;
        let schema = if self.initial_schema {
            context.initial_taxonomy()
        } else {
            taxonomy
        };
        let definition = schema
            .definitions()
            .iter()
            .find(|definition| definition.id().as_str() == "fix")
            .ok_or(FakeError("missing fix"))?;
        let draft = valid_draft(definition).map_err(|_| FakeError("invalid draft"))?;
        let message = match self.review_mode {
            ReviewMode::Missing => None,
            ReviewMode::Plain => validate_commit_draft(definition, &draft)
                .into_message()
                .ok(),
            ReviewMode::WrongFingerprint => validate_commit_draft_with_provenance(
                &CommitProvenance::new(
                    built_in_taxonomies()
                        .iter()
                        .find(|built_in| built_in.id() == taxonomy.id())
                        .ok_or(FakeError("missing built-in"))?
                        .clone(),
                    Fingerprint::from_bytes([0; 32]),
                ),
                &draft,
            )
            .into_message()
            .ok(),
            ReviewMode::Canonical => taxonomy.validate(&draft).into_message().ok(),
        };
        let id = self.forged.clone().unwrap_or_else(|| taxonomy.id().clone());
        let authored = match message {
            Some(message) => {
                self.reviewed.borrow_mut().push(message.as_str().to_owned());
                AuthoredCommit::reviewed(id, draft, message)
            }
            None => AuthoredCommit::new(id, draft),
        };
        if let Some(store) = self.change_policy {
            let research = TaxonomyId::new("research").map_err(|_| FakeError("invalid id"))?;
            *store.state.borrow_mut() = Ok(Some(
                ProjectConfiguration::new(1, research, built_in_taxonomies().to_vec())
                    .map_err(|_| FakeError("invalid policy"))?,
            ));
        }
        Ok(CommitAuthoringOutcome::Authored(authored))
    }
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

        let error = harness
            .run_with(&author)
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

    harness.run_with(&author)?;

    assert_eq!(*harness.writer.messages.borrow(), *author.reviewed.borrow());
    assert!(harness.writer.messages.borrow()[0].contains("Gitserious-Taxonomy: conventional-v1\n"));
    assert!(matches!(
        &*harness.store.state.borrow(),
        Ok(Some(config)) if config.default_taxonomy().as_str() == "research"
    ));
    assert_eq!(
        harness
            .trace
            .borrow()
            .iter()
            .filter(|call| **call == "load")
            .count(),
        1
    );
    Ok(())
}

struct DraftOnlyAuthor;

impl CommitDraftAuthor for DraftOnlyAuthor {
    type Error = FakeError;

    fn author(
        &self,
        definitions: &[CommitTypeDefinition],
        _: Option<&CommitTypeDefinition>,
    ) -> Result<CommitDraftAuthorOutcome, Self::Error> {
        valid_draft(&definitions[0])
            .map(CommitDraftAuthorOutcome::Authored)
            .map_err(|_| FakeError("invalid draft"))
    }
}

#[test]
fn draft_only_adapter_cannot_write_unreviewed_provenance() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);

    assert!(matches!(
        harness.run_with(&DraftOnlyAuthor),
        Err(CreateCommitError::MissingReviewedMessage)
    ));
    assert!(harness.writer.messages.borrow().is_empty());
    Ok(())
}

#[test]
fn authored_taxonomy_choice_uses_its_own_schema_without_policy_writes() -> Result<(), Box<dyn Error>>
{
    let initial = initialized_state()?;
    let harness = Harness::new(initial.clone());
    let author = ContextAuthor {
        choose: Some(TaxonomyId::new("research")?),
        ..Default::default()
    };

    harness.run_with(&author)?;

    let seen = author.seen.borrow();
    assert_eq!(seen[0].initial_taxonomy().id().as_str(), "conventional");
    assert_eq!(seen[0].taxonomies().len(), built_in_taxonomies().len());
    let messages = harness.writer.messages.borrow();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].contains("symptom:\n"));
    assert!(messages[0].contains("Gitserious-Taxonomy: research-v1\n"));
    assert_eq!(*harness.store.state.borrow(), Ok(initial));
    Ok(())
}

#[test]
fn unknown_taxonomy_and_cross_schema_drafts_never_reach_git() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    let author = ContextAuthor {
        forged: Some(TaxonomyId::new("global-only")?),
        ..Default::default()
    };
    assert!(matches!(
        harness.run_with(&author),
        Err(CreateCommitError::UnknownTaxonomy { .. })
    ));

    let author = ContextAuthor {
        choose: Some(TaxonomyId::new("research")?),
        initial_schema: true,
        ..Default::default()
    };
    assert!(matches!(
        harness.run_with(&author),
        Err(CreateCommitError::InvalidDraft(_))
    ));
    assert!(harness.writer.messages.borrow().is_empty());
    Ok(())
}

#[test]
fn authoring_context_rejects_ambiguous_or_missing_initial_selections() -> Result<(), Box<dyn Error>>
{
    let conventional = built_in_taxonomies()[0].clone();
    let id = conventional.id().clone();

    assert!(CommitAuthoringContext::new(vec![], &id, None).is_err());
    assert!(
        CommitAuthoringContext::new(vec![conventional.clone(), conventional.clone()], &id, None)
            .is_err()
    );
    assert!(
        CommitAuthoringContext::new(
            vec![conventional.clone()],
            &TaxonomyId::new("absent")?,
            None
        )
        .is_err()
    );
    assert!(
        CommitAuthoringContext::new(vec![conventional], &id, Some(&CommitTypeId::new("absent")?))
            .is_err()
    );
    Ok(())
}

#[test]
fn taxonomy_render_binds_the_application_fingerprint_into_provenance() -> Result<(), Box<dyn Error>>
{
    let conventional = built_in_taxonomies()[0].clone();
    let fingerprint = fingerprint_taxonomy(&conventional);
    let draft = CommitDraft::from_responses(
        CommitTypeId::new("docs")?,
        None,
        CommitSubject::new("correct a typo")?,
        vec![PropertyResponse::new(
            PropertyKey::new("reason")?,
            None,
            Some(ConditionalApplicability::DoesNotApply),
        )],
    )?;

    let message: CommitMessage = CommitTaxonomy::new(conventional)
        .validate(&draft)
        .into_message()?;

    assert_eq!(
        message.as_str(),
        format!(
            "docs: correct a typo\n\nGitserious-Taxonomy: conventional-v1\nGitserious-Schema: {fingerprint}\n"
        )
    );
    assert_eq!(fingerprint.to_string().parse::<Fingerprint>()?, fingerprint);
    Ok(())
}
