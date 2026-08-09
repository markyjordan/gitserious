use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gitserious_app::{
    CommitDraftEditor, CommitOutcome, CommitOutput, CommitPolicyError, CommitTypeCatalog,
    CommitTypeSelection, CommitTypeSelector, CommitWriter, CreateCommitError, ProjectConfig,
    ProjectLock, ProjectState, ProjectStateStore, RepositoryLocator, RepositoryRoot, create_commit,
    resolve_project_lock,
};
use gitserious_core::{
    CommitMessage, CommitTypeDefinition, CommitTypeId, SchemaVersion, built_in_commit_types,
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

struct FakeCatalog {
    definitions: Vec<CommitTypeDefinition>,
    fail: bool,
    trace: Trace,
}

impl CommitTypeCatalog for FakeCatalog {
    type Error = FakeError;

    fn find(&self, _id: &CommitTypeId) -> Result<Option<CommitTypeDefinition>, Self::Error> {
        Err(FakeError("unexpected find"))
    }

    fn list(&self) -> Result<Vec<CommitTypeDefinition>, Self::Error> {
        self.trace.borrow_mut().push("catalog");
        if self.fail {
            Err(FakeError("catalog failed"))
        } else {
            Ok(self.definitions.clone())
        }
    }
}

struct FakeSelector {
    result: RefCell<Result<CommitTypeSelection, FakeError>>,
    seen: RefCell<Vec<Vec<CommitTypeId>>>,
    trace: Trace,
}

impl CommitTypeSelector for FakeSelector {
    type Error = FakeError;

    fn select(
        &self,
        definitions: &[CommitTypeDefinition],
    ) -> Result<CommitTypeSelection, Self::Error> {
        self.trace.borrow_mut().push("select");
        self.seen.borrow_mut().push(
            definitions
                .iter()
                .map(|definition| definition.id().clone())
                .collect(),
        );
        self.result.borrow().clone()
    }
}

enum EditorResponse {
    Text(String),
    Echo,
    Error,
}

struct FakeEditor {
    responses: RefCell<VecDeque<EditorResponse>>,
    documents: RefCell<Vec<String>>,
    trace: Trace,
}

impl CommitDraftEditor for FakeEditor {
    type Error = FakeError;

    fn edit(&self, _root: &RepositoryRoot, document: &str) -> Result<String, Self::Error> {
        self.trace.borrow_mut().push("edit");
        self.documents.borrow_mut().push(document.to_owned());
        match self.responses.borrow_mut().pop_front() {
            Some(EditorResponse::Text(text)) => Ok(text),
            Some(EditorResponse::Echo) => Ok(document.to_owned()),
            Some(EditorResponse::Error) => Err(FakeError("editor failed")),
            None => Err(FakeError("editor response missing")),
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
    let lock = resolve_project_lock(&config)?;
    Ok(ProjectState::Initialized { config, lock })
}

fn feat_document() -> String {
    "feat(app): create durable commit\n\nintent:\n  coordinate ports\n\nbehavior:\n  create one commit\n"
        .to_owned()
}

struct Harness {
    trace: Trace,
    locator: FakeLocator,
    store: FakeStore,
    catalog: FakeCatalog,
    selector: FakeSelector,
    editor: FakeEditor,
    writer: FakeWriter,
}

impl Harness {
    fn new(state: ProjectState) -> Self {
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
            catalog: FakeCatalog {
                definitions: built_in_commit_types().to_vec(),
                fail: false,
                trace: Rc::clone(&trace),
            },
            selector: FakeSelector {
                result: RefCell::new(Ok(CommitTypeSelection::Selected(
                    built_in_commit_types()[0].id().clone(),
                ))),
                seen: RefCell::default(),
                trace: Rc::clone(&trace),
            },
            editor: FakeEditor {
                responses: RefCell::new(VecDeque::from([EditorResponse::Text(feat_document())])),
                documents: RefCell::default(),
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
        requested: Option<&CommitTypeId>,
    ) -> Result<
        CommitOutcome,
        CreateCommitError<FakeError, FakeError, FakeError, FakeError, FakeError, FakeError>,
    > {
        create_commit(
            &self.locator,
            &self.store,
            &self.catalog,
            &self.selector,
            &self.editor,
            &self.writer,
            &repository_path(),
            requested,
        )
    }
}

#[test]
fn requested_type_bypasses_selection_and_commits_once_in_port_order() -> Result<(), Box<dyn Error>>
{
    let harness = Harness::new(initialized_state()?);
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
        ["locate", "inspect", "catalog", "edit", "write"]
    );
    assert!(harness.selector.seen.borrow().is_empty());
    assert_eq!(harness.writer.messages.borrow().len(), 1);
    assert_eq!(
        harness.writer.messages.borrow()[0],
        "feat(app): create durable commit\n\nintent:\n  coordinate ports\n\nbehavior:\n  create one commit\n"
    );
    Ok(())
}

#[test]
fn omitted_type_selects_from_locked_policy_order() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    assert!(matches!(harness.run(None)?, CommitOutcome::Created(_)));
    assert_eq!(harness.selector.seen.borrow().len(), 1);
    assert_eq!(
        harness.selector.seen.borrow()[0],
        built_in_commit_types()
            .iter()
            .map(|definition| definition.id().clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        harness.trace.borrow().as_slice(),
        ["locate", "inspect", "catalog", "select", "edit", "write"]
    );
    Ok(())
}

#[test]
fn selector_and_unchanged_or_empty_editor_cancellation_never_write() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    let _ = harness
        .selector
        .result
        .replace(Ok(CommitTypeSelection::Cancelled));
    assert_eq!(harness.run(None)?, CommitOutcome::Cancelled);
    assert!(harness.editor.documents.borrow().is_empty());
    assert!(harness.writer.messages.borrow().is_empty());

    for response in [
        EditorResponse::Echo,
        EditorResponse::Text("# comment only\n".to_owned()),
    ] {
        let harness = Harness::new(initialized_state()?);
        harness.editor.responses.replace(VecDeque::from([response]));
        let requested = CommitTypeId::new("feat")?;
        assert_eq!(harness.run(Some(&requested))?, CommitOutcome::Cancelled);
        assert!(harness.writer.messages.borrow().is_empty());
    }
    Ok(())
}

#[test]
fn invalid_partial_draft_is_annotated_and_reopened_before_one_write() -> Result<(), Box<dyn Error>>
{
    let harness = Harness::new(initialized_state()?);
    harness.editor.responses.replace(VecDeque::from([
        EditorResponse::Text("feat: partial\n".to_owned()),
        EditorResponse::Text(feat_document()),
    ]));
    let requested = CommitTypeId::new("feat")?;
    assert!(matches!(
        harness.run(Some(&requested))?,
        CommitOutcome::Created(_)
    ));
    let documents = harness.editor.documents.borrow();
    assert_eq!(documents.len(), 2);
    assert!(documents[1].starts_with("# gitserious could not use this draft:"));
    assert!(documents[1].contains("complete required property"));
    assert!(documents[1].contains("intent"));
    assert!(documents[1].ends_with("feat: partial\n"));
    assert_eq!(harness.writer.messages.borrow().len(), 1);
    Ok(())
}

#[test]
fn every_incomplete_project_state_is_rejected_before_catalog_access() -> Result<(), Box<dyn Error>>
{
    let config = ProjectConfig::default_channel()?;
    let lock = resolve_project_lock(&config)?;
    for (state, expected) in [
        (ProjectState::Absent, CommitPolicyError::NotInitialized),
        (
            ProjectState::ConfigOnly(config),
            CommitPolicyError::MissingLock,
        ),
        (ProjectState::LockOnly, CommitPolicyError::OrphanLock),
    ] {
        let harness = Harness::new(state);
        let error = harness
            .run(None)
            .err()
            .ok_or("state unexpectedly accepted")?;
        assert!(matches!(error, CreateCommitError::Policy(actual) if actual == expected));
        assert_eq!(harness.trace.borrow().as_slice(), ["locate", "inspect"]);
    }

    let stale_config = ProjectConfig::default_channel()?;
    let stale = ProjectState::Initialized {
        config: stale_config,
        lock: ProjectLock::new(
            lock.version(),
            "sha256:0000000000000000000000000000000000000000000000000000000000000000".parse()?,
            lock.template_reference().clone(),
            lock.resolved_template().clone(),
        )?,
    };
    let harness = Harness::new(stale);
    assert!(matches!(
        harness.run(None),
        Err(CreateCommitError::Policy(CommitPolicyError::StaleLock))
    ));
    Ok(())
}

#[test]
fn requested_and_selected_types_must_belong_to_current_policy() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    let unknown = CommitTypeId::new("custom")?;
    let error = harness
        .run(Some(&unknown))
        .err()
        .ok_or("unknown type accepted")?;
    match error {
        CreateCommitError::UnknownCommitType {
            requested,
            available,
        } => {
            assert_eq!(requested, unknown);
            assert_eq!(available.len(), 11);
        }
        other => return Err(format!("unexpected error: {other}").into()),
    }

    let harness = Harness::new(initialized_state()?);
    let _ = harness
        .selector
        .result
        .replace(Ok(CommitTypeSelection::Selected(CommitTypeId::new(
            "custom",
        )?)));
    assert!(matches!(
        harness.run(None),
        Err(CreateCommitError::UnknownCommitType { .. })
    ));
    assert!(harness.editor.documents.borrow().is_empty());
    Ok(())
}

#[test]
fn locked_catalog_requires_every_exact_schema_and_fingerprint() -> Result<(), Box<dyn Error>> {
    let mut harness = Harness::new(initialized_state()?);
    harness.catalog.definitions.clear();
    assert!(matches!(
        harness.run(None),
        Err(CreateCommitError::Policy(
            CommitPolicyError::MissingDefinition(_)
        ))
    ));

    let mut version_harness = Harness::new(initialized_state()?);
    let original = &built_in_commit_types()[0];
    version_harness.catalog.definitions[0] = CommitTypeDefinition::new(
        SchemaVersion::new(2)?,
        original.id().clone(),
        original.description(),
        original.properties().to_vec(),
    )?;
    assert!(matches!(
        version_harness.run(None),
        Err(CreateCommitError::Policy(
            CommitPolicyError::SchemaVersionMismatch(_)
        ))
    ));

    let mut fingerprint_harness = Harness::new(initialized_state()?);
    fingerprint_harness.catalog.definitions[0] = CommitTypeDefinition::new(
        original.schema_version(),
        original.id().clone(),
        "A changed definition.",
        original.properties().to_vec(),
    )?;
    assert!(matches!(
        fingerprint_harness.run(None),
        Err(CreateCommitError::Policy(
            CommitPolicyError::DefinitionFingerprintMismatch(_)
        ))
    ));
    Ok(())
}

#[test]
fn each_port_failure_is_preserved_and_stops_downstream_calls() -> Result<(), Box<dyn Error>> {
    let mut harness = Harness::new(initialized_state()?);
    harness.locator.fail = true;
    assert!(matches!(
        harness.run(None),
        Err(CreateCommitError::Repository(FakeError("locator failed")))
    ));

    let harness = Harness::new(initialized_state()?);
    let _ = harness.store.state.replace(Err(FakeError("store failed")));
    assert!(matches!(
        harness.run(None),
        Err(CreateCommitError::Store(FakeError("store failed")))
    ));

    let mut harness = Harness::new(initialized_state()?);
    harness.catalog.fail = true;
    assert!(matches!(
        harness.run(None),
        Err(CreateCommitError::Catalog(FakeError("catalog failed")))
    ));

    let harness = Harness::new(initialized_state()?);
    let _ = harness
        .selector
        .result
        .replace(Err(FakeError("selector failed")));
    assert!(matches!(
        harness.run(None),
        Err(CreateCommitError::Selector(FakeError("selector failed")))
    ));

    let harness = Harness::new(initialized_state()?);
    harness
        .editor
        .responses
        .replace(VecDeque::from([EditorResponse::Error]));
    let feat = CommitTypeId::new("feat")?;
    assert!(matches!(
        harness.run(Some(&feat)),
        Err(CreateCommitError::Editor(FakeError("editor failed")))
    ));

    let mut harness = Harness::new(initialized_state()?);
    harness.writer.fail = true;
    assert!(matches!(
        harness.run(Some(&feat)),
        Err(CreateCommitError::Writer(FakeError("writer failed")))
    ));
    assert_eq!(harness.writer.messages.borrow().len(), 1);
    Ok(())
}
