use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

use gitserious_core::{TaxonomyId, TaxonomyVersion};

use crate::{
    ConfigurationError, GlobalConfiguration, ProjectConfiguration, ProjectConfigurationStore,
    RepositoryLocator, RepositoryRoot,
};

/// State transition performed by project initialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitStatus {
    Initialized,
    AlreadyInitialized,
}

/// Successful initialization details.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitOutcome {
    status: InitStatus,
    root: RepositoryRoot,
    taxonomy: TaxonomyId,
    version: Option<TaxonomyVersion>,
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
    pub const fn version(&self) -> Option<TaxonomyVersion> {
        self.version
    }
}

/// Failure to initialize project policy.
#[derive(Debug)]
pub enum InitializeProjectError<LocatorError, StoreError> {
    Repository(LocatorError),
    Store(StoreError),
    Configuration(ConfigurationError),
}

impl<L: Display, S: Display> Display for InitializeProjectError<L, S> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repository(error) => Display::fmt(error, formatter),
            Self::Store(error) => Display::fmt(error, formatter),
            Self::Configuration(error) => Display::fmt(error, formatter),
        }
    }
}

impl<L: Error + 'static, S: Error + 'static> Error for InitializeProjectError<L, S> {}

/// Creates the project configuration once; an existing project is never rewritten.
pub fn initialize_project<L, S>(
    locator: &L,
    store: &S,
    global: &GlobalConfiguration,
    start: &Path,
) -> Result<InitOutcome, InitializeProjectError<L::Error, S::Error>>
where
    L: RepositoryLocator + ?Sized,
    S: ProjectConfigurationStore + ?Sized,
{
    let root = locator
        .locate(start)
        .map_err(InitializeProjectError::Repository)?;
    let existing = store.load(&root).map_err(InitializeProjectError::Store)?;
    let (status, configuration) = if let Some(stored) = existing {
        store
            .ensure_local_state(&root)
            .map_err(InitializeProjectError::Store)?;
        (InitStatus::AlreadyInitialized, stored.configuration)
    } else {
        let configuration =
            initial_project_configuration(global).map_err(InitializeProjectError::Configuration)?;
        store
            .ensure_local_state(&root)
            .map_err(InitializeProjectError::Store)?;
        store
            .create(&root, &configuration)
            .map_err(InitializeProjectError::Store)?;
        (InitStatus::Initialized, configuration)
    };
    let selected = configuration.default_taxonomy();
    let version = configuration
        .find(selected)
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

/// Copies the Library's default environment into a new project.
pub fn initial_project_configuration(
    global: &GlobalConfiguration,
) -> Result<ProjectConfiguration, ConfigurationError> {
    let catalog = global.catalog()?;
    let taxonomies = global
        .available_taxonomies()
        .iter()
        .map(|id| {
            catalog
                .find(id)
                .cloned()
                .ok_or_else(|| ConfigurationError::UnknownTaxonomy(id.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    ProjectConfiguration::new(1, global.default_taxonomy().clone(), taxonomies)
}
