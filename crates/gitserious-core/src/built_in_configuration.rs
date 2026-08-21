use std::sync::LazyLock;

use crate::{
    ChangeTypeDefinition, ChangeTypeSchema, Description, TaxonomyDefinition, TaxonomyId,
    TaxonomyVersion, TemplateDefinition, TemplateId, TemplateVersion, TypesetDefinition, TypesetId,
    TypesetVersion, built_in_commit_types,
};

/// The complete built-in taxonomy, typeset, and template catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuiltInConfiguration {
    taxonomy: TaxonomyDefinition,
    typeset: TypesetDefinition,
    template: TemplateDefinition,
}

impl BuiltInConfiguration {
    /// Returns the built-in Conventional taxonomy.
    #[must_use]
    pub const fn taxonomy(&self) -> &TaxonomyDefinition {
        &self.taxonomy
    }

    /// Returns the built-in default durable-property typeset.
    #[must_use]
    pub const fn typeset(&self) -> &TypesetDefinition {
        &self.typeset
    }

    /// Returns the built-in default reusable template.
    #[must_use]
    pub const fn template(&self) -> &TemplateDefinition {
        &self.template
    }
}

static BUILT_IN_CONFIGURATION: LazyLock<BuiltInConfiguration> = LazyLock::new(|| {
    let taxonomy_id = TaxonomyId::from_trusted("conventional");
    let typeset_id = TypesetId::from_trusted("default");
    let definitions = built_in_commit_types();
    let taxonomy = TaxonomyDefinition::from_trusted(
        taxonomy_id.clone(),
        TaxonomyVersion::V1,
        Description::from_trusted(
            "The Conventional Commits classification system for software changes.",
        ),
        definitions
            .iter()
            .map(|definition| {
                ChangeTypeDefinition::new(
                    definition.id().clone(),
                    Description::from_validated(definition.description()),
                )
            })
            .collect(),
    );
    let typeset = TypesetDefinition::from_trusted(
        taxonomy_id.clone(),
        typeset_id.clone(),
        TypesetVersion::V1,
        Description::from_trusted(
            "The default durable context captured for Conventional change types.",
        ),
        definitions
            .iter()
            .map(|definition| {
                ChangeTypeSchema::from_trusted(
                    definition.id().clone(),
                    definition.properties().to_vec(),
                )
            })
            .collect(),
    );
    let template = TemplateDefinition::new(
        TemplateId::from_trusted("default"),
        TemplateVersion::V1,
        Description::from_trusted(
            "The built-in Conventional taxonomy with its default durable-property typeset.",
        ),
        taxonomy_id,
        typeset_id,
    );
    BuiltInConfiguration {
        taxonomy,
        typeset,
        template,
    }
});

/// Returns the immutable built-in configuration represented by public models.
#[must_use]
pub fn built_in_configuration() -> &'static BuiltInConfiguration {
    &BUILT_IN_CONFIGURATION
}
