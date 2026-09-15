use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

use gitserious_core::{TaxonomyId, TaxonomyVersion};

use crate::{
    ConfigurationError, GlobalConfiguration, ProjectConfig, ProjectLock, ProjectState,
    ProjectStateStore, RepositoryLocator, RepositoryRoot, resolve_effective_configuration,
};

/// State transition performed by project initialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitStatus {
    Initialized,
    LockCreated,
    LockRefreshed,
    AlreadyInitialized,
}

/// Successful initialization details.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitOutcome {
    status: InitStatus,
    root: RepositoryRoot,
    taxonomy: TaxonomyId,
    version: TaxonomyVersion,
}

impl InitOutcome {
    #[must_use]
    pub const fn status(&self) -> InitStatus {
        self.status
    }
    #[must_use]
    pub const fn root(&self) -> &RepositoryRoot {
        &self.root
    }
    #[must_use]
    pub const fn taxonomy(&self) -> &TaxonomyId {
        &self.taxonomy
    }
    #[must_use]
    pub const fn version(&self) -> TaxonomyVersion {
        self.version
    }
}

/// Failure to initialize portable project policy.
#[derive(Debug)]
pub enum InitializeProjectError<LocatorError, StoreError> {
    Repository(LocatorError),
    Store(StoreError),
    Configuration(ConfigurationError),
    OrphanLock,
}

impl<L: Display, S: Display> Display for InitializeProjectError<L, S> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repository(error) => Display::fmt(error, formatter),
            Self::Store(error) => Display::fmt(error, formatter),
            Self::Configuration(error) => Display::fmt(error, formatter),
            Self::OrphanLock => formatter.write_str(
                "gitserious.lock exists without gitserious.toml; restore or remove the orphan lock",
            ),
        }
    }
}

impl<L: Error + 'static, S: Error + 'static> Error for InitializeProjectError<L, S> {}

/// Initializes or reconciles repository-local portable policy.
pub fn initialize_project<L, S>(
    locator: &L,
    store: &S,
    global: &GlobalConfiguration,
    taxonomy: Option<&TaxonomyId>,
    start: &Path,
) -> Result<InitOutcome, InitializeProjectError<L::Error, S::Error>>
where
    L: RepositoryLocator + ?Sized,
    S: ProjectStateStore + ?Sized,
{
    let root = locator
        .locate(start)
        .map_err(InitializeProjectError::Repository)?;
    let state = store
        .inspect(&root)
        .map_err(InitializeProjectError::Store)?;
    let (initial_status, config, previous_lock) = match state {
        ProjectState::Absent => {
            let config = fresh_project_config(global, taxonomy)
                .map_err(InitializeProjectError::Configuration)?;
            (InitStatus::Initialized, config, None)
        }
        ProjectState::ConfigOnly(config) => (InitStatus::LockCreated, config, None),
        ProjectState::Initialized { config, lock } => {
            (InitStatus::AlreadyInitialized, config, Some(lock))
        }
        ProjectState::LockOnly => return Err(InitializeProjectError::OrphanLock),
    };
    let effective = resolve_effective_configuration(global, &config)
        .map_err(InitializeProjectError::Configuration)?;
    let expected =
        ProjectLock::capture(&config, &effective).map_err(InitializeProjectError::Configuration)?;
    store
        .ensure_local_state(&root)
        .map_err(InitializeProjectError::Store)?;
    let status = match (initial_status, previous_lock) {
        (InitStatus::Initialized, None) => {
            store
                .initialize(&root, &config, &expected)
                .map_err(InitializeProjectError::Store)?;
            InitStatus::Initialized
        }
        (InitStatus::LockCreated, None) => {
            store
                .create_lock(&root, &expected)
                .map_err(InitializeProjectError::Store)?;
            InitStatus::LockCreated
        }
        (InitStatus::AlreadyInitialized, Some(existing)) if existing == expected => {
            InitStatus::AlreadyInitialized
        }
        (InitStatus::AlreadyInitialized, Some(existing)) => {
            store
                .replace_lock(&root, &existing, &expected)
                .map_err(InitializeProjectError::Store)?;
            InitStatus::LockRefreshed
        }
        _ => return Err(InitializeProjectError::OrphanLock),
    };
    let selected = expected.default_taxonomy();
    let version = expected
        .taxonomies()
        .iter()
        .find(|item| item.id() == selected)
        .map(gitserious_core::Taxonomy::version)
        .ok_or_else(|| {
            InitializeProjectError::Configuration(ConfigurationError::UnknownTaxonomy(
                selected.clone(),
            ))
        })?;
    Ok(InitOutcome {
        status,
        root,
        taxonomy: selected.clone(),
        version,
    })
}

fn fresh_project_config(
    global: &GlobalConfiguration,
    requested: Option<&TaxonomyId>,
) -> Result<ProjectConfig, ConfigurationError> {
    let Some(requested) = requested else {
        return Ok(ProjectConfig::inherited());
    };
    let catalog = global.catalog()?;
    if catalog.find(requested).is_none() {
        return Err(ConfigurationError::UnknownTaxonomy(requested.clone()));
    }
    let available = if global.available_taxonomies().contains(requested) {
        None
    } else {
        let mut available = global.available_taxonomies().to_vec();
        available.push(requested.clone());
        Some(available)
    };
    ProjectConfig::new(1, Some(requested.clone()), available)
}
