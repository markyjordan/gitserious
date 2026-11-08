#![allow(clippy::expect_used)]

use std::cell::RefCell;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use gitserious_app::{
    ConfigurationDestination, ConfigurationEditor, ConfigurationSession, ConfigurationWorkspace,
    GlobalConfiguration, GlobalConfigurationStore, ProjectConfiguration, ProjectConfigurationStore,
    ProjectRevision, RepositoryLocator, RepositoryRoot, StoredProject, edit_configuration,
    initial_project_configuration,
};
use gitserious_core::{
    Taxonomy, TaxonomyId, TaxonomyLineage, TaxonomyVersion, built_in_taxonomies,
};

fn id(value: &str) -> TaxonomyId {
    TaxonomyId::new(value).expect("valid test identity")
}

fn root() -> RepositoryRoot {
    RepositoryRoot::new(std::env::temp_dir()).expect("temporary directory is absolute")
}

fn with_team() -> GlobalConfiguration {
    let mut available = GlobalConfiguration::default()
        .available_taxonomies()
        .to_vec();
    available.push(id("team"));
    let source = &built_in_taxonomies()[0];
    let team = Taxonomy::new(
        id("team"),
        Some(TaxonomyVersion::V1),
        source.description().clone(),
        Some(TaxonomyLineage::new(source.id().clone(), source.version())),
        source.commit_types().to_vec(),
    )
    .expect("valid test taxonomy");
    GlobalConfiguration::new(1, id("conventional"), available, vec![team.into()])
        .expect("valid global configuration")
}

fn built_in_project() -> ProjectConfiguration {
    initial_project_configuration(&GlobalConfiguration::default())
        .expect("built-ins form a project")
}

fn ids(configuration: &ProjectConfiguration) -> Vec<&str> {
    configuration
        .taxonomies()
        .iter()
        .map(|taxonomy| taxonomy.id().as_str())
        .collect()
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
    failure: Option<&'static str>,
    writes: RefCell<Vec<Write>>,
}

