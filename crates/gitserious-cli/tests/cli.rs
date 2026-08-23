use std::cell::RefCell;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gitserious_app::{
    CustomConfiguration, GlobalConfigurationStore, ProjectConfig, ProjectLock, ProjectState,
    ProjectStateStore, RepositoryLocator, RepositoryRoot,
};
use gitserious_cli::run_from;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FakeError;

impl Display for FakeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("repository unavailable")
    }
}

impl Error for FakeError {}

struct FakeLocator {
    error: bool,
}

impl RepositoryLocator for FakeLocator {
    type Error = FakeError;

    fn locate(&self, _start: &Path) -> Result<RepositoryRoot, Self::Error> {
        if self.error {
            Err(FakeError)
        } else {
            RepositoryRoot::new(repository_path()).map_err(|_| FakeError)
        }
    }
}

#[derive(Default)]
struct RecordingStore {
    initialized: RefCell<Option<(ProjectConfig, ProjectLock)>>,
}

impl ProjectStateStore for RecordingStore {
    type Error = FakeError;

    fn inspect(&self, _root: &RepositoryRoot) -> Result<ProjectState, Self::Error> {
        Ok(ProjectState::Absent)
    }

    fn ensure_local_state(&self, _root: &RepositoryRoot) -> Result<(), Self::Error> {
        Ok(())
    }

    fn initialize(
        &self,
        _root: &RepositoryRoot,
        config: &ProjectConfig,
        lock: &ProjectLock,
    ) -> Result<(), Self::Error> {
        self.initialized
            .replace(Some((config.clone(), lock.clone())));
        Ok(())
    }

    fn create_lock(&self, _root: &RepositoryRoot, _lock: &ProjectLock) -> Result<(), Self::Error> {
        Ok(())
    }

    fn replace_lock(
        &self,
        _root: &RepositoryRoot,
        _current: &ProjectLock,
        _replacement: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn compare_and_swap(
        &self,
        _root: &RepositoryRoot,
        _current_config: &ProjectConfig,
        _current_lock: &ProjectLock,
        _replacement_config: &ProjectConfig,
        _replacement_lock: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

struct FakeUserStore {
    error: bool,
}

impl GlobalConfigurationStore for FakeUserStore {
    type Error = FakeError;

    fn load(&self) -> Result<CustomConfiguration, Self::Error> {
        if self.error {
            Err(FakeError)
        } else {
            Ok(CustomConfiguration::default())
        }
    }

    fn compare_and_swap(
        &self,
        _expected: &CustomConfiguration,
        _replacement: &CustomConfiguration,
    ) -> Result<(), Self::Error> {
        Err(FakeError)
    }
}

fn repository_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fake-repository")
}

fn run(arguments: &[&str], locator: &FakeLocator) -> (ExitCode, String, String, RecordingStore) {
    let store = RecordingStore::default();
    let configuration = FakeUserStore { error: false };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit = run_from(
        arguments.iter().copied(),
        &repository_path().join("subdir"),
        locator,
        &store,
        &configuration,
        &mut stdout,
        &mut stderr,
    );
    (
        exit,
        String::from_utf8(stdout).unwrap_or_default(),
        String::from_utf8(stderr).unwrap_or_default(),
        store,
    )
}

#[test]
fn init_dispatches_and_reports_the_exact_resolution() {
    let (exit, stdout, stderr, store) = run(&["gitserious", "init"], &FakeLocator { error: false });

    assert_eq!(exit, ExitCode::SUCCESS);
    assert_eq!(
        stdout,
        format!(
            "Initialized gitserious in {} (default -> conventional@1).\n",
            repository_path().display()
        )
    );
    assert!(stderr.is_empty());
    let initialized = store.initialized.borrow();
    assert_eq!(
        initialized
            .as_ref()
            .map(|(config, _)| config.active_template().as_str()),
        Some("default")
    );
    assert_eq!(
        initialized
            .as_ref()
            .map(|(_, lock)| lock.resolved_template().commit_types().len()),
        Some(11)
    );
}

#[test]
fn default_init_does_not_load_unavailable_global_configuration() {
    let store = RecordingStore::default();
    let configuration = FakeUserStore { error: true };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit = run_from(
        ["gitserious", "init"],
        &repository_path(),
        &FakeLocator { error: false },
        &store,
        &configuration,
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(String::from_utf8_lossy(&stdout).contains("default -> conventional@1"));
    assert!(stderr.is_empty());
    assert!(store.initialized.borrow().is_some());
}

#[test]
fn operational_failures_use_stderr_and_exit_one() {
    let (exit, stdout, stderr, store) = run(&["gitserious", "init"], &FakeLocator { error: true });

    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert_eq!(stderr, "error: repository unavailable\n");
    assert!(store.initialized.borrow().is_none());
}

#[test]
fn init_selects_an_explicit_installed_template() {
    let store = RecordingStore::default();
    let configuration = FakeUserStore { error: false };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit = run_from(
        ["gitserious", "init", "--template", "default"]
            .into_iter()
            .map(std::borrow::ToOwned::to_owned),
        &repository_path().join("subdir"),
        &FakeLocator { error: false },
        &store,
        &configuration,
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(String::from_utf8_lossy(&stdout).contains("(default -> conventional@1)."));
    let initialized = store.initialized.borrow();
    assert_eq!(
        initialized
            .as_ref()
            .map(|(config, _)| config.active_template().as_str()),
        Some("default")
    );
}

#[test]
fn init_rejects_an_uninstalled_template() {
    let (exit, stdout, stderr, _) = run(
        &["gitserious", "init", "--template", "missing"],
        &FakeLocator { error: false },
    );

    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert_eq!(
        stderr,
        "error: template TemplateId(\"missing\") is not installed\n"
    );
}

#[test]
fn help_and_version_succeed_on_stdout_without_touching_adapters() {
    for (arguments, expected) in [
        (
            &["gitserious", "--help"][..],
            "Create durable commit-message policy",
        ),
        (&["gitserious", "--version"][..], "gitserious 0.1.0"),
    ] {
        let (exit, stdout, stderr, store) = run(arguments, &FakeLocator { error: true });
        assert_eq!(exit, ExitCode::SUCCESS);
        assert!(stdout.contains(expected));
        assert!(stderr.is_empty());
        assert!(store.initialized.borrow().is_none());
    }
}

#[test]
fn missing_and_unknown_commands_are_usage_errors() {
    for arguments in [
        &["gitserious"][..],
        &["gitserious", "unknown"][..],
        &["gitserious", "init", "extra"][..],
    ] {
        let (exit, stdout, stderr, store) = run(arguments, &FakeLocator { error: true });
        assert_eq!(exit, ExitCode::from(2));
        assert!(stdout.is_empty());
        assert!(stderr.contains("Usage:"));
        assert!(store.initialized.borrow().is_none());
    }
}
