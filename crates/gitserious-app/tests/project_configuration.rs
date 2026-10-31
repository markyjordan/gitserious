#![allow(clippy::expect_used)]

use std::cell::RefCell;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

use gitserious_app::{
    ConfigurationDestination, ConfigurationEditor, ConfigurationSession, ConfigurationWorkspace,
    GlobalConfiguration, GlobalConfigurationStore, ProjectConfig, ProjectLock, ProjectState,
    ProjectStateStore, RepositoryLocator, RepositoryRoot, edit_configuration,
    resolve_effective_configuration,
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
        TaxonomyVersion::V1,
        source.description().clone(),
        Some(TaxonomyLineage::new(source.id().clone(), source.version())),
        source.commit_types().to_vec(),
    )
    .expect("valid test taxonomy");
    GlobalConfiguration::new(1, id("conventional"), available, vec![team])
        .expect("valid global configuration")
}

fn initialized(global: &GlobalConfiguration, config: ProjectConfig) -> ProjectState {
    let effective = resolve_effective_configuration(global, &config).expect("resolvable config");
    let lock = ProjectLock::capture(&config, &effective).expect("capturable lock");
    ProjectState::Initialized { config, lock }
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

    fn locate(&self, _start: &Path) -> Result<RepositoryRoot, Self::Error> {
        Ok(root())
    }
}

struct Global(GlobalConfiguration);

impl GlobalConfigurationStore for Global {
    type Error = FakeError;

    fn load(&self) -> Result<GlobalConfiguration, Self::Error> {
        Ok(self.0.clone())
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
    Initialize(ProjectConfig, ProjectLock),
    Swap {
        from: (ProjectConfig, ProjectLock),
        to: (ProjectConfig, ProjectLock),
    },
}

struct Project {
    state: RefCell<ProjectState>,
    failure: Option<&'static str>,
    writes: RefCell<Vec<Write>>,
}

impl Project {
    fn new(state: ProjectState) -> Self {
        Self {
            state: RefCell::new(state),
            failure: None,
            writes: RefCell::default(),
        }
    }
}

impl ProjectStateStore for Project {
    type Error = FakeError;

