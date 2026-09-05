use gitserious_core::{
    TaxonomyDefinition, TaxonomyId, TaxonomyVersion, TemplateDefinition, TemplateId,
    TemplateVersion, TypesetDefinition, TypesetId, TypesetVersion, built_in_configuration,
};

use crate::{
    ConfigurationCatalog, ConfigurationCatalogError, ConfigurationEdit, ConfigurationMutationError,
    GlobalConfigurationStore, apply_custom_configuration_edits,
};

/// The custom identities minted by one configuration bundle fork.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForkedConfiguration {
    template: TemplateId,
    taxonomy: TaxonomyId,
    typeset: TypesetId,
}

impl ForkedConfiguration {
    /// Returns the forked reusable-template identifier.
    #[must_use]
    pub const fn template(&self) -> &TemplateId {
        &self.template
    }

    /// Returns the forked taxonomy identifier.
    #[must_use]
    pub const fn taxonomy(&self) -> &TaxonomyId {
        &self.taxonomy
    }

    /// Returns the forked taxonomy-scoped typeset identifier.
    #[must_use]
    pub const fn typeset(&self) -> &TypesetId {
        &self.typeset
    }
}

/// Copies the complete built-in Conventional chain into custom
/// definitions under freshly chosen identities.
///
/// The fork records one atomic batch: a taxonomy carrying every built-in
/// change type, a taxonomy-scoped typeset carrying every built-in schema, and
/// a template selecting both at their first versions. Built-in definitions
/// themselves stay immutable, and the copied semantic descriptions are
/// preserved verbatim because the fork changes ownership rather than meaning.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] when any identity is reserved or
/// already present, the resulting catalog is invalid, the stored state changed
/// concurrently, or persistence fails.
pub fn fork_conventional<S>(
    store: &S,
    template: TemplateId,
    taxonomy: TaxonomyId,
    typeset: TypesetId,
) -> Result<ForkedConfiguration, ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    fork_configuration(
        store,
        built_in_configuration().template().id(),
        template,
        taxonomy,
        typeset,
    )
}

/// Forks a built-in or global custom template into new global custom identities.
///
/// The source and destination are read from one snapshot. New definitions start
/// at version one while preserving source descriptions and schema ordering.
///
/// # Errors
///
/// Returns [`ConfigurationMutationError`] for an unavailable source, invalid or
/// conflicting target identities, concurrent changes, or persistence failures.
pub fn fork_configuration<S>(
    store: &S,
    source: &TemplateId,
    template: TemplateId,
    taxonomy: TaxonomyId,
    typeset: TypesetId,
) -> Result<ForkedConfiguration, ConfigurationMutationError<S::Error>>
where
    S: GlobalConfigurationStore + ?Sized,
{
    let current = store.load().map_err(ConfigurationMutationError::Store)?;
    let catalog =
        ConfigurationCatalog::new(&current).map_err(ConfigurationMutationError::Catalog)?;
    let edits = fork_configuration_edits(&catalog, source, &template, &taxonomy, &typeset)
        .map_err(ConfigurationMutationError::Catalog)?;
    let replacement = apply_custom_configuration_edits::<S::Error>(&current, edits)?;
    store
        .compare_and_swap(&current, &replacement)
        .map_err(ConfigurationMutationError::Store)?;
    Ok(ForkedConfiguration {
        template,
        taxonomy,
        typeset,
    })
}

/// Builds a complete fork batch from one resolved catalog snapshot.
///
/// # Errors
///
/// Returns [`ConfigurationCatalogError`] when the requested source is absent or
/// cannot resolve. Target identity and dependency checks happen when applying
/// the batch to the destination configuration.
pub fn fork_configuration_edits(
    source: &ConfigurationCatalog,
    selected: &TemplateId,
    template: &TemplateId,
    taxonomy: &TaxonomyId,
    typeset: &TypesetId,
) -> Result<Vec<ConfigurationEdit>, ConfigurationCatalogError> {
    source.resolve(selected)?;
    let source_template = source
        .find_template(selected)
        .ok_or_else(|| ConfigurationCatalogError::UnknownTemplate(selected.clone()))?;
    let source_taxonomy = source
        .find_taxonomy(source_template.taxonomy())
        .ok_or_else(|| ConfigurationCatalogError::UnknownTemplateTaxonomy {
            template: selected.clone(),
            taxonomy: source_template.taxonomy().clone(),
        })?;
    let source_typeset = source
        .find_typeset(source_template.taxonomy(), source_template.typeset())
        .ok_or_else(|| ConfigurationCatalogError::UnknownTemplateTypeset {
            template: selected.clone(),
            taxonomy: source_template.taxonomy().clone(),
            typeset: source_template.typeset().clone(),
        })?;
    Ok(bundle_edits(
        source_taxonomy,
        source_typeset,
        source_template,
        template,
        taxonomy,
        typeset,
    ))
}

/// Builds the atomic edit batch for one editable Conventional fork.
#[must_use]
pub fn fork_conventional_edits(
    template: &TemplateId,
    taxonomy: &TaxonomyId,
    typeset: &TypesetId,
) -> Vec<ConfigurationEdit> {
    let built_in = built_in_configuration();
    bundle_edits(
        built_in.taxonomy(),
        built_in.typeset(),
        built_in.template(),
        template,
        taxonomy,
        typeset,
    )
}

fn bundle_edits(
    source_taxonomy: &TaxonomyDefinition,
    source_typeset: &TypesetDefinition,
    source_template: &TemplateDefinition,
    template: &TemplateId,
    taxonomy: &TaxonomyId,
    typeset: &TypesetId,
) -> Vec<ConfigurationEdit> {
    let taxonomy_definition = TaxonomyDefinition::from_trusted(
        taxonomy.clone(),
        TaxonomyVersion::V1,
        source_taxonomy.description().clone(),
        source_taxonomy.change_types().to_vec(),
    );
    let typeset_definition = TypesetDefinition::from_trusted(
        taxonomy.clone(),
        typeset.clone(),
        TypesetVersion::V1,
        source_typeset.description().clone(),
        source_typeset.schemas().to_vec(),
    );
    let template_definition = TemplateDefinition::new(
        template.clone(),
        TemplateVersion::V1,
        source_template.description().clone(),
        taxonomy.clone(),
        typeset.clone(),
    );

    vec![
        ConfigurationEdit::CreateTaxonomy(taxonomy_definition),
        ConfigurationEdit::CreateTypeset(typeset_definition),
        ConfigurationEdit::CreateTemplate(template_definition),
    ]
}
