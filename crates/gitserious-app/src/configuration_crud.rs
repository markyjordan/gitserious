use std::error::Error;
use std::fmt::{self, Display, Formatter};

use gitserious_core::{
    TaxonomyDefinition, TaxonomyId, TemplateDefinition, TemplateId, TypesetDefinition, TypesetId,
    built_in_configuration,
};

use crate::{
    ConfigurationCatalog, ConfigurationCatalogError, CustomConfiguration, GlobalConfigurationStore,
};

/// A typed identity used in configuration mutation diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigurationEntity {
    /// One taxonomy.
    Taxonomy(TaxonomyId),
    /// One taxonomy-scoped typeset.
    Typeset {
        /// Containing taxonomy.
        taxonomy: TaxonomyId,
        /// Typeset identifier.
        typeset: TypesetId,
    },
    /// One reusable template.
    Template(TemplateId),
}

impl Display for ConfigurationEntity {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Taxonomy(id) => write!(formatter, "taxonomy {id:?}"),
            Self::Typeset { taxonomy, typeset } => {
                write!(formatter, "typeset {taxonomy:?}/{typeset:?}")
            }
            Self::Template(id) => write!(formatter, "template {id:?}"),
        }
    }
}

/// One atomic edit to the custom configuration aggregate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigurationEdit {
    /// Creates a taxonomy that must not already exist.
    CreateTaxonomy(TaxonomyDefinition),
    /// Replaces a custom taxonomy with the same identity and a newer version.
    UpdateTaxonomy(TaxonomyDefinition),
    /// Deletes one custom taxonomy.
    DeleteTaxonomy(TaxonomyId),
    /// Creates a taxonomy-scoped typeset.
    CreateTypeset(TypesetDefinition),
    /// Replaces a custom typeset with the same identity and a newer version.
    UpdateTypeset(TypesetDefinition),
    /// Deletes one custom typeset.
    DeleteTypeset {
        /// Containing taxonomy.
        taxonomy: TaxonomyId,
        /// Typeset identifier.
        typeset: TypesetId,
    },
    /// Creates a reusable template.
    CreateTemplate(TemplateDefinition),
    /// Replaces a custom template with the same identity and a newer version.
    UpdateTemplate(TemplateDefinition),
    /// Deletes one custom template.
    DeleteTemplate(TemplateId),
}

/// A configuration query or mutation failure.
#[derive(Debug)]
pub enum ConfigurationMutationError<StoreError> {
    /// Persistence failed.
    Store(StoreError),
    /// The effective catalog is invalid.
    Catalog(ConfigurationCatalogError),
    /// A create operation targeted an existing identity.
    AlreadyExists(ConfigurationEntity),
    /// An update or delete targeted an unavailable identity.
    NotFound(ConfigurationEntity),
    /// A mutation targeted a built-in identity.
    Reserved(ConfigurationEntity),
    /// An update did not advance its semantic version.
    VersionNotAdvanced {
        /// Updated definition.
        entity: ConfigurationEntity,
        /// Current version.
        current: u16,
        /// Requested replacement version.
        replacement: u16,
    },
    /// A delete would leave dependent definitions dangling.
    Referenced {
        /// Definition requested for deletion.
        entity: ConfigurationEntity,
        /// Definitions that still reference it.
        dependents: Vec<ConfigurationEntity>,
    },
}

impl<StoreError> Display for ConfigurationMutationError<StoreError>
where
    StoreError: Display,
{
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(error) => Display::fmt(error, formatter),
            Self::Catalog(error) => Display::fmt(error, formatter),
            Self::AlreadyExists(entity) => write!(formatter, "{entity} already exists"),
            Self::NotFound(entity) => write!(formatter, "{entity} was not found"),
            Self::Reserved(entity) => write!(formatter, "{entity} is reserved by gitserious"),
            Self::VersionNotAdvanced {
                entity,
                current,
                replacement,
            } => write!(
                formatter,
                "{entity} version must advance beyond {current}, found {replacement}"
            ),
            Self::Referenced { entity, dependents } => {
                write!(formatter, "cannot delete {entity}; referenced by ")?;
                for (index, dependent) in dependents.iter().enumerate() {
                    if index > 0 {
                        formatter.write_str(", ")?;
                    }
                    Display::fmt(dependent, formatter)?;
                }
                Ok(())
            }
        }
    }
}

