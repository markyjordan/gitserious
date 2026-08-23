use std::cell::{Cell, RefCell};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gitserious_app::{
    CommitDraftAuthor, CommitDraftAuthorOutcome, CommitOutput, CommitWriter, EffectiveDefinitions,
    ProjectConfig, ProjectLock, ProjectState, ProjectStateStore, RepositoryLocator, RepositoryRoot,
    resolve_project_lock,
};
use gitserious_cli::{CommitAdapters, run_from_with_commit};
use gitserious_core::{
    AuthoredProperty, CommitDraft, CommitMessage, CommitScope, CommitSubject, CommitTypeDefinition,
    CommitTypeId, PropertyRequirement, PropertyValue, PropertyValues, built_in_commit_types,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FakeError;

impl Display for FakeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("fake adapter failure")
    }
}

impl Error for FakeError {}

struct FakeLocator;

impl RepositoryLocator for FakeLocator {
    type Error = FakeError;

    fn locate(&self, _start: &Path) -> Result<RepositoryRoot, Self::Error> {
        RepositoryRoot::new(repository_path()).map_err(|_| FakeError)
    }
}

struct FakeStore {
    state: ProjectState,
}

impl ProjectStateStore for FakeStore {
    type Error = FakeError;

    fn inspect(&self, _root: &RepositoryRoot) -> Result<ProjectState, Self::Error> {
        Ok(self.state.clone())
    }

    fn ensure_local_state(&self, _root: &RepositoryRoot) -> Result<(), Self::Error> {
        Err(FakeError)
    }

    fn initialize(
        &self,
        _root: &RepositoryRoot,
        _config: &ProjectConfig,
        _lock: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Err(FakeError)
    }

    fn create_lock(&self, _root: &RepositoryRoot, _lock: &ProjectLock) -> Result<(), Self::Error> {
        Err(FakeError)
    }

    fn replace_lock(
        &self,
        _root: &RepositoryRoot,
        _current: &ProjectLock,
        _replacement: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Err(FakeError)
    }
}

struct FakeCatalog {
    calls: Cell<usize>,
}

impl EffectiveDefinitions for FakeCatalog {
    type Error = FakeError;

    fn for_template(
        &self,
        _template: &gitserious_core::TemplateId,
    ) -> Result<Vec<CommitTypeDefinition>, Self::Error> {
        self.calls.set(self.calls.get() + 1);
        Ok(built_in_commit_types().to_vec())
    }
}

struct FakeAuthor {
    outcome: RefCell<AuthorOutcome>,
    calls: Cell<usize>,
    seen: RefCell<Vec<(Vec<CommitTypeId>, Option<CommitTypeId>)>>,
}

enum AuthorOutcome {
    Valid,
    Cancelled,
    Failed,
}

impl CommitDraftAuthor for FakeAuthor {
    type Error = FakeError;

    fn author(
        &self,
        definitions: &[CommitTypeDefinition],
        preselected: Option<&CommitTypeDefinition>,
    ) -> Result<CommitDraftAuthorOutcome, Self::Error> {
        self.calls.set(self.calls.get() + 1);
        self.seen.borrow_mut().push((
            definitions
                .iter()
                .map(|definition| definition.id().clone())
                .collect(),
            preselected.map(|definition| definition.id().clone()),
        ));
        match *self.outcome.borrow() {
            AuthorOutcome::Valid => valid_draft(&definitions[0])
                .map(CommitDraftAuthorOutcome::Authored)
                .map_err(|_| FakeError),
            AuthorOutcome::Cancelled => Ok(CommitDraftAuthorOutcome::Cancelled),
            AuthorOutcome::Failed => Err(FakeError),
        }
    }
}

struct FakeWriter {
    calls: Cell<usize>,
    messages: RefCell<Vec<String>>,
}

impl CommitWriter for FakeWriter {
    type Error = FakeError;

