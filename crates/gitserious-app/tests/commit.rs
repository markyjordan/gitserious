use std::cell::RefCell;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gitserious_app::{
    CommitDraftAuthor, CommitDraftAuthorOutcome, CommitOutcome, CommitOutput, CommitPolicyError,
    CommitWriter, ConfigurationCatalog, CreateCommitError, ProjectConfig, ProjectLock, ProjectState,
    ProjectStateStore, RepositoryLocator, ResolvedCommitType, ResolvedTemplate, RepositoryRoot,
    UserConfiguration, built_in_effective_catalog, create_commit, resolve_project_lock,
};
use gitserious_core::{
    AuthoredProperty, ChangeTypeDefinition, ChangeTypeId, ChangeTypeSchema, CommitDraft,
    CommitMessage, CommitSubject, CommitTypeDefinition, CommitTypeId, Description,
    PropertyDefinition, PropertyKey, PropertyMultiplicity, PropertyRequirement, PropertyValue,
    PropertyValues, TaxonomyDefinition, TaxonomyId, TaxonomyVersion, TemplateDefinition,
    TemplateId, TemplateVersion, TypesetDefinition, TypesetId, TypesetVersion,
    built_in_commit_types,
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
    state: RefCell<Result<ProjectState, FakeError>>,
    trace: Trace,
}

impl ProjectStateStore for FakeStore {
    type Error = FakeError;

    fn inspect(&self, _root: &RepositoryRoot) -> Result<ProjectState, Self::Error> {
        self.trace.borrow_mut().push("inspect");
        self.state.borrow().clone()
    }

    fn ensure_local_state(&self, _root: &RepositoryRoot) -> Result<(), Self::Error> {
        Err(FakeError("unexpected ensure local state"))
    }

    fn initialize(
        &self,
        _root: &RepositoryRoot,
        _config: &ProjectConfig,
        _lock: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Err(FakeError("unexpected initialize"))
    }

    fn create_lock(&self, _root: &RepositoryRoot, _lock: &ProjectLock) -> Result<(), Self::Error> {
        Err(FakeError("unexpected create lock"))
    }

    fn replace_lock(
        &self,
        _root: &RepositoryRoot,
        _current: &ProjectLock,
        _replacement: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Err(FakeError("unexpected replace lock"))
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

fn initialized_state() -> Result<ProjectState, Box<dyn Error>> {
    let config = ProjectConfig::default_channel()?;
    let lock = resolve_project_lock(&config, &built_in_effective_catalog()?)?;
    Ok(ProjectState::Initialized { config, lock })
}

fn valid_draft(definition: &CommitTypeDefinition) -> Result<CommitDraft, Box<dyn Error>> {
    let properties = definition
        .properties()
        .iter()
        .filter(|property| property.requirement() == &PropertyRequirement::Required)
        .map(|property| {
            Ok(AuthoredProperty::new(
                property.key().clone(),
                PropertyValues::single(PropertyValue::new(format!("authored {}", property.key()))?),
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
    catalog: ConfigurationCatalog,
    author: FakeAuthor,
    writer: FakeWriter,
}

impl Harness {
    fn new(state: ProjectState) -> Result<Self, Box<dyn Error>> {
        let trace = Trace::default();
        Ok(Self {
            locator: FakeLocator {
                fail: false,
                trace: Rc::clone(&trace),
            },
            store: FakeStore {
                state: RefCell::new(Ok(state)),
                trace: Rc::clone(&trace),
            },
            catalog: built_in_effective_catalog()?,
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
        })
    }

    fn run(
        &self,
        requested: Option<&CommitTypeId>,
    ) -> Result<CommitOutcome, CreateCommitError<FakeError, FakeError, FakeError, FakeError>> {
        create_commit(
            &self.locator,
            &self.store,
            &self.catalog,
            &self.author,
            &self.writer,
            &repository_path(),
            requested,
        )
    }
}

#[test]
fn requested_type_is_resolved_before_authoring_and_commits_once_in_port_order()
-> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?)?;
    let requested = CommitTypeId::new("feat")?;
    let outcome = harness.run(Some(&requested))?;
    assert_eq!(
        outcome,
        CommitOutcome::Created(CommitOutput::new(
            b"git stdout\n".to_vec(),
            b"git stderr\n".to_vec()
        ))
    );
    assert_eq!(
        harness.trace.borrow().as_slice(),
        ["locate", "inspect", "author", "write"]
    );
    let seen = harness.author.seen.borrow();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].1.as_ref(), Some(&requested));
    assert_eq!(seen[0].0.len(), built_in_commit_types().len());
    assert_eq!(harness.writer.messages.borrow().len(), 1);
    assert!(harness.writer.messages.borrow()[0].starts_with("feat: create durable commit\n"));
    Ok(())
}

#[test]
fn omitted_type_delegates_selection_with_locked_policy_order() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?)?;
    assert!(matches!(harness.run(None)?, CommitOutcome::Created(_)));
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
    let harness = Harness::new(initialized_state()?)?;
    harness.author.result.replace(FakeAuthorResult::Outcome(
        CommitDraftAuthorOutcome::Cancelled,
    ));
    assert_eq!(harness.run(None)?, CommitOutcome::Cancelled);
    assert!(harness.writer.messages.borrow().is_empty());
    assert_eq!(
        harness.trace.borrow().as_slice(),
        ["locate", "inspect", "author"]
    );
    Ok(())
}

