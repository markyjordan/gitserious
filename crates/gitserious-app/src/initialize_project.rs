use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

use gitserious_core::{IdentifierError, TemplateId, TemplateVersion};

use crate::{
    ConfigurationCatalog, ConfigurationCatalogError, ConfigurationOrigin, CustomConfiguration,
    CustomConfigurationError, PROJECT_CONFIG_VERSION, ProjectConfig, ProjectConfigError,
    ProjectState, ProjectStateStore, RepositoryLocator, RepositoryRoot, ResolveProjectPolicyError,
    resolve_project_lock, taxonomy_origin, template_origin, typeset_origin,
};

/// The state transition performed by project initialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitStatus {
    /// Authored configuration and its lock were created.
    Initialized,
    /// A missing lock was created for existing authored configuration.
    LockCreated,
    /// A stale recognized lock was replaced.
    LockRefreshed,
    /// Existing authored and generated state already matched.
    AlreadyInitialized,
}

/// Successful project-initialization details for presentation adapters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitOutcome {
    status: InitStatus,
    root: RepositoryRoot,
    template_reference: TemplateId,
    resolved_template: TemplateId,
    resolved_version: TemplateVersion,
}

impl InitOutcome {
    /// Returns the state transition that occurred.
    #[must_use]
    pub const fn status(&self) -> InitStatus {
        self.status
    }

    /// Returns the initialized worktree root.
    #[must_use]
    pub const fn root(&self) -> &RepositoryRoot {
        &self.root
    }

    /// Returns the authored template channel.
    #[must_use]
    pub const fn template_reference(&self) -> &TemplateId {
        &self.template_reference
    }

    /// Returns the concrete template ID.
    #[must_use]
    pub const fn resolved_template(&self) -> &TemplateId {
        &self.resolved_template
    }

    /// Returns the concrete template version.
    #[must_use]
    pub const fn resolved_version(&self) -> TemplateVersion {
        self.resolved_version
    }
}

/// Failure to initialize repository-local project policy.
#[derive(Debug)]
pub enum InitializeProjectError<LocatorError, StoreError> {
    /// Repository discovery failed.
    Repository(LocatorError),
    /// Repository-local state inspection or persistence failed.
    Store(StoreError),
    /// The built-in default channel identifier violated core rules.
    InvalidDefaultReference(IdentifierError),
    /// The requested fresh-policy template violated configuration rules.
    InvalidTemplate(ProjectConfigError),
    /// Selected custom definitions could not form a project snapshot.
    InvalidCustom(CustomConfigurationError),
    /// Authored policy could not be resolved.
    Policy(ResolveProjectPolicyError),
    /// A generated lock exists without authored configuration.
    OrphanLock,
}

impl<LocatorError, StoreError> Display for InitializeProjectError<LocatorError, StoreError>
where
    LocatorError: Display,
    StoreError: Display,
{
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repository(error) => Display::fmt(error, formatter),
            Self::Store(error) => Display::fmt(error, formatter),
            Self::InvalidDefaultReference(error) => Display::fmt(error, formatter),
            Self::InvalidTemplate(error) => Display::fmt(error, formatter),
            Self::InvalidCustom(error) => Display::fmt(error, formatter),
            Self::Policy(error) => Display::fmt(error, formatter),
            Self::OrphanLock => formatter.write_str(
                "gitserious.lock exists without gitserious.toml; restore or remove the orphan lock",
            ),
        }
    }
}

impl<LocatorError, StoreError> Error for InitializeProjectError<LocatorError, StoreError>
where
    LocatorError: Error + 'static,
    StoreError: Error + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Repository(error) => Some(error),
            Self::Store(error) => Some(error),
            Self::InvalidDefaultReference(error) => Some(error),
            Self::InvalidTemplate(error) => Some(error),
            Self::InvalidCustom(error) => Some(error),
            Self::Policy(error) => Some(error),
            Self::OrphanLock => None,
        }
    }
}

