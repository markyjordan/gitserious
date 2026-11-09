#![allow(dead_code)]

use std::cell::RefCell;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gitserious_app::{
    GlobalConfiguration, GlobalConfigurationStore, ProjectConfiguration, ProjectConfigurationStore,
    ProjectRevision, RepositoryLocator, RepositoryRoot, StoredProject,
};
use gitserious_cli::{CommitAdapters, run_from_with_commit};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FakeError(pub &'static str);

impl Display for FakeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for FakeError {}

pub fn repository_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fake-repository")
}

pub struct Locator {
    pub fail: bool,
}

impl RepositoryLocator for Locator {
    type Error = FakeError;

    fn is_not_repository(_: &Self::Error) -> bool {
        false
    }

    fn locate(&self, _start: &Path) -> Result<RepositoryRoot, Self::Error> {
        if self.fail {
            return Err(FakeError("repository unavailable"));
        }
        RepositoryRoot::new(repository_path()).map_err(|_| FakeError("invalid root"))
    }
}

pub struct Global {
    pub configuration: GlobalConfiguration,
    pub fail: bool,
}

impl Global {
    pub fn new(configuration: GlobalConfiguration) -> Self {
        Self {
            configuration,
            fail: false,
        }
    }
}

impl GlobalConfigurationStore for Global {
    type Error = FakeError;

    fn path(&self) -> PathBuf {
        PathBuf::from("/config/config.toml")
    }

    fn load(&self) -> Result<GlobalConfiguration, Self::Error> {
        if self.fail {
            return Err(FakeError("global configuration unavailable"));
        }
        Ok(self.configuration.clone())
    }

    fn compare_and_swap(
        &self,
        _: &GlobalConfiguration,
        _: &GlobalConfiguration,
    ) -> Result<(), Self::Error> {
        Err(FakeError("unexpected global write"))
    }
}

#[derive(Default)]
pub struct Project {
    pub state: RefCell<Option<ProjectConfiguration>>,
    pub initialized: RefCell<Option<ProjectConfiguration>>,
}

impl Project {
    pub fn with(state: ProjectConfiguration) -> Self {
        Self {
            state: RefCell::new(Some(state)),
            initialized: RefCell::default(),
        }
    }
}

impl ProjectConfigurationStore for Project {
    type Error = FakeError;

    fn load(&self, _: &RepositoryRoot) -> Result<Option<StoredProject>, Self::Error> {
        Ok(self
            .state
            .borrow()
            .clone()
            .map(|configuration| StoredProject {
                configuration,
                revision: ProjectRevision::of_bytes(b"project"),
            }))
    }
    fn create(
        &self,
        _: &RepositoryRoot,
        configuration: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error> {
        *self.initialized.borrow_mut() = Some(configuration.clone());
        Ok(ProjectRevision::of_bytes(b"created"))
    }
    fn replace(
        &self,
        _: &RepositoryRoot,
        _: ProjectRevision,
        _: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error> {
        Err(FakeError("unexpected project write"))
    }
    fn ensure_local_state(&self, _: &RepositoryRoot) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub struct Harness {
    pub locator: Locator,
    pub project: Project,
    pub global: Global,
}

impl Harness {
    pub fn new(project: Project) -> Self {
        Self {
            locator: Locator { fail: false },
            project,
            global: Global::new(GlobalConfiguration::default()),
        }
    }

    pub fn run(&self, arguments: &[&str]) -> (ExitCode, String, String) {
        self.run_with(arguments, &CommitAdapters::new())
    }

    pub fn run_with(
        &self,
        arguments: &[&str],
        adapters: &CommitAdapters<'_>,
    ) -> (ExitCode, String, String) {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit = run_from_with_commit(
            arguments.iter().copied(),
            &repository_path().join("subdir"),
            &self.locator,
            &self.project,
            &self.global,
            adapters,
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
