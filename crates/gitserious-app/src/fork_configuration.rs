use gitserious_core::{
    TaxonomyDefinition, TaxonomyId, TaxonomyVersion, TemplateDefinition, TemplateId,
    TemplateVersion, TypesetDefinition, TypesetId, TypesetVersion, built_in_configuration,
};

use crate::{
    ConfigurationEdit, ConfigurationMutationError, UserConfigurationStore,
    apply_configuration_edits,
};

/// The user-owned identities minted by one built-in-configuration fork.
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

/// Copies the complete built-in Conventional chain into user-owned
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
    S: UserConfigurationStore + ?Sized,
{
    let built_in = built_in_configuration();
    let taxonomy_definition = TaxonomyDefinition::from_trusted(
        taxonomy.clone(),
        TaxonomyVersion::V1,
        built_in.taxonomy().description().clone(),
        built_in.taxonomy().change_types().to_vec(),
    );
    let typeset_definition = TypesetDefinition::from_trusted(
        taxonomy.clone(),
        typeset.clone(),
        TypesetVersion::V1,
        built_in.typeset().description().clone(),
        built_in.typeset().schemas().to_vec(),
    );
    let template_definition = TemplateDefinition::new(
        template.clone(),
        TemplateVersion::V1,
        built_in.template().description().clone(),
        taxonomy.clone(),
        typeset.clone(),
    );

    apply_configuration_edits(
        store,
        [
            ConfigurationEdit::CreateTaxonomy(taxonomy_definition),
            ConfigurationEdit::CreateTypeset(typeset_definition),
            ConfigurationEdit::CreateTemplate(template_definition),
        ],
    )?;
    Ok(ForkedConfiguration {
        template,
        taxonomy,
        typeset,
    })
}
