use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

use gitserious_core::{TaxonomyId, TemplateId, TypesetId};

use crate::{
    ConfigurationCatalog, ConfigurationCatalogError, ConfigurationEdit, ConfigurationEntity,
    ConfigurationMutationError, ConfigurationOrigin, CustomConfiguration, PROJECT_CONFIG_VERSION,
    ProjectConfig, ProjectConfigError, ProjectLock, ProjectState, ProjectStateStore,
    RepositoryLocator, RepositoryRoot, ResolveProjectPolicyError, apply_custom_configuration_edits,
    fork_configuration_edits, fork_conventional_edits, resolve_project_lock, taxonomy_origin,
    template_origin, typeset_origin,
};

/// One atomic edit to repository-owned configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectConfigurationEdit {
    /// Applies one custom taxonomy, typeset, or template edit.
    Custom(ConfigurationEdit),
    /// Selects the active built-in or project custom template.
    SelectTemplate(TemplateId),
}

/// The complete project state after one successful configuration operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectConfigurationOutcome {
    root: RepositoryRoot,
    config: ProjectConfig,
    lock: ProjectLock,
}

impl ProjectConfigurationOutcome {
    /// Returns the configured repository root.
    #[must_use]
    pub const fn root(&self) -> &RepositoryRoot {
        &self.root
    }

    /// Returns the persisted project configuration.
    #[must_use]
    pub const fn config(&self) -> &ProjectConfig {
        &self.config
    }

    /// Returns the generated lock matching the configuration.
    #[must_use]
    pub const fn lock(&self) -> &ProjectLock {
        &self.lock
    }
}

/// Failure to inspect or mutate repository-owned configuration.
#[derive(Debug)]
pub enum ProjectConfigurationError<LocatorError, StoreError> {
    /// Repository discovery failed.
    Repository(LocatorError),
    /// Project persistence failed.
    Store(StoreError),
    /// The repository has no project configuration.
    NotInitialized,
    /// Authored project configuration exists without a lock.
    MissingLock,
    /// A lock exists without authored project configuration.
    OrphanLock,
    /// The observed lock does not match the authored project configuration.
    StaleLock,
    /// A custom-definition edit was invalid.
    Mutation(ConfigurationMutationError<StoreError>),
    /// The requested active template is unavailable from built-ins and project custom state.
    UnknownTemplate(TemplateId),
    /// The active custom template must be changed before it can be deleted.
    ActiveTemplateDeletion(TemplateId),
    /// A global import collided with a different project custom definition.
    ImportConflict(ConfigurationEntity),
    /// The source catalog could not provide one complete template chain.
    Source(ConfigurationCatalogError),
    /// The replacement project configuration was structurally invalid.
    InvalidProject(ProjectConfigError),
    /// The replacement policy could not be resolved.
    Policy(ResolveProjectPolicyError),
}

impl<LocatorError, StoreError> Display for ProjectConfigurationError<LocatorError, StoreError>
where
    LocatorError: Display,
    StoreError: Display,
{
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repository(error) => Display::fmt(error, formatter),
            Self::Store(error) => Display::fmt(error, formatter),
            Self::NotInitialized => formatter.write_str(
                "gitserious is not initialized; run `gitserious init` before configuring the project",
            ),
            Self::MissingLock => formatter.write_str(
                "gitserious.lock is missing; run `gitserious init` before configuring the project",
            ),
            Self::OrphanLock => formatter.write_str(
                "gitserious.lock exists without gitserious.toml; restore or remove the orphan lock",
            ),
            Self::StaleLock => formatter.write_str(
                "gitserious project policy is stale; run `gitserious init` before configuring the project",
            ),
            Self::Mutation(error) => Display::fmt(error, formatter),
            Self::UnknownTemplate(id) => write!(formatter, "template {id:?} is not available"),
            Self::ActiveTemplateDeletion(id) => write!(
                formatter,
                "cannot delete active template {id:?}; select another template first"
            ),
            Self::ImportConflict(entity) => write!(
                formatter,
                "cannot import {entity}; project custom configuration contains a different definition"
            ),
            Self::Source(error) => Display::fmt(error, formatter),
            Self::InvalidProject(error) => Display::fmt(error, formatter),
            Self::Policy(error) => Display::fmt(error, formatter),
        }
    }
}