impl<StoreError> Error for ConfigurationMutationError<StoreError>
where
    StoreError: Error + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Store(error) => Some(error),
            Self::Catalog(error) => Some(error),
            Self::AlreadyExists(_)
            | Self::NotFound(_)
            | Self::Reserved(_)
            | Self::VersionNotAdvanced { .. }
            | Self::Referenced { .. } => None,
        }
    }
}

/// Returns all effective taxonomies in deterministic identity order.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] when loading or catalog validation
/// fails.
pub fn list_taxonomies<S>(
    store: &S,
) -> Result<Vec<TaxonomyDefinition>, ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    Ok(load_catalog(store)?.taxonomies().to_vec())
}

/// Finds one effective taxonomy.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] when loading or catalog validation
/// fails.
pub fn find_taxonomy<S>(
    store: &S,
    id: &TaxonomyId,
) -> Result<Option<TaxonomyDefinition>, ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    Ok(load_catalog(store)?.find_taxonomy(id).cloned())
}

/// Returns all effective typesets in deterministic qualified identity order.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] when loading or catalog validation
/// fails.
pub fn list_typesets<S>(
    store: &S,
) -> Result<Vec<TypesetDefinition>, ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    Ok(load_catalog(store)?.typesets().to_vec())
}

/// Finds one effective taxonomy-scoped typeset.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] when loading or catalog validation
/// fails.
pub fn find_typeset<S>(
    store: &S,
    taxonomy: &TaxonomyId,
    typeset: &TypesetId,
) -> Result<Option<TypesetDefinition>, ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    Ok(load_catalog(store)?
        .find_typeset(taxonomy, typeset)
        .cloned())
}

/// Returns all effective templates in deterministic identity order.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] when loading or catalog validation
/// fails.
pub fn list_templates<S>(
    store: &S,
) -> Result<Vec<TemplateDefinition>, ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    Ok(load_catalog(store)?.templates().to_vec())
}

/// Finds one effective reusable template.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] when loading or catalog validation
/// fails.
pub fn find_template<S>(
    store: &S,
    id: &TemplateId,
) -> Result<Option<TemplateDefinition>, ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    Ok(load_catalog(store)?.find_template(id).cloned())
}

/// Creates one custom taxonomy.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] for duplicate/reserved identities,
/// invalid resulting catalogs, concurrency, or persistence failures.
pub fn create_taxonomy<S>(
    store: &S,
    taxonomy: TaxonomyDefinition,
) -> Result<(), ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    apply_configuration_edits(store, [ConfigurationEdit::CreateTaxonomy(taxonomy)])
}

/// Updates one custom taxonomy without changing identity.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] when the target is unavailable or
/// reserved, identity/version rules fail, or the final catalog cannot persist.
pub fn update_taxonomy<S>(
    store: &S,
    taxonomy: TaxonomyDefinition,
) -> Result<(), ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    apply_configuration_edits(store, [ConfigurationEdit::UpdateTaxonomy(taxonomy)])
}

/// Deletes one unreferenced custom taxonomy.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] when the taxonomy is unavailable,
/// reserved, referenced, or cannot be persisted safely.
pub fn delete_taxonomy<S>(
    store: &S,
    id: &TaxonomyId,
) -> Result<(), ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    let current = store.load().map_err(ConfigurationMutationError::Store)?;
    reject_taxonomy_dependents(&current, id)?;
    apply_loaded_edits(
        store,
        &current,
        [ConfigurationEdit::DeleteTaxonomy(id.clone())],
    )
}

/// Creates one custom typeset.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] when identity, coverage, reference,
/// concurrency, or persistence validation fails.
pub fn create_typeset<S>(
    store: &S,
    typeset: TypesetDefinition,
) -> Result<(), ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    apply_configuration_edits(store, [ConfigurationEdit::CreateTypeset(typeset)])
}

/// Updates one custom typeset without changing qualified identity.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] when identity/version/catalog or
/// persistence validation fails.
pub fn update_typeset<S>(
    store: &S,
    typeset: TypesetDefinition,
) -> Result<(), ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    apply_configuration_edits(store, [ConfigurationEdit::UpdateTypeset(typeset)])
}

