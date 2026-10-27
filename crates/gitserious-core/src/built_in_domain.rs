use crate::{
    CommitTypeDefinition, CommitTypeId, PropertyDefinition, PropertyKey, PropertyRequirement,
};

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
