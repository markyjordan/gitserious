use gitserious_app::ConfigurationError;
use gitserious_core::{
    CommitTypeDefinition, CommitTypeId, ConditionId, Description, PropertyCondition,
    PropertyDefinition, PropertyKey, PropertyMultiplicity, PropertyRequirement, SchemaVersion,
    Taxonomy, TaxonomyId, TaxonomyLineage, TaxonomyVersion,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct TaxonomyWire {
    id: String,
    version: u16,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    derived_from: Option<LineageWire>,
    commit_types: Vec<CommitTypeWire>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct LineageWire {
    taxonomy: String,
    version: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct CommitTypeWire {
    id: String,
    schema_version: u16,
    description: String,
    properties: Vec<PropertyWire>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct PropertyWire {
    key: String,
    description: String,
    multiplicity: MultiplicityWire,
    requirement: RequirementWire,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum MultiplicityWire {
    Single,
    Multiple,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "level", rename_all = "kebab-case", deny_unknown_fields)]
enum RequirementWire {
    Required,
    Recommended,
    Optional,
    Conditional {
        condition: String,
        rationale: String,
    },
}

pub(crate) fn taxonomy_to_wire(taxonomy: &Taxonomy) -> TaxonomyWire {
    TaxonomyWire {
        id: taxonomy.id().to_string(),
        version: taxonomy.version().get(),
        description: taxonomy.description().to_string(),
        derived_from: taxonomy.derived_from().map(|source| LineageWire {
            taxonomy: source.id().to_string(),
            version: source.version().get(),
        }),
        commit_types: taxonomy
            .commit_types()
            .iter()
            .map(|definition| CommitTypeWire {
                id: definition.id().to_string(),
                schema_version: definition.schema_version().get(),
                description: definition.description().to_owned(),
                properties: definition
                    .properties()
                    .iter()
                    .map(property_to_wire)
                    .collect(),
            })
            .collect(),
    }
}

fn property_to_wire(property: &PropertyDefinition) -> PropertyWire {
    PropertyWire {
        key: property.key().to_string(),
        description: property.description().to_owned(),
        multiplicity: match property.multiplicity() {
            PropertyMultiplicity::Single => MultiplicityWire::Single,
            PropertyMultiplicity::Multiple => MultiplicityWire::Multiple,
        },
        requirement: match property.requirement() {
            PropertyRequirement::Required => RequirementWire::Required,
            PropertyRequirement::Recommended => RequirementWire::Recommended,
            PropertyRequirement::Optional => RequirementWire::Optional,
            PropertyRequirement::Conditional(condition) => RequirementWire::Conditional {
                condition: condition.id().to_string(),
                rationale: condition.rationale().to_owned(),
            },
        },
    }
}

pub(crate) fn taxonomy_from_wire(wire: TaxonomyWire, location: &str) -> Result<Taxonomy, String> {
    let id = TaxonomyId::new(wire.id).map_err(|error| format!("{location}.id: {error}"))?;
    let version = TaxonomyVersion::new(wire.version)
        .map_err(|error| format!("{location}.version: {error}"))?;
    let description = Description::new(wire.description)
        .map_err(|error| format!("{location}.description: {error}"))?;
    let derived_from = if let Some(source) = wire.derived_from {
        Some(TaxonomyLineage::new(
            TaxonomyId::new(source.taxonomy)
                .map_err(|error| format!("{location}.derived-from.taxonomy: {error}"))?,
            TaxonomyVersion::new(source.version)
                .map_err(|error| format!("{location}.derived-from.version: {error}"))?,
        ))
    } else {
        None
    };
    let commit_types = wire
        .commit_types
        .into_iter()
        .enumerate()
        .map(|(index, definition)| {
            commit_type_from_wire(definition, &format!("{location}.commit-types[{index}]"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Taxonomy::new(id, version, description, derived_from, commit_types)
        .map_err(|error| format!("{location}: {error}"))
}

fn commit_type_from_wire(
    wire: CommitTypeWire,
    location: &str,
) -> Result<CommitTypeDefinition, String> {
    let id = CommitTypeId::new(wire.id).map_err(|error| format!("{location}.id: {error}"))?;
    let version = SchemaVersion::new(wire.schema_version)
        .map_err(|error| format!("{location}.schema-version: {error}"))?;
    let properties = wire
        .properties
        .into_iter()
        .enumerate()
        .map(|(index, property)| {
            property_from_wire(property, &format!("{location}.properties[{index}]"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    CommitTypeDefinition::new(version, id, wire.description, properties)
        .map_err(|error| format!("{location}: {error}"))
}

fn property_from_wire(wire: PropertyWire, location: &str) -> Result<PropertyDefinition, String> {
    let key = PropertyKey::new(wire.key).map_err(|error| format!("{location}.key: {error}"))?;
    let multiplicity = match wire.multiplicity {
        MultiplicityWire::Single => PropertyMultiplicity::Single,
        MultiplicityWire::Multiple => PropertyMultiplicity::Multiple,
    };
    let requirement = match wire.requirement {
        RequirementWire::Required => PropertyRequirement::Required,
        RequirementWire::Recommended => PropertyRequirement::Recommended,
        RequirementWire::Optional => PropertyRequirement::Optional,
        RequirementWire::Conditional {
            condition,
            rationale,
        } => {
            let id = ConditionId::new(condition)
                .map_err(|error| format!("{location}.requirement.condition: {error}"))?;
            let condition = PropertyCondition::new(id, rationale)
                .map_err(|error| format!("{location}.requirement.rationale: {error}"))?;
            PropertyRequirement::Conditional(condition)
        }
    };
    PropertyDefinition::new(key, wire.description, requirement, multiplicity)
        .map_err(|error| format!("{location}: {error}"))
}

pub(crate) fn legacy_shape(contents: &str) -> bool {
    toml::from_str::<toml::Value>(contents).is_ok_and(|value| {
        value.get("templates").is_some()
            || value.get("typesets").is_some()
            || value.get("active-template").is_some()
    })
}

pub(crate) fn configuration_error(error: ConfigurationError) -> String {
    error.to_string()
}