/// Deletes one unreferenced custom typeset.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] when the typeset is unavailable,
/// reserved, referenced, or cannot be persisted safely.
pub fn delete_typeset<S>(
    store: &S,
    taxonomy: &TaxonomyId,
    typeset: &TypesetId,
) -> Result<(), ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    let current = store.load().map_err(ConfigurationMutationError::Store)?;
    reject_typeset_dependents(&current, taxonomy, typeset)?;
    apply_loaded_edits(
        store,
        &current,
        [ConfigurationEdit::DeleteTypeset {
            taxonomy: taxonomy.clone(),
            typeset: typeset.clone(),
        }],
    )
}

/// Creates one custom template.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] when identity, references,
/// concurrency, or persistence validation fails.
pub fn create_template<S>(
    store: &S,
    template: TemplateDefinition,
) -> Result<(), ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    apply_configuration_edits(store, [ConfigurationEdit::CreateTemplate(template)])
}

/// Updates one custom template without changing identity.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] when identity/version/catalog or
/// persistence validation fails.
pub fn update_template<S>(
    store: &S,
    template: TemplateDefinition,
) -> Result<(), ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    apply_configuration_edits(store, [ConfigurationEdit::UpdateTemplate(template)])
}

/// Deletes one custom template.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] when the template is unavailable,
/// reserved, or cannot be persisted safely.
pub fn delete_template<S>(
    store: &S,
    id: &TemplateId,
) -> Result<(), ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    apply_configuration_edits(store, [ConfigurationEdit::DeleteTemplate(id.clone())])
}

/// Applies dependent item edits as one validated compare-and-swap mutation.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] when any item edit is invalid, the
/// final effective catalog is invalid, concurrent state changes, or persistence
/// fails.
pub fn apply_configuration_edits<S>(
    store: &S,
    edits: impl IntoIterator<Item = ConfigurationEdit>,
) -> Result<(), ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    let current = store.load().map_err(ConfigurationMutationError::Store)?;
    apply_loaded_edits(store, &current, edits)
}

fn apply_loaded_edits<S>(
    store: &S,
    current: &CustomConfiguration,
    edits: impl IntoIterator<Item = ConfigurationEdit>,
) -> Result<(), ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    let replacement = apply_custom_configuration_edits(current, edits)?;
    store
        .compare_and_swap(current, &replacement)
        .map_err(ConfigurationMutationError::Store)
}

/// Applies a complete edit batch to an in-memory custom configuration.
///
/// Persistence adapters can reuse this pure mutation contract for global and
/// project scopes while retaining the same reserved, version, dependency, and
/// final-catalog validation rules.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] when any edit or the final effective
/// catalog is invalid. This function never returns the `Store` variant.
pub fn apply_custom_configuration_edits<StoreError>(
    current: &CustomConfiguration,
    edits: impl IntoIterator<Item = ConfigurationEdit>,
) -> Result<CustomConfiguration, ConfigurationMutationError<StoreError>> {
    let mut replacement = current.clone();
    for edit in edits {
        apply_edit(&mut replacement, edit)?;
    }
    replacement.sort();
    ConfigurationCatalog::new(&replacement).map_err(ConfigurationMutationError::Catalog)?;
    Ok(replacement)
}

fn apply_edit<StoreError>(
    configuration: &mut CustomConfiguration,
    edit: ConfigurationEdit,
) -> Result<(), ConfigurationMutationError<StoreError>> {
    match edit {
        ConfigurationEdit::CreateTaxonomy(value) => apply_create_taxonomy(configuration, value)?,
        ConfigurationEdit::UpdateTaxonomy(value) => apply_update_taxonomy(configuration, value)?,
        ConfigurationEdit::DeleteTaxonomy(id) => apply_delete_taxonomy(configuration, &id)?,
        ConfigurationEdit::CreateTypeset(value) => apply_create_typeset(configuration, value)?,
        ConfigurationEdit::UpdateTypeset(value) => apply_update_typeset(configuration, value)?,
        ConfigurationEdit::DeleteTypeset { taxonomy, typeset } => {
            apply_delete_typeset(configuration, &taxonomy, &typeset)?;
        }
        ConfigurationEdit::CreateTemplate(value) => apply_create_template(configuration, value)?,
        ConfigurationEdit::UpdateTemplate(value) => apply_update_template(configuration, value)?,
        ConfigurationEdit::DeleteTemplate(id) => apply_delete_template(configuration, &id)?,
    }
    Ok(())
}

