use std::sync::LazyLock;

use crate::{
    ChangeTypeDefinition, ChangeTypeSchema, Description, TaxonomyDefinition, TaxonomyId,
    TaxonomyVersion, TemplateDefinition, TemplateId, TemplateVersion, TypesetDefinition, TypesetId,
    TypesetVersion, built_in_commit_types,
};

/// The complete built-in taxonomy, typeset, and template catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuiltInConfiguration {
    taxonomies: Vec<TaxonomyDefinition>,
    typesets: Vec<TypesetDefinition>,
    templates: Vec<TemplateDefinition>,
}

impl BuiltInConfiguration {
    /// Returns built-in taxonomies in catalog order, with Conventional first.
    #[must_use]
    pub fn taxonomies(&self) -> &[TaxonomyDefinition] {
        &self.taxonomies
    }

    /// Returns all built-in typesets in catalog order.
    #[must_use]
    pub fn typesets(&self) -> &[TypesetDefinition] {
        &self.typesets
    }

    /// Returns all built-in templates, with the existing default first.
    #[must_use]
    pub fn templates(&self) -> &[TemplateDefinition] {
        &self.templates
    }

    /// Finds a built-in taxonomy without relying on its catalog position.
    #[must_use]
    pub fn find_taxonomy(&self, id: &TaxonomyId) -> Option<&TaxonomyDefinition> {
        self.taxonomies.iter().find(|value| value.id() == id)
    }

    /// Finds a built-in typeset by its taxonomy-qualified identity.
    #[must_use]
    pub fn find_typeset(
        &self,
        taxonomy: &TaxonomyId,
        id: &TypesetId,
    ) -> Option<&TypesetDefinition> {
        self.typesets
            .iter()
            .find(|value| value.taxonomy() == taxonomy && value.id() == id)
    }

    /// Finds a built-in template without relying on its catalog position.
    #[must_use]
    pub fn find_template(&self, id: &TemplateId) -> Option<&TemplateDefinition> {
        self.templates.iter().find(|value| value.id() == id)
    }

    /// Returns the built-in Conventional taxonomy.
    #[must_use]
    pub fn taxonomy(&self) -> &TaxonomyDefinition {
        &self.taxonomies[0]
    }

    /// Returns the built-in default durable-property typeset.
    #[must_use]
    pub fn typeset(&self) -> &TypesetDefinition {
        &self.typesets[0]
    }

    /// Returns the built-in default reusable template.
    #[must_use]
    pub fn template(&self) -> &TemplateDefinition {
        &self.templates[0]
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
    // The compatibility accessors intentionally refer to the first bundle.
    BuiltInConfiguration {
        taxonomies: vec![taxonomy],
        typesets: vec![typeset],
        templates: vec![template],
    }
});

/// Returns the immutable built-in configuration represented by public models.
#[must_use]
pub fn built_in_configuration() -> &'static BuiltInConfiguration {
    &BUILT_IN_CONFIGURATION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn additional_bundles_do_not_replace_the_compatibility_default() {
        let original = built_in_configuration();
        let mut catalog = original.clone();
        let taxonomy = TaxonomyId::from_trusted("second");
        let typeset = TypesetId::from_trusted("default");
        let template = TemplateId::from_trusted("second-template");
        catalog.taxonomies.push(TaxonomyDefinition::from_trusted(
            taxonomy.clone(),
            TaxonomyVersion::V1,
            Description::from_trusted("A second taxonomy."),
            original.taxonomy().change_types().to_vec(),
        ));
        catalog.typesets.push(TypesetDefinition::from_trusted(
            taxonomy.clone(),
            typeset.clone(),
            TypesetVersion::V1,
            Description::from_trusted("A second default typeset."),
            original.typeset().schemas().to_vec(),
        ));
        catalog.templates.push(TemplateDefinition::new(
            template.clone(),
            TemplateVersion::V1,
            Description::from_trusted("A second template."),
            taxonomy.clone(),
            typeset.clone(),
        ));

        assert_eq!(catalog.taxonomy(), original.taxonomy());
        assert_eq!(catalog.typeset(), original.typeset());
        assert_eq!(catalog.template(), original.template());
        assert_eq!(catalog.taxonomies().len(), 2);
        assert_eq!(catalog.typesets().len(), 2);
        assert_eq!(catalog.templates().len(), 2);
        assert_eq!(
            catalog.find_taxonomy(&taxonomy),
            catalog.taxonomies().get(1)
        );
        assert_eq!(catalog.find_template(&template), catalog.templates().get(1));
        assert_eq!(
            catalog.find_typeset(&taxonomy, &typeset),
            catalog.typesets().get(1)
        );
        assert_eq!(
            catalog.find_typeset(original.taxonomy().id(), &typeset),
            Some(original.typeset())
        );
        assert!(
            catalog
                .find_taxonomy(&TaxonomyId::from_trusted("missing"))
                .is_none()
        );
        assert!(
            catalog
                .find_template(&TemplateId::from_trusted("missing"))
                .is_none()
        );
        assert!(
            catalog
                .find_typeset(&TaxonomyId::from_trusted("missing"), &typeset)
                .is_none()
        );
        assert!(
            catalog
                .find_typeset(&taxonomy, &TypesetId::from_trusted("missing"))
                .is_none()
        );
        for definition in catalog.templates() {
            let resolved = catalog
                .find_taxonomy(definition.taxonomy())
                .zip(catalog.find_typeset(definition.taxonomy(), definition.typeset()))
                .map(|(taxonomy, typeset)| {
                    crate::ResolvedTaxonomy::resolve(definition, taxonomy, typeset)
                });
            assert!(matches!(resolved, Some(Ok(_))));
        }
    }
}