impl<LocatorError, StoreError> Error for ProjectConfigurationError<LocatorError, StoreError>
where
    LocatorError: Error + 'static,
    StoreError: Error + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Repository(error) => Some(error),
            Self::Store(error) => Some(error),
            Self::Mutation(error) => Some(error),
            Self::Source(error) => Some(error),
            Self::InvalidProject(error) => Some(error),
            Self::Policy(error) => Some(error),
            Self::NotInitialized
            | Self::MissingLock
            | Self::OrphanLock
            | Self::StaleLock
            | Self::UnknownTemplate(_)
            | Self::ActiveTemplateDeletion(_)
            | Self::ImportConflict(_) => None,
        }
    }
}

/// Applies one validated batch to initialized project configuration.
///
/// # Errors
///
/// Returns [`ProjectConfigurationError`] when repository discovery, current
/// policy validation, any edit, final resolution, or guarded persistence fails.
pub fn apply_project_configuration_edits<L, S>(
    locator: &L,
    store: &S,
    start: &Path,
    edits: impl IntoIterator<Item = ProjectConfigurationEdit>,
) -> Result<ProjectConfigurationOutcome, ProjectConfigurationError<L::Error, S::Error>>
where
    L: RepositoryLocator + ?Sized,
    S: ProjectStateStore + ?Sized,
{
    let root = locator
        .locate(start)
        .map_err(ProjectConfigurationError::Repository)?;
    let state = store
        .inspect(&root)
        .map_err(ProjectConfigurationError::Store)?;
    let (current_config, current_lock) = initialized_state(state)?;
    apply_inspected_project_edits(root, store, &current_config, &current_lock, edits)
}

fn apply_inspected_project_edits<LocatorError, S>(
    root: RepositoryRoot,
    store: &S,
    current_config: &ProjectConfig,
    current_lock: &ProjectLock,
    edits: impl IntoIterator<Item = ProjectConfigurationEdit>,
) -> Result<ProjectConfigurationOutcome, ProjectConfigurationError<LocatorError, S::Error>>
where
    S: ProjectStateStore + ?Sized,
{
    if resolve_project_lock(current_config).map_err(ProjectConfigurationError::Policy)?
        != *current_lock
    {
        return Err(ProjectConfigurationError::StaleLock);
    }

    let mut custom_edits = Vec::new();
    let mut selected = current_config.active_template().clone();
    for edit in edits {
        match edit {
            ProjectConfigurationEdit::Custom(edit) => custom_edits.push(edit),
            ProjectConfigurationEdit::SelectTemplate(template) => selected = template,
        }
    }
    if selected == *current_config.active_template()
        && custom_edits.iter().any(|edit| {
            matches!(edit, ConfigurationEdit::DeleteTemplate(id) if id == current_config.active_template())
        })
    {
        return Err(ProjectConfigurationError::ActiveTemplateDeletion(selected));
    }

    let custom =
        apply_custom_configuration_edits::<S::Error>(current_config.custom(), custom_edits)
            .map_err(ProjectConfigurationError::Mutation)?;
    let replacement_config = ProjectConfig::new(PROJECT_CONFIG_VERSION, selected.clone(), custom)
        .map_err(ProjectConfigurationError::InvalidProject)?;
    let catalog = ConfigurationCatalog::new(replacement_config.custom()).map_err(|error| {
        ProjectConfigurationError::Mutation(ConfigurationMutationError::Catalog(error))
    })?;
    if catalog.find_template(&selected).is_none() {
        return Err(ProjectConfigurationError::UnknownTemplate(selected));
    }
    let replacement_lock =
        resolve_project_lock(&replacement_config).map_err(ProjectConfigurationError::Policy)?;
    if replacement_config != *current_config || replacement_lock != *current_lock {
        store
            .compare_and_swap(
                &root,
                current_config,
                current_lock,
                &replacement_config,
                &replacement_lock,
            )
            .map_err(ProjectConfigurationError::Store)?;
    }
    Ok(ProjectConfigurationOutcome {
        root,
        config: replacement_config,
        lock: replacement_lock,
    })
}

/// Selects one built-in or project custom template.
///
/// # Errors
///
/// Returns [`ProjectConfigurationError`] under the same conditions as
/// [`apply_project_configuration_edits`].
pub fn select_project_template<L, S>(
    locator: &L,
    store: &S,
    start: &Path,
    template: TemplateId,
) -> Result<ProjectConfigurationOutcome, ProjectConfigurationError<L::Error, S::Error>>
where
    L: RepositoryLocator + ?Sized,
    S: ProjectStateStore + ?Sized,
{
    apply_project_configuration_edits(
        locator,
        store,
        start,
        [ProjectConfigurationEdit::SelectTemplate(template)],
    )
}