    fn inspect(&self, _: &RepositoryRoot) -> Result<ProjectState, Self::Error> {
        Ok(self.state.borrow().clone())
    }
    fn ensure_local_state(&self, _: &RepositoryRoot) -> Result<(), Self::Error> {
        Ok(())
    }
    fn initialize(
        &self,
        _: &RepositoryRoot,
        config: &ProjectConfig,
        lock: &ProjectLock,
    ) -> Result<(), Self::Error> {
        if let Some(failure) = self.failure {
            return Err(FakeError(failure));
        }
        self.writes
            .borrow_mut()
            .push(Write::Initialize(config.clone(), lock.clone()));
        *self.state.borrow_mut() = ProjectState::Initialized {
            config: config.clone(),
            lock: lock.clone(),
        };
        Ok(())
    }
    fn create_lock(&self, _: &RepositoryRoot, _: &ProjectLock) -> Result<(), Self::Error> {
        Err(FakeError("unexpected create lock"))
    }
    fn replace_lock(
        &self,
        _: &RepositoryRoot,
        _: &ProjectLock,
        _: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Err(FakeError("unexpected replace lock"))
    }
    fn compare_and_swap(
        &self,
        _: &RepositoryRoot,
        current_config: &ProjectConfig,
        current_lock: &ProjectLock,
        replacement_config: &ProjectConfig,
        replacement_lock: &ProjectLock,
    ) -> Result<(), Self::Error> {
        if let Some(failure) = self.failure {
            return Err(FakeError(failure));
        }
        let observed = ProjectState::Initialized {
            config: current_config.clone(),
            lock: current_lock.clone(),
        };
        if *self.state.borrow() != observed {
            return Err(FakeError("project policy changed while editing"));
        }
        self.writes.borrow_mut().push(Write::Swap {
            from: (current_config.clone(), current_lock.clone()),
            to: (replacement_config.clone(), replacement_lock.clone()),
        });
        *self.state.borrow_mut() = ProjectState::Initialized {
            config: replacement_config.clone(),
            lock: replacement_lock.clone(),
        };
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

fn edit_project<F>(global: GlobalConfiguration, project: &Project, script: F) -> Result<(), String>
where
    F: Fn(&dyn ConfigurationWorkspace) -> Result<(), String>,
{
    edit_configuration(
        &Locator,
        project,
        &Global(global),
        Path::new("."),
        &Script(script),
    )
}

#[test]
fn project_mutations_require_the_project_destination_and_initialized_state()
-> Result<(), Box<dyn Error>> {
    let mut global = ConfigurationSession::open_global(GlobalConfiguration::default());
    assert!(global.configure_project(None, None).is_err());
    assert!(global.initialize_project().is_err());

    assert!(
        ConfigurationSession::open_project(
            root(),
            GlobalConfiguration::default(),
            ProjectState::LockOnly
        )
        .expect_err("an orphan lock opened")
        .contains("orphan lock")
    );

    let mut absent = ConfigurationSession::open_project(
        root(),
        GlobalConfiguration::default(),
        ProjectState::Absent,
    )?;
    assert!(absent.review().is_err());
    absent.initialize_project()?;
    assert!(absent.initialize_project().is_err());
    assert!(absent.review()?.changes.contains("PROJECT CONFIGURATION"));
    Ok(())
}

#[test]
fn project_selection_rejects_unknown_taxonomies_without_changing_the_draft()
-> Result<(), Box<dyn Error>> {
    let global = GlobalConfiguration::default();
    let mut session = ConfigurationSession::open_project(
        root(),
        global.clone(),
        initialized(&global, ProjectConfig::inherited()),
    )?;

    assert!(session.configure_project(Some(id("team")), None).is_err());
    assert!(
        session
            .configure_project(None, Some(vec![id("research"), id("missing")]))
            .is_err()
    );
    assert!(
        session
            .configure_project(Some(id("conventional")), Some(vec![id("research")]))
            .is_err()
    );

    assert_eq!(session.project_config(), Some(&ProjectConfig::inherited()));
    assert!(!session.is_dirty());
    session.configure_project(Some(id("research")), Some(vec![id("research")]))?;
    assert!(session.is_dirty());
    Ok(())
}

#[test]
fn first_save_initializes_and_later_saves_compare_and_swap() -> Result<(), Box<dyn Error>> {
    let project = Project::new(ProjectState::Absent);
    edit_project(GlobalConfiguration::default(), &project, |workspace| {
        let mut session = workspace.load(ConfigurationDestination::Project)?;
        session.initialize_project()?;
        let mut saved = workspace.save(&session)?;
        assert!(!saved.is_dirty());
        saved.configure_project(Some(id("infra-ops")), None)?;
        workspace.save(&saved).map(|_| ())
    })?;

    let writes = project.writes.borrow();
    assert!(matches!(
        writes.as_slice(),
        [Write::Initialize(..), Write::Swap { .. }]
    ));
    let Write::Swap {
        to: (config, lock), ..
    } = &writes[1]
    else {
        return Err("second save did not swap".into());
    };
    assert_eq!(config.default_taxonomy(), Some(&id("infra-ops")));
    assert_eq!(lock.default_taxonomy(), &id("infra-ops"));
    assert!(lock.matches(config));
    Ok(())
}

#[test]
fn saved_project_lock_captures_the_session_global_snapshot() -> Result<(), Box<dyn Error>> {
    let project = Project::new(ProjectState::Absent);
    edit_project(with_team(), &project, |workspace| {
        let mut session = workspace.load(ConfigurationDestination::Project)?;
        session.initialize_project()?;
        workspace.save(&session).map(|_| ())
    })?;

    let ProjectState::Initialized { lock, .. } = &*project.state.borrow() else {
        return Err("project was not initialized".into());
    };
    assert_eq!(
        lock.taxonomies().last().map(gitserious_core::Taxonomy::id),
        Some(&id("team"))
    );
    let reopened =
        ConfigurationSession::open_project(root(), with_team(), project.state.borrow().clone())?;
    assert!(!reopened.update_available());
    assert!(!reopened.is_dirty());
    Ok(())
}

#[test]
fn update_available_reports_global_changes_that_would_change_the_lock() -> Result<(), Box<dyn Error>>
{
    let pinned = initialized(&GlobalConfiguration::default(), ProjectConfig::inherited());

    let unchanged =
        ConfigurationSession::open_project(root(), GlobalConfiguration::default(), pinned.clone())?;
    let changed = ConfigurationSession::open_project(root(), with_team(), pinned)?;

    assert!(!unchanged.update_available());
    assert!(changed.update_available());
    assert_eq!(changed.source_error(), None);
    Ok(())
}

#[test]
fn project_store_conflicts_and_failures_leave_the_project_unchanged() -> Result<(), Box<dyn Error>>
{
    let global = GlobalConfiguration::default();
    let original = initialized(&global, ProjectConfig::inherited());
    let concurrent = initialized(&global, ProjectConfig::new(1, Some(id("research")), None)?);

    let project = Project::new(original.clone());
    let error = edit_project(global.clone(), &project, |workspace| {
        let mut session = workspace.load(ConfigurationDestination::Project)?;
        *project.state.borrow_mut() = concurrent.clone();
        session.configure_project(Some(id("infra-ops")), None)?;
        workspace.save(&session).map(|_| ())
    })
    .expect_err("a stale project session overwrote newer policy");
    assert!(error.contains("changed while editing"), "{error}");
    assert_eq!(*project.state.borrow(), concurrent);

    let mut failing = Project::new(original.clone());
    failing.failure = Some("project store failed");
    let error = edit_project(global, &failing, |workspace| {
        let mut session = workspace.load(ConfigurationDestination::Project)?;
        session.configure_project(Some(id("infra-ops")), None)?;
        workspace.save(&session).map(|_| ())
    })
    .expect_err("a failed save was reported as success");
    assert_eq!(error, "project store failed");
    assert_eq!(*failing.state.borrow(), original);
    assert!(failing.writes.borrow().is_empty());
    Ok(())
}

#[test]
fn unresolvable_project_policy_never_reaches_the_project_store() -> Result<(), Box<dyn Error>> {
    let pinned = initialized(&with_team(), ProjectConfig::new(1, Some(id("team")), None)?);
    let project = Project::new(pinned.clone());

    let error = edit_project(GlobalConfiguration::default(), &project, |workspace| {
        let session = workspace.load(ConfigurationDestination::Project)?;
        assert!(session.source_error().is_some());
        workspace.save(&session).map(|_| ())
    })
    .expect_err("policy naming an uninstalled taxonomy was saved");

    assert!(error.contains("team"), "{error}");
    assert!(project.writes.borrow().is_empty());
    assert_eq!(*project.state.borrow(), pinned);
    Ok(())
}