/// Initializes or safely reconciles repository-local project policy.
///
/// Fresh policy selects the authored `template` when supplied and the built-in
/// default channel otherwise; existing authored configuration is preserved.
///
/// # Errors
///
/// Returns [`InitializeProjectError`] when repository discovery, state access,
/// policy resolution, or a safe state transition fails.
pub fn initialize_project<L, S>(
    locator: &L,
    store: &S,
    catalog: &ConfigurationCatalog,
    template: Option<&TemplateId>,
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

    let (status, config, existing_lock) = match state {
        ProjectState::Absent => {
            let config = match template {
                Some(template) => project_config_from_catalog(catalog, template)?,
                None => ProjectConfig::default_channel()
                    .map_err(InitializeProjectError::InvalidDefaultReference)?,
            };
            (InitStatus::Initialized, config, None)
        }
        ProjectState::ConfigOnly(config) => (InitStatus::LockCreated, config, None),
        ProjectState::Initialized { config, lock } => {
            (InitStatus::AlreadyInitialized, config, Some(lock))
        }
        ProjectState::LockOnly => return Err(InitializeProjectError::OrphanLock),
    };

    let expected_lock = resolve_project_lock(&config).map_err(InitializeProjectError::Policy)?;
    store
        .ensure_local_state(&root)
        .map_err(InitializeProjectError::Store)?;
    let status = match (status, existing_lock) {
        (InitStatus::Initialized, None) => {
            store
                .initialize(&root, &config, &expected_lock)
                .map_err(InitializeProjectError::Store)?;
            InitStatus::Initialized
        }
        (InitStatus::LockCreated, None) => {
            store
                .create_lock(&root, &expected_lock)
                .map_err(InitializeProjectError::Store)?;
            InitStatus::LockCreated
        }
        (InitStatus::AlreadyInitialized, Some(existing)) if existing == expected_lock => {
            InitStatus::AlreadyInitialized
        }
        (InitStatus::AlreadyInitialized, Some(existing)) => {
            store
                .replace_lock(&root, &existing, &expected_lock)
                .map_err(InitializeProjectError::Store)?;
            InitStatus::LockRefreshed
        }
        _ => return Err(InitializeProjectError::OrphanLock),
    };

    let resolved = expected_lock.resolved_template();
    Ok(InitOutcome {
        status,
        root,
        template_reference: expected_lock.template_reference().clone(),
        resolved_template: resolved.id().clone(),
        resolved_version: resolved.version(),
    })
}

fn project_config_from_catalog<LocatorError, StoreError>(
    catalog: &ConfigurationCatalog,
    selected: &TemplateId,
) -> Result<ProjectConfig, InitializeProjectError<LocatorError, StoreError>> {
    catalog.resolve(selected).map_err(|error| {
        InitializeProjectError::Policy(match error {
            ConfigurationCatalogError::UnknownTemplate(id) => {
                ResolveProjectPolicyError::UnknownTemplate(id)
            }
            other => ResolveProjectPolicyError::Catalog(other),
        })
    })?;
    let template = catalog.find_template(selected).ok_or_else(|| {
        InitializeProjectError::Policy(ResolveProjectPolicyError::UnknownTemplate(selected.clone()))
    })?;
    let taxonomy = catalog.find_taxonomy(template.taxonomy()).ok_or_else(|| {
        InitializeProjectError::Policy(ResolveProjectPolicyError::Catalog(
            ConfigurationCatalogError::UnknownTemplateTaxonomy {
                template: template.id().clone(),
                taxonomy: template.taxonomy().clone(),
            },
        ))
    })?;
    let typeset = catalog
        .find_typeset(template.taxonomy(), template.typeset())
        .ok_or_else(|| {
            InitializeProjectError::Policy(ResolveProjectPolicyError::Catalog(
                ConfigurationCatalogError::UnknownTemplateTypeset {
                    template: template.id().clone(),
                    taxonomy: template.taxonomy().clone(),
                    typeset: template.typeset().clone(),
                },
            ))
        })?;
    let custom = CustomConfiguration::new(
        (taxonomy_origin(taxonomy.id()) == ConfigurationOrigin::Custom)
            .then(|| taxonomy.clone())
            .into_iter()
            .collect(),
        (typeset_origin(typeset.taxonomy(), typeset.id()) == ConfigurationOrigin::Custom)
            .then(|| typeset.clone())
            .into_iter()
            .collect(),
        (template_origin(template.id()) == ConfigurationOrigin::Custom)
            .then(|| template.clone())
            .into_iter()
            .collect(),
    )
    .map_err(InitializeProjectError::InvalidCustom)?;
    ProjectConfig::new(PROJECT_CONFIG_VERSION, selected.clone(), custom)
        .map_err(InitializeProjectError::InvalidTemplate)
}