#[test]
fn every_incomplete_project_state_is_rejected_before_authoring() -> Result<(), Box<dyn Error>> {
    let config = ProjectConfig::default_channel()?;
    let lock = resolve_project_lock(&config, &built_in_effective_catalog()?)?;
    for (state, expected) in [
        (ProjectState::Absent, CommitPolicyError::NotInitialized),
        (
            ProjectState::ConfigOnly(config),
            CommitPolicyError::MissingLock,
        ),
        (ProjectState::LockOnly, CommitPolicyError::OrphanLock),
    ] {
        let harness = Harness::new(state)?;
        assert!(matches!(
            harness.run(None),
            Err(CreateCommitError::Policy(actual)) if actual == expected
        ));
        assert!(harness.author.seen.borrow().is_empty());
    }

    let stale = ProjectState::Initialized {
        config: ProjectConfig::default_channel()?,
        lock: ProjectLock::new(
            lock.version(),
            "sha256:0000000000000000000000000000000000000000000000000000000000000000".parse()?,
            lock.template_reference().clone(),
            lock.resolved_template().clone(),
        )?,
    };
    let stale_harness = Harness::new(stale)?;
    assert!(matches!(
        stale_harness.run(None),
        Err(CreateCommitError::Policy(CommitPolicyError::StaleLock))
    ));
    Ok(())
}

#[test]
fn requested_and_authored_types_are_defended_at_the_application_boundary()
-> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?)?;
    let unknown = CommitTypeId::new("custom")?;
    assert!(matches!(
        harness.run(Some(&unknown)),
        Err(CreateCommitError::UnknownCommitType { requested, .. }) if requested == unknown
    ));
    assert!(harness.author.seen.borrow().is_empty());

    let harness = Harness::new(initialized_state()?)?;
    harness.author.result.replace(FakeAuthorResult::Valid(1));
    let feat = CommitTypeId::new("feat")?;
    assert!(matches!(
        harness.run(Some(&feat)),
        Err(CreateCommitError::AuthoredTypeMismatch { expected, actual })
            if expected == feat && actual == CommitTypeId::new("fix")?
    ));
    assert!(harness.writer.messages.borrow().is_empty());

    let harness = Harness::new(initialized_state()?)?;
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
        harness.run(None),
        Err(CreateCommitError::UnknownCommitType { .. })
    ));
    assert!(harness.writer.messages.borrow().is_empty());
    Ok(())
}

#[test]
fn invalid_authored_draft_is_revalidated_before_writing() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?)?;
    let incomplete = CommitDraft::new(
        CommitTypeId::new("feat")?,
        None,
        CommitSubject::new("missing properties")?,
        Vec::new(),
    )?;
    harness.author.result.replace(FakeAuthorResult::Outcome(
        CommitDraftAuthorOutcome::Authored(incomplete),
    ));
    assert!(matches!(
        harness.run(None),
        Err(CreateCommitError::InvalidDraft(errors)) if errors.as_slice().len() == 2
    ));
    assert!(harness.writer.messages.borrow().is_empty());
    Ok(())
}