fn apply_create_taxonomy<StoreError>(
    configuration: &mut CustomConfiguration,
    value: TaxonomyDefinition,
) -> Result<(), ConfigurationMutationError<StoreError>> {
    let entity = taxonomy_entity(&value);
    reject_reserved(&entity)?;
    if configuration
        .taxonomies()
        .iter()
        .any(|current| current.id() == value.id())
    {
        return Err(ConfigurationMutationError::AlreadyExists(entity));
    }
    configuration.taxonomies_mut().push(value);
    Ok(())
}

fn apply_update_taxonomy<StoreError>(
    configuration: &mut CustomConfiguration,
    value: TaxonomyDefinition,
) -> Result<(), ConfigurationMutationError<StoreError>> {
    let entity = taxonomy_entity(&value);
    reject_reserved(&entity)?;
    let Some(current) = configuration
        .taxonomies_mut()
        .iter_mut()
        .find(|current| current.id() == value.id())
    else {
        return Err(ConfigurationMutationError::NotFound(entity));
    };
    require_version_advance(entity, current.version().get(), value.version().get())?;
    *current = value;
    Ok(())
}

fn apply_delete_taxonomy<StoreError>(
    configuration: &mut CustomConfiguration,
    id: &TaxonomyId,
) -> Result<(), ConfigurationMutationError<StoreError>> {
    let entity = ConfigurationEntity::Taxonomy(id.clone());
    reject_reserved(&entity)?;
    remove_matching(
        configuration.taxonomies_mut(),
        |current| current.id() == id,
        entity,
    )
}

fn apply_create_typeset<StoreError>(
    configuration: &mut CustomConfiguration,
    value: TypesetDefinition,
) -> Result<(), ConfigurationMutationError<StoreError>> {
    let entity = typeset_entity(&value);
    reject_reserved(&entity)?;
    if configuration
        .typesets()
        .iter()
        .any(|current| current.taxonomy() == value.taxonomy() && current.id() == value.id())
    {
        return Err(ConfigurationMutationError::AlreadyExists(entity));
    }
    configuration.typesets_mut().push(value);
    Ok(())
}

fn apply_update_typeset<StoreError>(
    configuration: &mut CustomConfiguration,
    value: TypesetDefinition,
) -> Result<(), ConfigurationMutationError<StoreError>> {
    let entity = typeset_entity(&value);
    reject_reserved(&entity)?;
    let Some(current) = configuration
        .typesets_mut()
        .iter_mut()
        .find(|current| current.taxonomy() == value.taxonomy() && current.id() == value.id())
    else {
        return Err(ConfigurationMutationError::NotFound(entity));
    };
    require_version_advance(entity, current.version().get(), value.version().get())?;
    *current = value;
    Ok(())
}

fn apply_delete_typeset<StoreError>(
    configuration: &mut CustomConfiguration,
    taxonomy: &TaxonomyId,
    typeset: &TypesetId,
) -> Result<(), ConfigurationMutationError<StoreError>> {
    let entity = ConfigurationEntity::Typeset {
        taxonomy: taxonomy.clone(),
        typeset: typeset.clone(),
    };
    reject_reserved(&entity)?;
    remove_matching(
        configuration.typesets_mut(),
        |current| current.taxonomy() == taxonomy && current.id() == typeset,
        entity,
    )
}

fn apply_create_template<StoreError>(
    configuration: &mut CustomConfiguration,
    value: TemplateDefinition,
) -> Result<(), ConfigurationMutationError<StoreError>> {
    let entity = template_entity(&value);
    reject_reserved(&entity)?;
    if configuration
        .templates()
        .iter()
        .any(|current| current.id() == value.id())
    {
        return Err(ConfigurationMutationError::AlreadyExists(entity));
    }
    configuration.templates_mut().push(value);
    Ok(())
}

fn apply_update_template<StoreError>(
    configuration: &mut CustomConfiguration,
    value: TemplateDefinition,
) -> Result<(), ConfigurationMutationError<StoreError>> {
    let entity = template_entity(&value);
    reject_reserved(&entity)?;
    let Some(current) = configuration
        .templates_mut()
        .iter_mut()
        .find(|current| current.id() == value.id())
    else {
        return Err(ConfigurationMutationError::NotFound(entity));
    };
    require_version_advance(entity, current.version().get(), value.version().get())?;
    *current = value;
    Ok(())
}

