use std::cell::RefCell;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gitserious_app::{
    ConfigurationCatalog, ProjectConfig, ProjectLock, ProjectState, ProjectStateStore,
    RepositoryLocator, RepositoryRoot, UserConfiguration, UserConfigurationStore,
    fork_conventional,
};
use gitserious_cli::run_from;
use gitserious_core::{TaxonomyId, TemplateId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FakeError;

impl Display for FakeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("configuration store unavailable")
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

#[derive(Default)]
struct RecordingStore;

impl ProjectStateStore for RecordingStore {
    type Error = FakeError;

    fn inspect(&self, _root: &RepositoryRoot) -> Result<ProjectState, Self::Error> {
        Ok(ProjectState::Absent)
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

struct FakeUserStore {
    configuration: RefCell<UserConfiguration>,
}

impl FakeUserStore {
    fn empty() -> Self {
        Self {
            configuration: RefCell::new(UserConfiguration::default()),
        }
    }
}

impl UserConfigurationStore for FakeUserStore {
    type Error = FakeError;

    fn load(&self) -> Result<UserConfiguration, Self::Error> {
        Ok(self.configuration.borrow().clone())
    }

    fn compare_and_swap(
        &self,
        expected: &UserConfiguration,
        replacement: &UserConfiguration,
    ) -> Result<(), Self::Error> {
        if *self.configuration.borrow() != *expected {
            return Err(FakeError);
        }
        *self.configuration.borrow_mut() = replacement.clone();
        Ok(())
    }
}

fn repository_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fake-repository")
}

fn run(arguments: &[&str], configuration: &FakeUserStore) -> (ExitCode, String, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit = run_from(
        arguments.iter().copied(),
        &repository_path().join("subdir"),
        &FakeLocator,
        &RecordingStore,
        configuration,
        &mut stdout,
        &mut stderr,
    );
    (
        exit,
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    )
}

fn catalog(configuration: &FakeUserStore) -> Result<ConfigurationCatalog, Box<dyn Error>> {
    Ok(ConfigurationCatalog::new(&configuration.load()?)?)
}

#[test]
fn list_reports_built_in_definitions_across_every_kind() {
    let configuration = FakeUserStore::empty();
    let (exit, stdout, stderr) = run(&["gitserious", "config", "list"], &configuration);

    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(stderr.is_empty());
    assert!(stdout.contains("TAXONOMIES\n  conventional  built-in v1"));
    assert!(stdout.contains("TYPESETS\n  conventional/default  built-in v1"));
    assert!(stdout.contains("TEMPLATES\n  default  built-in v1"));
}

#[test]
fn list_kind_filter_renders_only_one_section() {
    let configuration = FakeUserStore::empty();
    let (exit, stdout, _) = run(
        &["gitserious", "config", "list", "template"],
        &configuration,
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(stdout.contains("TEMPLATES"));
    assert!(!stdout.contains("TAXONOMIES"));
    assert!(!stdout.contains("TYPESETS"));
}

#[test]
fn show_renders_each_entity_kind_in_detail() {
    let configuration = FakeUserStore::empty();

    let (_, stdout, stderr) = run(
        &["gitserious", "config", "show", "template", "default"],
        &configuration,
    );
    assert_eq!(stderr, "");
    assert!(stdout.starts_with("template default (built-in)\nversion: 1\n"));
    assert!(stdout.contains("selects taxonomy conventional with typeset default"));
    assert!(stdout.contains("resolves to 11 change types"));

    let (_, stdout, _) = run(
        &[
            "gitserious",
            "config",
            "show",
            "typeset",
            "conventional/default",
        ],
        &configuration,
    );
    assert!(stdout.starts_with("typeset conventional/default (built-in)\n"));
    assert!(stdout.contains("  feat\n    intent  required  single"));

    let (_, stdout, _) = run(
        &["gitserious", "config", "show", "taxonomy", "conventional"],
        &configuration,
    );
    assert!(stdout.starts_with("taxonomy conventional (built-in)\n"));
    assert!(stdout.contains("change types:\n  feat"));
}

#[test]
fn show_missing_entities_fail_on_stderr() {
    let configuration = FakeUserStore::empty();

    let (exit, stdout, stderr) = run(
        &["gitserious", "config", "show", "template", "missing"],
        &configuration,
    );
    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert_eq!(stderr, "error: template missing was not found\n");

    let (exit, _, stderr) = run(
        &["gitserious", "config", "show", "typeset", "not-qualified"],
        &configuration,
    );
    assert_eq!(exit, ExitCode::FAILURE);
    assert_eq!(
        stderr,
        "error: typeset identity must be TAXONOMY/TYPESET, found \"not-qualified\"\n"
    );
}

#[test]
fn list_and_show_include_user_forks() -> Result<(), Box<dyn Error>> {
    let configuration = FakeUserStore::empty();
    fork_conventional(
        &configuration,
        TemplateId::new("platform")?,
        TaxonomyId::new("ops")?,
        gitserious_core::TypesetId::new("baseline")?,
    )?;
    assert_eq!(catalog(&configuration)?.templates().len(), 2);

    let (exit, stdout, _) = run(&["gitserious", "config", "list"], &configuration);
    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(stdout.contains("  ops  user v1"));
    assert!(stdout.contains("  ops/baseline  user v1"));
    assert!(stdout.contains("  platform  user v1  ops / baseline"));

    let (_, _, stderr) = run(
        &["gitserious", "config", "show", "taxonomy", "ops"],
        &configuration,
    );
    assert_eq!(stderr, "");
    Ok(())
}