#[test]
fn tampered_lock_entries_are_detected_before_authoring() -> Result<(), Box<dyn Error>> {
    for tampered in [
        CommitTypeId::new("feat")?,
        CommitTypeId::new("docs")?,
        CommitTypeId::new("revert")?,
    ] {
        let config = ProjectConfig::default_channel()?;
        let catalog = built_in_effective_catalog()?;
        let expected = resolve_project_lock(&config, &catalog)?;
        let entries = expected
            .resolved_template()
            .commit_types()
            .iter()
            .map(|entry| -> Result<ResolvedCommitType, Box<dyn Error>> {
                if entry.id() == &tampered {
                    Ok(ResolvedCommitType::new(
                        entry.id().clone(),
                        entry.schema_version(),
                        "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                            .parse()?,
                    ))
                } else {
                    Ok(entry.clone())
                }
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        let resolved = ResolvedTemplate::new(
            expected.resolved_template().id().clone(),
            expected.resolved_template().version(),
            expected.resolved_template().fingerprint(),
            entries,
        )?;
        let state = ProjectState::Initialized {
            config: config.clone(),
            lock: ProjectLock::new(
                expected.version(),
                expected.config_fingerprint(),
                expected.template_reference().clone(),
                resolved,
            )?,
        };
        let harness = Harness::new(state)?;
        assert!(matches!(
            harness.run(None),
            Err(CreateCommitError::Policy(CommitPolicyError::StaleLock))
        ));
        assert!(harness.author.seen.borrow().is_empty());
    }
    Ok(())
}

#[test]
fn every_port_failure_is_preserved_and_stops_downstream_calls() -> Result<(), Box<dyn Error>> {
    let mut locator = Harness::new(initialized_state()?)?;
    locator.locator.fail = true;
    assert!(matches!(
        locator.run(None),
        Err(CreateCommitError::Repository(FakeError("locator failed")))
    ));

    let store = Harness::new(initialized_state()?)?;
    let _ = store.store.state.replace(Err(FakeError("store failed")));
    assert!(matches!(
        store.run(None),
        Err(CreateCommitError::Store(FakeError("store failed")))
    ));

    let author = Harness::new(initialized_state()?)?;
    author
        .author
        .result
        .replace(FakeAuthorResult::Error(FakeError("author failed")));
    assert!(matches!(
        author.run(None),
        Err(CreateCommitError::Author(FakeError("author failed")))
    ));
    assert!(author.writer.messages.borrow().is_empty());

    let mut writer = Harness::new(initialized_state()?)?;
    writer.writer.fail = true;
    assert!(matches!(
        writer.run(None),
        Err(CreateCommitError::Writer(FakeError("writer failed")))
    ));
    assert_eq!(writer.writer.messages.borrow().len(), 1);
    Ok(())
}

#[test]
fn custom_template_commits_with_its_own_schema() -> Result<(), Box<dyn Error>> {
    let taxonomy = TaxonomyDefinition::new(
        TaxonomyId::new("ops")?,
        TaxonomyVersion::V1,
        Description::new("Operations change classes.")?,
        vec![ChangeTypeDefinition::new(
            ChangeTypeId::new("provision")?,
            Description::new("Infrastructure provisioning.")?,
        )],
    )?;
    let typeset = TypesetDefinition::new(
        taxonomy.id().clone(),
        TypesetId::new("baseline")?,
        TypesetVersion::V1,
        Description::new("Baseline durable context for operations.")?,
        vec![ChangeTypeSchema::new(
            ChangeTypeId::new("provision")?,
            vec![PropertyDefinition::new(
                PropertyKey::new("intent")?,
                "Why the infrastructure must change.",
                PropertyRequirement::Required,
                PropertyMultiplicity::Single,
            )?],
        )?],
    )?;
    let template = TemplateDefinition::new(
        TemplateId::new("platform")?,
        TemplateVersion::V1,
        Description::new("Platform operations policy.")?,
        taxonomy.id().clone(),
        typeset.id().clone(),
    );
    let configuration =
        UserConfiguration::new(vec![taxonomy], vec![typeset], vec![template])?;
    let catalog = ConfigurationCatalog::new(&configuration)?;
    let config = ProjectConfig::new(gitserious_app::PROJECT_CONFIG_VERSION, TemplateId::new("platform")?)?;
    let lock = resolve_project_lock(&config, &catalog)?;

    let trace = Trace::default();
    let locator = FakeLocator {
        fail: false,
        trace: Rc::clone(&trace),
    };
    let store = FakeStore {
        state: RefCell::new(Ok(ProjectState::Initialized { config, lock })),
        trace: Rc::clone(&trace),
    };
    let author = FakeAuthor {
        result: RefCell::new(FakeAuthorResult::Valid(0)),
        seen: RefCell::default(),
        trace: Rc::clone(&trace),
    };
    let writer = FakeWriter {
        fail: false,
        messages: RefCell::default(),
        trace: Rc::clone(&trace),
    };
    let outcome = create_commit(
        &locator,
        &store,
        &catalog,
        &author,
        &writer,
        &repository_path(),
        None,
    )?;
    assert!(matches!(outcome, CommitOutcome::Created(_)));
    assert_eq!(
        author.seen.borrow()[0].0,
        vec![CommitTypeId::new("provision")?]
    );
    let message = &writer.messages.borrow()[0];
    assert!(message.starts_with("provision: create durable commit\n"));
    assert!(message.contains("intent:\nauthored intent\n"));
    Ok(())
}