    fn commit(
        &self,
        _root: &RepositoryRoot,
        message: &CommitMessage,
    ) -> Result<CommitOutput, Self::Error> {
        self.calls.set(self.calls.get() + 1);
        self.messages.borrow_mut().push(message.as_str().to_owned());
        Ok(CommitOutput::new(
            b"git summary\n".to_vec(),
            b"hook warning\n".to_vec(),
        ))
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
        Some(CommitScope::new("tui editor")?),
        CommitSubject::new("expose command")?,
        properties,
    )?
    .with_breaking_change(PropertyValue::new("replace CLI contract")?))
}

struct Harness {
    store: FakeStore,
    catalog: FakeCatalog,
    author: FakeAuthor,
    writer: FakeWriter,
}

impl Harness {
    fn new(state: ProjectState) -> Self {
        Self {
            store: FakeStore { state },
            catalog: FakeCatalog {
                calls: Cell::new(0),
            },
            author: FakeAuthor {
                outcome: RefCell::new(AuthorOutcome::Valid),
                calls: Cell::new(0),
                seen: RefCell::default(),
            },
            writer: FakeWriter {
                calls: Cell::new(0),
                messages: RefCell::default(),
            },
        }
    }

    fn run(&self, arguments: &[&str]) -> (ExitCode, String, String) {
        let commit = CommitAdapters::new(&self.catalog, &self.author, &self.writer);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit = run_from_with_commit(
            arguments.iter().copied(),
            &repository_path(),
            &FakeLocator,
            &self.store,
            &commit,
            &mut stdout,
            &mut stderr,
        );
        (
            exit,
            String::from_utf8_lossy(&stdout).into_owned(),
            String::from_utf8_lossy(&stderr).into_owned(),
        )
    }
}

#[test]
fn type_option_is_a_preselection_and_forwards_exact_git_output() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    let (exit, stdout, stderr) = harness.run(&["gitserious", "commit", "--type", "feat"]);
    assert_eq!(exit, ExitCode::SUCCESS);
    assert_eq!(stdout, "git summary\n");
    assert_eq!(stderr, "hook warning\n");
    assert_eq!(harness.author.calls.get(), 1);
    assert_eq!(
        harness.author.seen.borrow()[0].1,
        Some(CommitTypeId::new("feat")?)
    );
    assert_eq!(harness.writer.calls.get(), 1);
    assert_eq!(
        harness.writer.messages.borrow()[0],
        "feat(tui-editor)!: expose command\n\nintent:\nauthored intent\n\ndecision:\nauthored decision\n\nBREAKING CHANGE: replace CLI contract\n"
    );
    Ok(())
}

#[test]
fn bare_commit_delegates_type_selection_to_the_author() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    let (exit, _, _) = harness.run(&["gitserious", "commit"]);
    assert_eq!(exit, ExitCode::SUCCESS);
    let seen = harness.author.seen.borrow();
    assert_eq!(seen[0].1, None);
    assert_eq!(seen[0].0.len(), built_in_commit_types().len());
    assert_eq!(harness.writer.calls.get(), 1);
    Ok(())
}

#[test]
fn cancellation_keeps_the_failure_status_and_never_writes() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    harness.author.outcome.replace(AuthorOutcome::Cancelled);
    let (exit, stdout, stderr) = harness.run(&["gitserious", "commit"]);
    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert_eq!(stderr, "Commit cancelled.\n");
    assert_eq!(harness.writer.calls.get(), 0);
    Ok(())
}

#[test]
fn unavailable_type_is_rejected_before_authoring_in_policy_order() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    let (exit, stdout, stderr) = harness.run(&["gitserious", "commit", "--type", "custom"]);
    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert!(stderr.starts_with("error: commit type "));
    assert!(stderr.contains("custom"));
    assert!(stderr.contains("choose one of: feat, fix"));
    assert!(stderr.contains("revert\n"));
    assert_eq!(harness.author.calls.get(), 0);
    assert_eq!(harness.writer.calls.get(), 0);
    Ok(())
}

#[test]
fn author_errors_are_presented_without_invoking_git() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    harness.author.outcome.replace(AuthorOutcome::Failed);
    let (exit, stdout, stderr) = harness.run(&["gitserious", "commit"]);
    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert_eq!(stderr, "error: fake adapter failure\n");
    assert_eq!(harness.writer.calls.get(), 0);
    Ok(())
}
