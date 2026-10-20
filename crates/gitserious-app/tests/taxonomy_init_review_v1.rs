use std::cell::RefCell;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

use gitserious_app::{
    ConfigurationSession, GlobalConfiguration, InitStatus, InitializeProjectError, ProjectConfig,
    ProjectLock, ProjectState, ProjectStateStore, RepositoryLocator, RepositoryRoot,
    initialize_project, resolve_effective_configuration,
};
use gitserious_core::TaxonomyId;

const STALE_LOCK_REPAIR: &str = "project lock differs from the current library; run `gitserious config`, open Project, and review Apply/Refresh";

#[derive(Debug)]
struct StoreError(&'static str);

impl Display for StoreError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for StoreError {}

struct Locator(RepositoryRoot);

impl RepositoryLocator for Locator {
    type Error = StoreError;

    fn locate(&self, _: &Path) -> Result<RepositoryRoot, Self::Error> {
        Ok(self.0.clone())
    }
}

struct Memory(RefCell<ProjectState>);

impl Memory {
    fn new(state: ProjectState) -> Self {
        Self(RefCell::new(state))
    }

    fn snapshot(&self) -> ProjectState {
        self.0.borrow().clone()
    }
}

impl ProjectStateStore for Memory {
    type Error = StoreError;

    fn inspect(&self, _: &RepositoryRoot) -> Result<ProjectState, Self::Error> {
        Ok(self.snapshot())
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
        let mut state = self.0.borrow_mut();
        if !matches!(*state, ProjectState::Absent) {
            return Err(StoreError("initialize"));
        }
        *state = ProjectState::Initialized {
            config: config.clone(),
            lock: lock.clone(),
        };
        Ok(())
    }

    fn create_lock(&self, _: &RepositoryRoot, lock: &ProjectLock) -> Result<(), Self::Error> {
        let mut state = self.0.borrow_mut();
        let ProjectState::ConfigOnly(config) = &*state else {
            return Err(StoreError("create_lock"));
        };
        let config = config.clone();
        *state = ProjectState::Initialized {
            config,
            lock: lock.clone(),
        };
        Ok(())
    }

    fn replace_lock(
        &self,
        _: &RepositoryRoot,
        current: &ProjectLock,
        replacement: &ProjectLock,
    ) -> Result<(), Self::Error> {
        let mut state = self.0.borrow_mut();
        let ProjectState::Initialized { config, lock } = &*state else {
            return Err(StoreError("replace_lock"));
        };
        if lock != current {
            return Err(StoreError("replace_lock"));
        }
        let config = config.clone();
        *state = ProjectState::Initialized {
            config,
            lock: replacement.clone(),
        };
        Ok(())
    }

    fn compare_and_swap(
        &self,
        _: &RepositoryRoot,
        _: &ProjectConfig,
        _: &ProjectLock,
        _: &ProjectConfig,
        _: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Err(StoreError("compare_and_swap"))
    }
}

fn root() -> Result<RepositoryRoot, Box<dyn Error>> {
    Ok(RepositoryRoot::new(std::env::temp_dir())?)
}

fn captured(
    global: &GlobalConfiguration,
    config: &ProjectConfig,
) -> Result<ProjectLock, Box<dyn Error>> {
    let effective = resolve_effective_configuration(global, config)?;
    Ok(ProjectLock::capture(config, &effective)?)
}

fn conventional_only() -> Result<GlobalConfiguration, Box<dyn Error>> {
    Ok(GlobalConfiguration::new(
        1,
        TaxonomyId::new("conventional")?,
        vec![TaxonomyId::new("conventional")?],
        Vec::new(),
    )?)
}

#[test]
fn fresh_init_creates_config_and_lock() -> Result<(), Box<dyn Error>> {
    let global = GlobalConfiguration::default();
    let store = Memory::new(ProjectState::Absent);
    let outcome = initialize_project(&Locator(root()?), &store, &global, None, Path::new("."))?;
    assert_eq!(outcome.status(), InitStatus::Initialized);
    let config = ProjectConfig::inherited();
    assert_eq!(
        store.snapshot(),
        ProjectState::Initialized {
            config: config.clone(),
            lock: captured(&global, &config)?,
        }
    );
    Ok(())
}

#[test]
fn fresh_init_honors_a_requested_taxonomy() -> Result<(), Box<dyn Error>> {
    let global = GlobalConfiguration::default();
    let requested = TaxonomyId::new("research")?;
    let store = Memory::new(ProjectState::Absent);
    let outcome = initialize_project(
        &Locator(root()?),
        &store,
        &global,
        Some(&requested),
        Path::new("."),
    )?;
    assert_eq!(outcome.status(), InitStatus::Initialized);
    assert_eq!(outcome.taxonomy(), &requested);
    Ok(())
}

#[test]
fn missing_lock_creates_the_captured_lock() -> Result<(), Box<dyn Error>> {
    let global = GlobalConfiguration::default();
    let config = ProjectConfig::inherited();
    let store = Memory::new(ProjectState::ConfigOnly(config.clone()));
    let outcome = initialize_project(&Locator(root()?), &store, &global, None, Path::new("."))?;
    assert_eq!(outcome.status(), InitStatus::LockCreated);
    assert_eq!(
        store.snapshot(),
        ProjectState::Initialized {
            lock: captured(&global, &config)?,
            config,
        }
    );
    Ok(())
}

#[test]
fn matching_lock_stays_already_initialized() -> Result<(), Box<dyn Error>> {
    let global = GlobalConfiguration::default();
    let config = ProjectConfig::inherited();
    let lock = captured(&global, &config)?;
    let store = Memory::new(ProjectState::Initialized {
        config: config.clone(),
        lock: lock.clone(),
    });
    let outcome = initialize_project(&Locator(root()?), &store, &global, None, Path::new("."))?;
    assert_eq!(outcome.status(), InitStatus::AlreadyInitialized);
    assert_eq!(store.snapshot(), ProjectState::Initialized { config, lock });
    Ok(())
}

#[test]
fn stale_lock_after_global_change_refuses_silent_replace() -> Result<(), Box<dyn Error>> {
    let wide = GlobalConfiguration::default();
    let narrowed = conventional_only()?;
    let config = ProjectConfig::inherited();
    let wide_lock = captured(&wide, &config)?;
    let store = Memory::new(ProjectState::Initialized {
        config: config.clone(),
        lock: wide_lock.clone(),
    });
    let Err(error) = initialize_project(&Locator(root()?), &store, &narrowed, None, Path::new("."))
    else {
        return Err("init replaced a differing lock".into());
    };
    let message = error.to_string();
    if !matches!(error, InitializeProjectError::StaleLock) {
        return Err(format!("expected a stale lock, got {message}").into());
    }
    assert_eq!(message, STALE_LOCK_REPAIR);
    let state = store.snapshot();
    assert_eq!(
        state,
        ProjectState::Initialized {
            config: config.clone(),
            lock: wide_lock,
        }
    );
    let session = ConfigurationSession::open_project(root()?, narrowed, state)?;
    assert!(session.update_available());
    Ok(())
}
