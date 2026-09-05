use std::cell::RefCell;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gitserious_app::{
    ConfigurationCatalog, CustomConfiguration, GlobalConfigurationStore, ProjectConfig,
    ProjectLock, ProjectState, ProjectStateStore, RepositoryLocator, RepositoryRoot,
    fork_conventional,
};
use gitserious_cli::run_from;
use gitserious_core::{TaxonomyId, TemplateId, built_in_configuration};

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

    fn compare_and_swap(
        &self,
        _root: &RepositoryRoot,
        _current_config: &ProjectConfig,
        _current_lock: &ProjectLock,
        _replacement_config: &ProjectConfig,
        _replacement_lock: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Err(FakeError)
    }
}

struct FakeUserStore {
    configuration: RefCell<CustomConfiguration>,
}

impl FakeUserStore {
    fn empty() -> Self {
        Self {
            configuration: RefCell::new(CustomConfiguration::default()),
        }
    }
}

impl GlobalConfigurationStore for FakeUserStore {
    type Error = FakeError;

    fn load(&self) -> Result<CustomConfiguration, Self::Error> {
        Ok(self.configuration.borrow().clone())
    }

    fn compare_and_swap(
        &self,
        expected: &CustomConfiguration,
        replacement: &CustomConfiguration,
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
    assert_eq!(
        catalog(&configuration)?.templates().len(),
        built_in_configuration().templates().len() + 1
    );

    let (exit, stdout, _) = run(&["gitserious", "config", "list"], &configuration);
    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(stdout.contains("  ops  custom v1"));
    assert!(stdout.contains("  ops/baseline  custom v1"));
    assert!(stdout.contains("  platform  custom v1  ops / baseline"));

    let (_, _, stderr) = run(
        &["gitserious", "config", "show", "taxonomy", "ops"],
        &configuration,
    );
    assert_eq!(stderr, "");
    Ok(())
}

#[test]
fn fork_derives_sibling_identities_and_persists_definitions() -> Result<(), Box<dyn Error>> {
    let configuration = FakeUserStore::empty();
    let (exit, stdout, stderr) = run(
        &["gitserious", "config", "fork", "--template", "platform"],
        &configuration,
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    assert_eq!(stderr, "");
    assert_eq!(
        stdout,
        "Forked conventional into template platform \
         (taxonomy platform-taxonomy, typeset platform-typeset).\n"
    );
    assert_eq!(
        catalog(&configuration)?.templates().len(),
        built_in_configuration().templates().len() + 1
    );
    let (_, _, stderr) = run(
        &[
            "gitserious",
            "config",
            "show",
            "typeset",
            "platform-taxonomy/platform-typeset",
        ],
        &configuration,
    );
    assert_eq!(stderr, "");
    Ok(())
}

#[test]
fn fork_rejects_duplicate_identity_with_a_failure_exit() {
    let configuration = FakeUserStore::empty();
    let _ = run(
        &["gitserious", "config", "fork", "--template", "platform"],
        &configuration,
    );

    let (exit, stdout, stderr) = run(
        &["gitserious", "config", "fork", "--template", "platform"],
        &configuration,
    );
    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert_eq!(
        stderr,
        "error: taxonomy TaxonomyId(\"platform-taxonomy\") already exists\n"
    );
}

#[test]
fn delete_removes_user_definitions_and_enforces_references() -> Result<(), Box<dyn Error>> {
    let configuration = FakeUserStore::empty();
    let _ = run(
        &["gitserious", "config", "fork", "--template", "platform"],
        &configuration,
    );

    let (exit, _, stderr) = run(
        &[
            "gitserious",
            "config",
            "delete",
            "taxonomy",
            "platform-taxonomy",
        ],
        &configuration,
    );
    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stderr.contains("referenced by typeset"));

    for arguments in [
        &["gitserious", "config", "delete", "template", "platform"][..],
        &[
            "gitserious",
            "config",
            "delete",
            "typeset",
            "platform-taxonomy/platform-typeset",
        ],
        &[
            "gitserious",
            "config",
            "delete",
            "taxonomy",
            "platform-taxonomy",
        ],
    ] {
        let (exit, stdout, stderr) = run(arguments, &configuration);
        assert_eq!(exit, ExitCode::SUCCESS, "deletion failed: {stderr}");
        assert!(!stdout.is_empty());
    }
    assert_eq!(
        catalog(&configuration)?.templates().len(),
        built_in_configuration().templates().len()
    );

    let (exit, _, stderr) = run(
        &["gitserious", "config", "delete", "template", "default"],
        &configuration,
    );
    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stderr.contains("is reserved by gitserious"));
    Ok(())
}

#[test]
fn domain_bundles_are_inspectable_and_cannot_be_deleted() -> Result<(), Box<dyn Error>> {
    let configuration = FakeUserStore::empty();
    for domain in ["ml-research"] {
        for (kind, identity) in [
            ("taxonomy", domain.to_owned()),
            ("typeset", format!("{domain}/default")),
            ("template", domain.to_owned()),
        ] {
            let (exit, listing, stderr) =
                run(&["gitserious", "config", "list", kind], &configuration);
            assert_eq!(exit, ExitCode::SUCCESS);
            assert!(stderr.is_empty());
            assert!(listing.contains(&format!("  {identity}  built-in v1")));
            let (exit, shown, stderr) = run(
                &["gitserious", "config", "show", kind, &identity],
                &configuration,
            );
            assert_eq!(exit, ExitCode::SUCCESS);
            assert!(stderr.is_empty());
            assert!(shown.starts_with(&format!("{kind} {identity} (built-in)\n")));
            let (exit, stdout, stderr) = run(
                &["gitserious", "config", "delete", kind, &identity],
                &configuration,
            );
            assert_eq!(exit, ExitCode::FAILURE);
            assert!(stdout.is_empty());
            assert!(stderr.contains("reserved by gitserious"));
        }
    }
    assert_eq!(configuration.load()?, CustomConfiguration::default());
    Ok(())
}
