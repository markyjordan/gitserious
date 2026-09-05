use crate::{
    ChangeTypeDefinition, ChangeTypeSchema, CommitTypeDefinition, CommitTypeId, Description,
    PropertyDefinition, PropertyKey, PropertyRequirement, TaxonomyDefinition, TaxonomyId,
    TaxonomyVersion, TemplateDefinition, TemplateId, TemplateVersion, TypesetDefinition, TypesetId,
    TypesetVersion,
};

pub(crate) type BuiltInBundle = (TaxonomyDefinition, TypesetDefinition, TemplateDefinition);

pub(crate) fn bundle(
    id: &'static str,
    taxonomy_description: &'static str,
    typeset_description: &'static str,
    template_description: &'static str,
    definitions: &[CommitTypeDefinition],
) -> BuiltInBundle {
    let taxonomy_id = TaxonomyId::from_trusted(id);
    let typeset_id = TypesetId::from_trusted("default");
    let taxonomy = TaxonomyDefinition::from_trusted(
        taxonomy_id.clone(),
        TaxonomyVersion::V1,
        Description::from_trusted(taxonomy_description),
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
        Description::from_trusted(typeset_description),
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
        TemplateId::from_trusted(id),
        TemplateVersion::V1,
        Description::from_trusted(template_description),
        taxonomy_id,
        typeset_id,
    );
    (taxonomy, typeset, template)
}

pub(crate) fn change_type(
    id: &'static str,
    description: &'static str,
    properties: Vec<PropertyDefinition>,
) -> CommitTypeDefinition {
    CommitTypeDefinition::from_trusted(CommitTypeId::from_trusted(id), description, properties)
}

pub(crate) fn required(key: &'static str, guidance: &'static str) -> PropertyDefinition {
    PropertyDefinition::from_trusted(
        PropertyKey::from_trusted(key),
        guidance,
        PropertyRequirement::Required,
    )
}

pub(crate) fn recommended(key: &'static str, guidance: &'static str) -> PropertyDefinition {
    PropertyDefinition::from_trusted(
        PropertyKey::from_trusted(key),
        guidance,
        PropertyRequirement::Recommended,
    )
}
