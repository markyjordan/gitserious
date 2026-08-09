use std::cell::{Cell, RefCell};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gitserious_app::{
    CommitDraftEditor, CommitOutput, CommitTypeCatalog, CommitTypeSelection, CommitTypeSelector,
    CommitWriter, ProjectConfig, ProjectLock, ProjectState, ProjectStateStore, RepositoryLocator,
    RepositoryRoot, resolve_project_lock,
};
use gitserious_cli::{CommitAdapters, run_from_with_commit};
use gitserious_core::{CommitMessage, CommitTypeDefinition, CommitTypeId, built_in_commit_types};

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

impl CommitTypeCatalog for FakeCatalog {
    type Error = FakeError;

    fn find(&self, id: &CommitTypeId) -> Result<Option<CommitTypeDefinition>, Self::Error> {
        Ok(built_in_commit_types()
            .iter()
            .find(|definition| definition.id() == id)
            .cloned())
    }

    fn list(&self) -> Result<Vec<CommitTypeDefinition>, Self::Error> {
        self.calls.set(self.calls.get() + 1);
        Ok(built_in_commit_types().to_vec())
    }
}

struct FakeSelector {
    selection: RefCell<CommitTypeSelection>,
    calls: Cell<usize>,
}

impl CommitTypeSelector for FakeSelector {
    type Error = FakeError;

    fn select(
        &self,
        _definitions: &[CommitTypeDefinition],
    ) -> Result<CommitTypeSelection, Self::Error> {
        self.calls.set(self.calls.get() + 1);
        Ok(self.selection.borrow().clone())
    }
}

enum EditMode {
    Valid,
    Echo,
}

struct FakeEditor {
    mode: EditMode,
    calls: Cell<usize>,
}

impl CommitDraftEditor for FakeEditor {
    type Error = FakeError;

    fn edit(&self, _root: &RepositoryRoot, document: &str) -> Result<String, Self::Error> {
        self.calls.set(self.calls.get() + 1);
        Ok(match self.mode {
            EditMode::Valid => "feat(cli): expose command\n\nintent:\n  author commits\n\nbehavior:\n  invoke Git\n".to_owned(),
            EditMode::Echo => document.to_owned(),
        })
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

struct Harness {
    store: FakeStore,
    catalog: FakeCatalog,
    selector: FakeSelector,
    editor: FakeEditor,
    writer: FakeWriter,
}

impl Harness {
    fn new(state: ProjectState) -> Self {
        Self {
            store: FakeStore { state },
            catalog: FakeCatalog {
                calls: Cell::new(0),
            },
            selector: FakeSelector {
                selection: RefCell::new(CommitTypeSelection::Selected(
                    built_in_commit_types()[0].id().clone(),
                )),
                calls: Cell::new(0),
            },
            editor: FakeEditor {
                mode: EditMode::Valid,
                calls: Cell::new(0),
            },
            writer: FakeWriter {
                calls: Cell::new(0),
                messages: RefCell::default(),
            },
        }
    }

    fn run(&self, arguments: &[&str]) -> (ExitCode, String, String) {
        let commit = CommitAdapters::new(&self.catalog, &self.selector, &self.editor, &self.writer);
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
fn type_option_bypasses_picker_and_forwards_exact_git_output() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    let (exit, stdout, stderr) = harness.run(&["gitserious", "commit", "--type", "feat"]);
    assert_eq!(exit, ExitCode::SUCCESS);
    assert_eq!(stdout, "git summary\n");
    assert_eq!(stderr, "hook warning\n");
    assert_eq!(harness.selector.calls.get(), 0);
    assert_eq!(harness.editor.calls.get(), 1);
    assert_eq!(harness.writer.calls.get(), 1);
    assert_eq!(
        harness.writer.messages.borrow()[0],
        "feat(cli): expose command\n\nintent:\n  author commits\n\nbehavior:\n  invoke Git\n"
    );
    Ok(())
}

#[test]
fn bare_commit_selects_type_before_opening_editor() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    let (exit, _, _) = harness.run(&["gitserious", "commit"]);
    assert_eq!(exit, ExitCode::SUCCESS);
    assert_eq!(harness.selector.calls.get(), 1);
    assert_eq!(harness.editor.calls.get(), 1);
    assert_eq!(harness.writer.calls.get(), 1);
    Ok(())
}

#[test]
fn selector_and_editor_cancellation_exit_one_without_writing() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    harness
        .selector
        .selection
        .replace(CommitTypeSelection::Cancelled);
    let (exit, stdout, stderr) = harness.run(&["gitserious", "commit"]);
    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert_eq!(stderr, "Commit cancelled.\n");
    assert_eq!(harness.writer.calls.get(), 0);

    let mut harness = Harness::new(initialized_state()?);
    harness.editor.mode = EditMode::Echo;
    let (exit, stdout, stderr) = harness.run(&["gitserious", "commit", "--type", "feat"]);
    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert_eq!(stderr, "Commit cancelled.\n");
    assert_eq!(harness.writer.calls.get(), 0);
    Ok(())
}

#[test]
fn unavailable_type_reports_policy_order_and_never_opens_editor() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    let (exit, stdout, stderr) = harness.run(&["gitserious", "commit", "--type", "custom"]);
    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert!(stderr.starts_with("error: commit type "));
    assert!(stderr.contains("custom"));
    assert!(stderr.contains("choose one of: feat, fix"));
    assert!(stderr.contains("revert\n"));
    assert_eq!(harness.editor.calls.get(), 0);
    assert_eq!(harness.writer.calls.get(), 0);
    Ok(())
}

#[test]
fn syntactically_invalid_types_and_extra_arguments_are_usage_errors() -> Result<(), Box<dyn Error>>
{
    for arguments in [
        &["gitserious", "commit", "--type", "INVALID"][..],
        &["gitserious", "commit", "--type", "feat", "extra"][..],
    ] {
        let harness = Harness::new(initialized_state()?);
        let (exit, stdout, stderr) = harness.run(arguments);
        assert_eq!(exit, ExitCode::from(2));
        assert!(stdout.is_empty());
        assert!(stderr.contains("For more information, try '--help'."));
        assert_eq!(harness.catalog.calls.get(), 0);
        assert_eq!(harness.editor.calls.get(), 0);
    }
    Ok(())
}

#[test]
fn missing_project_policy_is_an_operational_error() {
    let harness = Harness::new(ProjectState::Absent);
    let (exit, stdout, stderr) = harness.run(&["gitserious", "commit", "--type", "feat"]);
    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert_eq!(
        stderr,
        "error: gitserious is not initialized; run `gitserious init` before committing\n"
    );
    assert_eq!(harness.catalog.calls.get(), 0);
}

#[test]
fn commit_help_is_stdout_only_and_does_not_touch_adapters() -> Result<(), Box<dyn Error>> {
    let harness = Harness::new(initialized_state()?);
    let (exit, stdout, stderr) = harness.run(&["gitserious", "commit", "--help"]);
    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(stdout.contains("gitserious commit [OPTIONS]"));
    assert!(stdout.contains("--type <COMMIT TYPE>"));
    assert!(stderr.is_empty());
    assert_eq!(harness.catalog.calls.get(), 0);
    Ok(())
}
