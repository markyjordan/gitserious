#![allow(clippy::expect_used)]

use std::cell::{Cell, RefCell};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use gitserious_app::{
    ConfigurationDestination, ConfigurationServices, ConfigurationWorkspace, GlobalConfiguration,
    GlobalConfigurationStore, ProjectConfiguration, ProjectConfigurationStore, ProjectRevision,
    RepositoryLocator, RepositoryRoot, StoredProject, initial_project_configuration,
};
fn root() -> RepositoryRoot {
    RepositoryRoot::new(std::env::temp_dir()).expect("temporary directory is absolute")
}

fn built_in_project() -> ProjectConfiguration {
    initial_project_configuration(&GlobalConfiguration::default())
        .expect("built-ins form a project")
}

#[derive(Debug)]
struct FakeError(&'static str);

impl Display for FakeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for FakeError {}

struct Locator;

impl RepositoryLocator for Locator {
    type Error = FakeError;

    fn is_not_repository(_: &Self::Error) -> bool {
        false
    }

    fn locate(&self, _start: &Path) -> Result<RepositoryRoot, Self::Error> {
        Ok(root())
    }
}

struct BrokenLibrary;

impl GlobalConfigurationStore for BrokenLibrary {
    type Error = FakeError;

    fn path(&self) -> PathBuf {
        PathBuf::from("/config/config.toml")
    }

    fn load(&self) -> Result<GlobalConfiguration, Self::Error> {
        Err(FakeError("the Library is corrupt"))
    }

    fn compare_and_swap(
        &self,
        _: &GlobalConfiguration,
        _: &GlobalConfiguration,
    ) -> Result<(), Self::Error> {
        Err(FakeError("unexpected global write"))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Write {
    Create(ProjectConfiguration),
    Replace {
        expected: ProjectRevision,
        to: ProjectConfiguration,
    },
}

/// One in-memory project file whose revision advances on every write.
struct Project {
    stored: RefCell<Option<StoredProject>>,
    failure: Cell<Option<&'static str>>,
    load_fail: Cell<bool>,
    attempts: Cell<usize>,
    writes: RefCell<Vec<Write>>,
}

impl Project {
    fn new(configuration: Option<ProjectConfiguration>) -> Self {
        Self {
            stored: RefCell::new(configuration.map(|configuration| StoredProject {
                configuration,
                revision: ProjectRevision::of_bytes(b"0"),
            })),
            failure: Cell::new(None),
            load_fail: Cell::new(false),
            attempts: Cell::new(0),
            writes: RefCell::default(),
        }
    }

    fn write(&self, configuration: &ProjectConfiguration) -> ProjectRevision {
        let revision = ProjectRevision::of_bytes(self.writes.borrow().len().to_string().as_bytes());
        *self.stored.borrow_mut() = Some(StoredProject {
            configuration: configuration.clone(),
            revision,
        });
        revision
    }

    fn configuration(&self) -> Option<ProjectConfiguration> {
        self.stored
            .borrow()
            .as_ref()
            .map(|stored| stored.configuration.clone())
    }
}

impl ProjectConfigurationStore for Project {
    type Error = FakeError;

    fn load(&self, _: &RepositoryRoot) -> Result<Option<StoredProject>, Self::Error> {
        if self.load_fail.get() {
            return Err(FakeError("project read failed"));
        }
        Ok(self.stored.borrow().clone())
    }

    fn create(
        &self,
        _: &RepositoryRoot,
        configuration: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error> {
        self.attempts.set(self.attempts.get() + 1);
        if let Some(failure) = self.failure.get() {
            return Err(FakeError(failure));
        }
        if self.stored.borrow().is_some() {
            return Err(FakeError("refusing to overwrite existing gitserious.toml"));
        }
        self.writes
            .borrow_mut()
            .push(Write::Create(configuration.clone()));
        Ok(self.write(configuration))
    }

    fn replace(
        &self,
        _: &RepositoryRoot,
        expected: ProjectRevision,
        configuration: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error> {
        self.attempts.set(self.attempts.get() + 1);
        if let Some(failure) = self.failure.get() {
            return Err(FakeError(failure));
        }
        if self.stored.borrow().as_ref().map(|stored| stored.revision) != Some(expected) {
            return Err(FakeError("gitserious.toml changed since it was opened"));
        }
        self.writes.borrow_mut().push(Write::Replace {
            expected,
            to: configuration.clone(),
        });
        Ok(self.write(configuration))
    }

    fn ensure_local_state(&self, _: &RepositoryRoot) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Edits the project while the Library store is unreadable, so any Library read fails the edit.
fn edit_project<F>(project: &Project, script: F) -> Result<(), String>
where
    F: Fn(&dyn ConfigurationWorkspace) -> Result<(), String>,
{
    script(&ConfigurationServices::new(
        &Locator,
        project,
        &BrokenLibrary,
        Path::new("."),
    ))
}

#[test]
fn a_project_load_failure_does_not_attempt_persistence() -> Result<(), Box<dyn Error>> {
    let project = Project::new(Some(built_in_project()));
    project.load_fail.set(true);
    edit_project(&project, |workspace| {
        assert_eq!(
            workspace
                .load(ConfigurationDestination::Project)
                .err()
                .as_deref(),
            Some("project read failed")
        );
        Ok(())
    })?;
    assert_eq!(project.attempts.get(), 0);
    assert!(project.writes.borrow().is_empty());
    assert_eq!(project.configuration(), Some(built_in_project()));
    Ok(())
}