fn apply_delete_template<StoreError>(
    configuration: &mut CustomConfiguration,
    id: &TemplateId,
) -> Result<(), ConfigurationMutationError<StoreError>> {
    let entity = ConfigurationEntity::Template(id.clone());
    reject_reserved(&entity)?;
    remove_matching(
        configuration.templates_mut(),
        |current| current.id() == id,
        entity,
    )
}

fn load_catalog<S>(store: &S) -> Result<ConfigurationCatalog, ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    let configuration = store.load().map_err(ConfigurationMutationError::Store)?;
    ConfigurationCatalog::new(&configuration).map_err(ConfigurationMutationError::Catalog)
}

fn taxonomy_entity(value: &TaxonomyDefinition) -> ConfigurationEntity {
    ConfigurationEntity::Taxonomy(value.id().clone())
}

fn typeset_entity(value: &TypesetDefinition) -> ConfigurationEntity {
    ConfigurationEntity::Typeset {
        taxonomy: value.taxonomy().clone(),
        typeset: value.id().clone(),
    }
}

fn template_entity(value: &TemplateDefinition) -> ConfigurationEntity {
    ConfigurationEntity::Template(value.id().clone())
}

fn reject_reserved<StoreError>(
    entity: &ConfigurationEntity,
) -> Result<(), ConfigurationMutationError<StoreError>> {
    let built_in = built_in_configuration();
    let reserved = match entity {
        ConfigurationEntity::Taxonomy(id) => built_in.find_taxonomy(id).is_some(),
        ConfigurationEntity::Typeset { taxonomy, typeset } => {
            built_in.find_typeset(taxonomy, typeset).is_some()
        }
        ConfigurationEntity::Template(id) => built_in.find_template(id).is_some(),
    };
    if reserved {
        Err(ConfigurationMutationError::Reserved(entity.clone()))
    } else {
        Ok(())
    }
}

fn require_version_advance<StoreError>(
    entity: ConfigurationEntity,
    current: u16,
    replacement: u16,
) -> Result<(), ConfigurationMutationError<StoreError>> {
    if replacement > current {
        Ok(())
    } else {
        Err(ConfigurationMutationError::VersionNotAdvanced {
            entity,
            current,
            replacement,
        })
    }
}

fn remove_matching<T, StoreError>(
    values: &mut Vec<T>,
    predicate: impl Fn(&T) -> bool,
    entity: ConfigurationEntity,
) -> Result<(), ConfigurationMutationError<StoreError>> {
    let Some(index) = values.iter().position(predicate) else {
        return Err(ConfigurationMutationError::NotFound(entity));
    };
    values.remove(index);
    Ok(())
}

fn reject_taxonomy_dependents<StoreError>(
    configuration: &CustomConfiguration,
    id: &TaxonomyId,
) -> Result<(), ConfigurationMutationError<StoreError>> {
    let dependents = configuration
        .typesets()
        .iter()
        .filter(|typeset| typeset.taxonomy() == id)
        .map(typeset_entity)
        .chain(
            configuration
                .templates()
                .iter()
                .filter(|template| template.taxonomy() == id)
                .map(template_entity),
        )
        .collect::<Vec<_>>();
    if dependents.is_empty() {
        Ok(())
    } else {
        Err(ConfigurationMutationError::Referenced {
            entity: ConfigurationEntity::Taxonomy(id.clone()),
            dependents,
        })
    }
}

fn reject_typeset_dependents<StoreError>(
    configuration: &CustomConfiguration,
    taxonomy: &TaxonomyId,
    typeset: &TypesetId,
) -> Result<(), ConfigurationMutationError<StoreError>> {
    let dependents = configuration
        .templates()
        .iter()
        .filter(|template| template.taxonomy() == taxonomy && template.typeset() == typeset)
        .map(template_entity)
        .collect::<Vec<_>>();
    if dependents.is_empty() {
        Ok(())
    } else {
        Err(ConfigurationMutationError::Referenced {
            entity: ConfigurationEntity::Typeset {
                taxonomy: taxonomy.clone(),
                typeset: typeset.clone(),
            },
            dependents,
        })
    }
}
