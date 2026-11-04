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
    taxonomy: Option<&TaxonomyId>,
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
        let configuration = initial_project_configuration(global, taxonomy)
            .map_err(InitializeProjectError::Configuration)?;
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

/// The project a new repository starts with: every built-in taxonomy, the first as default.
#[must_use]
pub fn built_in_project_configuration() -> ProjectConfiguration {
    let taxonomies = gitserious_core::built_in_taxonomies().to_vec();
    ProjectConfiguration::new(1, taxonomies[0].id().clone(), taxonomies)
        .unwrap_or_else(|_| unreachable!("the built-in taxonomies form a valid project"))
}

/// Copies the Library's default environment, plus `requested`, into a new project.
pub fn initial_project_configuration(
    global: &GlobalConfiguration,
    requested: Option<&TaxonomyId>,
) -> Result<ProjectConfiguration, ConfigurationError> {
    let catalog = global.catalog()?;
    let default = requested.unwrap_or_else(|| global.default_taxonomy());
    let mut ids = global.available_taxonomies().to_vec();
    if !ids.contains(default) {
        ids.push(default.clone());
    }
    let taxonomies = ids
        .iter()
        .map(|id| {
            catalog
                .find(id)
                .cloned()
                .ok_or_else(|| ConfigurationError::UnknownTaxonomy(id.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    ProjectConfiguration::new(1, default.clone(), taxonomies)
}