/// Forks the built-in Conventional chain into project custom configuration.
///
/// # Errors
///
/// Returns [`ProjectConfigurationError`] when the project is unavailable or
/// any requested identity conflicts with built-in or project custom state.
pub fn fork_project_configuration<L, S>(
    locator: &L,
    store: &S,
    start: &Path,
    template: &TemplateId,
    taxonomy: &TaxonomyId,
    typeset: &TypesetId,
) -> Result<ProjectConfigurationOutcome, ProjectConfigurationError<L::Error, S::Error>>
where
    L: RepositoryLocator + ?Sized,
    S: ProjectStateStore + ?Sized,
{
    let edits = fork_conventional_edits(template, taxonomy, typeset)
        .into_iter()
        .map(ProjectConfigurationEdit::Custom);
    apply_project_configuration_edits(locator, store, start, edits)
}

/// Forks a built-in or project custom template into new project identities.
///
/// Global definitions must be imported first. The active template is unchanged,
/// and source inspection and destination persistence use the same snapshot.
///
/// # Errors
///
/// Returns [`ProjectConfigurationError`] when project policy or the source is
/// unavailable, target identities conflict, or guarded persistence fails.
pub fn fork_project_template<L, S>(
    locator: &L,
    store: &S,
    start: &Path,
    source: &TemplateId,
    template: &TemplateId,
    taxonomy: &TaxonomyId,
    typeset: &TypesetId,
) -> Result<ProjectConfigurationOutcome, ProjectConfigurationError<L::Error, S::Error>>
where
    L: RepositoryLocator + ?Sized,
    S: ProjectStateStore + ?Sized,
{
    let root = locator
        .locate(start)
        .map_err(ProjectConfigurationError::Repository)?;
    let state = store
        .inspect(&root)
        .map_err(ProjectConfigurationError::Store)?;
    let (config, lock) = initialized_state(state)?;
    let catalog =
        ConfigurationCatalog::new(config.custom()).map_err(ProjectConfigurationError::Source)?;
    let edits = fork_configuration_edits(&catalog, source, template, taxonomy, typeset)
        .map_err(ProjectConfigurationError::Source)?
        .into_iter()
        .map(ProjectConfigurationEdit::Custom);
    apply_inspected_project_edits(root, store, &config, &lock, edits)
}

/// Copies one global custom template chain into project custom configuration.
///
/// Exact matching definitions are reused, different definitions at the same
/// identity are rejected, and built-in definitions are never persisted.
///
/// # Errors
///
/// Returns [`ProjectConfigurationError`] when the source chain is unavailable,
/// an import identity conflicts, or project mutation fails.
pub fn import_project_template<L, S>(
    locator: &L,
    store: &S,
    source: &ConfigurationCatalog,
    start: &Path,
    template: &TemplateId,
) -> Result<ProjectConfigurationOutcome, ProjectConfigurationError<L::Error, S::Error>>
where
    L: RepositoryLocator + ?Sized,
    S: ProjectStateStore + ?Sized,
{
    import_project_template_with_selection(locator, store, source, start, template, false)
}

/// Copies one global custom template chain and selects it in one project write.
///
/// # Errors
///
/// Returns [`ProjectConfigurationError`] under the same conditions as
/// [`import_project_template`].
pub fn import_and_select_project_template<L, S>(
    locator: &L,
    store: &S,
    source: &ConfigurationCatalog,
    start: &Path,
    template: &TemplateId,
) -> Result<ProjectConfigurationOutcome, ProjectConfigurationError<L::Error, S::Error>>
where
    L: RepositoryLocator + ?Sized,
    S: ProjectStateStore + ?Sized,
{
    import_project_template_with_selection(locator, store, source, start, template, true)
}

fn import_project_template_with_selection<L, S>(
    locator: &L,
    store: &S,
    source: &ConfigurationCatalog,
    start: &Path,
    selected: &TemplateId,
    select: bool,
) -> Result<ProjectConfigurationOutcome, ProjectConfigurationError<L::Error, S::Error>>
where
    L: RepositoryLocator + ?Sized,
    S: ProjectStateStore + ?Sized,
{
    let root = locator
        .locate(start)
        .map_err(ProjectConfigurationError::Repository)?;
    let state = store
        .inspect(&root)
        .map_err(ProjectConfigurationError::Store)?;
    let (current, lock) = initialized_state(state)?;
    let edits = import_edits(source, current.custom(), selected)?;
    let mut project_edits = edits
        .into_iter()
        .map(ProjectConfigurationEdit::Custom)
        .collect::<Vec<_>>();
    if select {
        project_edits.push(ProjectConfigurationEdit::SelectTemplate(selected.clone()));
    }
    apply_inspected_project_edits(root, store, &current, &lock, project_edits)
}