impl Project {
    fn new(configuration: Option<ProjectConfiguration>) -> Self {
        Self {
            stored: RefCell::new(configuration.map(|configuration| StoredProject {
                configuration,
                revision: ProjectRevision::of_bytes(b"0"),
            })),
            failure: None,
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

    fn external_edit(&self) {
        if let Some(stored) = self.stored.borrow_mut().as_mut() {
            stored.revision = ProjectRevision::of_bytes(b"edited elsewhere");
        }
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
        Ok(self.stored.borrow().clone())
    }

    fn create(
        &self,
        _: &RepositoryRoot,
        configuration: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error> {
        if let Some(failure) = self.failure {
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
        if let Some(failure) = self.failure {
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

struct Script<F>(F);

impl<F> ConfigurationEditor for Script<F>
where
    F: Fn(&dyn ConfigurationWorkspace) -> Result<(), String>,
{
    fn edit(&self, workspace: &dyn ConfigurationWorkspace) -> Result<(), String> {
        (self.0)(workspace)
    }
}

/// Edits the project while the Library store is unreadable, so any Library read fails the edit.
fn edit_project<F>(project: &Project, script: F) -> Result<(), String>
where
    F: Fn(&dyn ConfigurationWorkspace) -> Result<(), String>,
{
    edit_configuration(
        &Locator,
        project,
        &BrokenLibrary,
        Path::new("."),
        &Script(script),
    )
}

#[test]
fn project_sessions_open_review_and_apply_without_the_library() -> Result<(), Box<dyn Error>> {
    let project = Project::new(Some(built_in_project()));

    edit_project(&project, |workspace| {
        assert!(
            workspace.load(ConfigurationDestination::Global).is_err(),
            "the Library fixture must be unreadable"
        );
        let mut session = workspace.load(ConfigurationDestination::Project)?;
        session.configure_project(id("research"), &[id("research"), id("conventional")])?;
        session.review()?;
        workspace.save(&session)?;
        Ok(())
    })?;

    let saved = project.configuration().ok_or("project vanished")?;
    assert_eq!(saved.default_taxonomy().as_str(), "research");
    assert_eq!(ids(&saved), ["research", "conventional"]);
    Ok(())
}

#[test]
fn project_mutations_require_the_project_destination_and_initialized_state()
-> Result<(), Box<dyn Error>> {
    let mut global = ConfigurationSession::open_global(GlobalConfiguration::default());
    assert!(
        global
            .configure_project(id("conventional"), &[id("conventional")])
            .is_err()
    );

    let mut uninitialized = ConfigurationSession::open_project(root(), None);
    assert!(
        uninitialized
            .configure_project(id("conventional"), &[id("conventional")])
            .is_err()
    );
    assert!(uninitialized.review().is_err());

    uninitialized.initialize_project(&with_team())?;
    let staged = uninitialized.project_config().ok_or("nothing staged")?;
    assert_eq!(
        ids(&staged),
        ["conventional", "research", "infra-ops", "team"]
    );
    assert!(uninitialized.initialize_project(&with_team()).is_err());
    Ok(())
}

#[test]
fn project_selection_rejects_invalid_choices_without_changing_the_draft()
-> Result<(), Box<dyn Error>> {
    let mut session = ConfigurationSession::open_project(
        root(),
        Some(StoredProject {
            configuration: built_in_project(),
            revision: ProjectRevision::of_bytes(b"0"),
        }),
    );

    for (default, available) in [
        (id("team"), vec![id("conventional"), id("team")]),
        (id("research"), vec![id("conventional")]),
        (id("conventional"), Vec::new()),
        (
            id("conventional"),
            vec![id("conventional"), id("conventional")],
        ),
    ] {
        assert!(session.configure_project(default, &available).is_err());
        assert_eq!(session.project_config(), Some(built_in_project()));
        assert!(!session.is_dirty());
    }

    session.configure_project(id("conventional"), &[id("conventional")])?;
    session.configure_project(id("conventional"), &[id("conventional"), id("research")])?;
    assert_eq!(
        ids(&session.project_config().ok_or("no draft")?),
        ["conventional", "research"]
    );
    Ok(())
}

#[test]
fn first_save_creates_and_later_saves_replace_the_opened_revision() -> Result<(), Box<dyn Error>> {
    let project = Project::new(None);
    let library = with_team();

    edit_project(&project, |workspace| {
        let mut session = workspace.load(ConfigurationDestination::Project)?;
        session.initialize_project(&library)?;
        let mut session = workspace.save(&session)?;
        session.configure_project(id("team"), &[id("team")])?;
        workspace.save(&session)?;
        Ok(())
    })?;

    let initial = initial_project_configuration(&library)?;
    let writes = project.writes.borrow();
    assert_eq!(writes.len(), 2);
    assert_eq!(writes[0], Write::Create(initial));
    assert!(matches!(
        &writes[1],
        Write::Replace { to, .. } if to.default_taxonomy().as_str() == "team" && ids(to) == ["team"]
    ));
    Ok(())
}

#[test]
fn stale_or_failed_applies_leave_the_project_file_unchanged() {
    let project = Project::new(Some(built_in_project()));
    let error = edit_project(&project, |workspace| {
        let mut session = workspace.load(ConfigurationDestination::Project)?;
        session.configure_project(id("research"), &[id("research")])?;
        project.external_edit();
        workspace.save(&session).map(|_| ())
    })
    .expect_err("a stale session overwrote an external edit");
    assert!(error.contains("changed since it was opened"), "{error}");
    assert_eq!(project.configuration(), Some(built_in_project()));

    let mut failing = Project::new(Some(built_in_project()));
    failing.failure = Some("disk full");
    let error = edit_project(&failing, |workspace| {
        let mut session = workspace.load(ConfigurationDestination::Project)?;
        session.configure_project(id("research"), &[id("research")])?;
        workspace.save(&session).map(|_| ())
    })
    .expect_err("a failed write reported success");
    assert_eq!(error, "disk full");
    assert!(failing.writes.borrow().is_empty());
}