pub(crate) fn import_edits<LocatorError, StoreError>(
    source: &ConfigurationCatalog,
    destination: &CustomConfiguration,
    selected: &TemplateId,
) -> Result<Vec<ConfigurationEdit>, ProjectConfigurationError<LocatorError, StoreError>> {
    source
        .resolve(selected)
        .map_err(ProjectConfigurationError::Source)?;
    let template = source.find_template(selected).ok_or_else(|| {
        ProjectConfigurationError::Source(ConfigurationCatalogError::UnknownTemplate(
            selected.clone(),
        ))
    })?;
    let taxonomy = source.find_taxonomy(template.taxonomy()).ok_or_else(|| {
        ProjectConfigurationError::Source(ConfigurationCatalogError::UnknownTemplateTaxonomy {
            template: template.id().clone(),
            taxonomy: template.taxonomy().clone(),
        })
    })?;
    let typeset = source
        .find_typeset(template.taxonomy(), template.typeset())
        .ok_or_else(|| {
            ProjectConfigurationError::Source(ConfigurationCatalogError::UnknownTemplateTypeset {
                template: template.id().clone(),
                taxonomy: template.taxonomy().clone(),
                typeset: template.typeset().clone(),
            })
        })?;

    let mut edits = Vec::new();
    if taxonomy_origin(taxonomy.id()) == ConfigurationOrigin::Custom {
        push_taxonomy_import(destination, taxonomy, &mut edits)?;
    }
    if typeset_origin(typeset.taxonomy(), typeset.id()) == ConfigurationOrigin::Custom {
        push_typeset_import(destination, typeset, &mut edits)?;
    }
    if template_origin(template.id()) == ConfigurationOrigin::Custom {
        push_template_import(destination, template, &mut edits)?;
    }
    Ok(edits)
}

fn push_taxonomy_import<LocatorError, StoreError>(
    destination: &CustomConfiguration,
    source: &gitserious_core::TaxonomyDefinition,
    edits: &mut Vec<ConfigurationEdit>,
) -> Result<(), ProjectConfigurationError<LocatorError, StoreError>> {
    match destination
        .taxonomies()
        .iter()
        .find(|current| current.id() == source.id())
    {
        Some(current) if current == source => Ok(()),
        Some(_) => Err(ProjectConfigurationError::ImportConflict(
            ConfigurationEntity::Taxonomy(source.id().clone()),
        )),
        None => {
            edits.push(ConfigurationEdit::CreateTaxonomy(source.clone()));
            Ok(())
        }
    }
}

fn push_typeset_import<LocatorError, StoreError>(
    destination: &CustomConfiguration,
    source: &gitserious_core::TypesetDefinition,
    edits: &mut Vec<ConfigurationEdit>,
) -> Result<(), ProjectConfigurationError<LocatorError, StoreError>> {
    match destination
        .typesets()
        .iter()
        .find(|current| current.taxonomy() == source.taxonomy() && current.id() == source.id())
    {
        Some(current) if current == source => Ok(()),
        Some(_) => Err(ProjectConfigurationError::ImportConflict(
            ConfigurationEntity::Typeset {
                taxonomy: source.taxonomy().clone(),
                typeset: source.id().clone(),
            },
        )),
        None => {
            edits.push(ConfigurationEdit::CreateTypeset(source.clone()));
            Ok(())
        }
    }
}

fn push_template_import<LocatorError, StoreError>(
    destination: &CustomConfiguration,
    source: &gitserious_core::TemplateDefinition,
    edits: &mut Vec<ConfigurationEdit>,
) -> Result<(), ProjectConfigurationError<LocatorError, StoreError>> {
    match destination
        .templates()
        .iter()
        .find(|current| current.id() == source.id())
    {
        Some(current) if current == source => Ok(()),
        Some(_) => Err(ProjectConfigurationError::ImportConflict(
            ConfigurationEntity::Template(source.id().clone()),
        )),
        None => {
            edits.push(ConfigurationEdit::CreateTemplate(source.clone()));
            Ok(())
        }
    }
}

fn initialized_state<LocatorError, StoreError>(
    state: ProjectState,
) -> Result<(ProjectConfig, ProjectLock), ProjectConfigurationError<LocatorError, StoreError>> {
    match state {
        ProjectState::Initialized { config, lock } => Ok((config, lock)),
        ProjectState::Absent => Err(ProjectConfigurationError::NotInitialized),
        ProjectState::ConfigOnly(_) => Err(ProjectConfigurationError::MissingLock),
        ProjectState::LockOnly => Err(ProjectConfigurationError::OrphanLock),
    }
}
