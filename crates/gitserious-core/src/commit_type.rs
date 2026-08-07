use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::{CommitTypeId, PropertyDefinition, PropertyKey, SchemaVersion};

/// A versioned semantic schema for one open commit type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitTypeDefinition {
    schema_version: SchemaVersion,
    id: CommitTypeId,
    description: Box<str>,
    properties: Vec<PropertyDefinition>,
}

impl CommitTypeDefinition {
    /// Creates a commit-type definition and enforces its structural invariants.
    ///
    /// # Errors
    ///
    /// Returns [`CommitTypeDefinitionError`] when the description is blank, no
    /// properties are supplied, or a property key is repeated.
    pub fn new(
        schema_version: SchemaVersion,
        id: CommitTypeId,
        description: impl Into<String>,
        properties: Vec<PropertyDefinition>,
    ) -> Result<Self, CommitTypeDefinitionError> {
        let description = description.into();
        if description.trim().is_empty() {
            return Err(CommitTypeDefinitionError::EmptyDescription);
        }
        if properties.is_empty() {
            return Err(CommitTypeDefinitionError::EmptyProperties);
        }

        let mut keys = BTreeSet::new();
        for property in &properties {
            if !keys.insert(property.key()) {
                return Err(CommitTypeDefinitionError::DuplicateProperty(
                    property.key().clone(),
                ));
            }
        }

        Ok(Self {
            schema_version,
            id,
            description: description.into_boxed_str(),
            properties,
        })
    }

    /// Returns the semantic-schema version.
    #[must_use]
    pub const fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    /// Returns the open commit-type identifier.
    #[must_use]
    pub const fn id(&self) -> &CommitTypeId {
        &self.id
    }

    /// Returns the commit type's semantic purpose.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Returns ordered property definitions.
    #[must_use]
    pub fn properties(&self) -> &[PropertyDefinition] {
        &self.properties
    }

    pub(crate) fn from_trusted(
        id: CommitTypeId,
        description: &'static str,
        properties: Vec<PropertyDefinition>,
    ) -> Self {
        Self {
            schema_version: SchemaVersion::V1,
            id,
            description: Box::from(description),
            properties,
        }
    }
}

/// A structural invariant violation in a commit-type definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitTypeDefinitionError {
    /// The commit type has no semantic description.
    EmptyDescription,
    /// The commit type defines no durable properties.
    EmptyProperties,
    /// Two properties use the same key.
    DuplicateProperty(PropertyKey),
}

impl Display for CommitTypeDefinitionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyDescription => formatter
                .write_str("commit type definition description must contain non-whitespace text"),
            Self::EmptyProperties => {
                formatter.write_str("commit type definition must contain at least one property")
            }
            Self::DuplicateProperty(key) => {
                write!(formatter, "commit type definition repeats property {key:?}")
            }
        }
    }
}

impl Error for CommitTypeDefinitionError {}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use crate::{
        CommitTypeId, PropertyDefinition, PropertyKey, PropertyMultiplicity, PropertyRequirement,
        SchemaVersion,
    };

    use super::{CommitTypeDefinition, CommitTypeDefinitionError};

    fn property(key: &str) -> Result<PropertyDefinition, Box<dyn Error>> {
        Ok(PropertyDefinition::new(
            PropertyKey::new(key)?,
            format!("Description for {key}."),
            PropertyRequirement::Required,
            PropertyMultiplicity::Single,
        )?)
    }

    #[test]
    fn definitions_preserve_version_identity_description_and_property_order()
    -> Result<(), Box<dyn Error>> {
        let definition = CommitTypeDefinition::new(
            SchemaVersion::new(2)?,
            CommitTypeId::new("custom-type")?,
            "A custom semantic contract.",
            vec![property("first")?, property("second")?],
        )?;

        assert_eq!(definition.schema_version().get(), 2);
        assert_eq!(definition.id().as_str(), "custom-type");
        assert_eq!(definition.description(), "A custom semantic contract.");
        assert_eq!(
            definition
                .properties()
                .iter()
                .map(|definition| definition.key().as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert_eq!(definition.clone(), definition);

        Ok(())
    }

    #[test]
    fn definitions_reject_blank_descriptions() -> Result<(), Box<dyn Error>> {
        let result = CommitTypeDefinition::new(
            SchemaVersion::V1,
            CommitTypeId::new("custom")?,
            " \n",
            vec![property("intent")?],
        );

        assert_eq!(result, Err(CommitTypeDefinitionError::EmptyDescription));
        assert!(
            !CommitTypeDefinitionError::EmptyDescription
                .to_string()
                .is_empty()
        );

        Ok(())
    }

    #[test]
    fn definitions_reject_empty_property_sets() -> Result<(), Box<dyn Error>> {
        let result = CommitTypeDefinition::new(
            SchemaVersion::V1,
            CommitTypeId::new("custom")?,
            "A custom contract.",
            Vec::new(),
        );

        assert_eq!(result, Err(CommitTypeDefinitionError::EmptyProperties));
        assert!(
            !CommitTypeDefinitionError::EmptyProperties
                .to_string()
                .is_empty()
        );

        Ok(())
    }

    #[test]
    fn definitions_reject_duplicate_property_keys() -> Result<(), Box<dyn Error>> {
        let duplicate_key = PropertyKey::new("intent")?;
        let result = CommitTypeDefinition::new(
            SchemaVersion::V1,
            CommitTypeId::new("custom")?,
            "A custom contract.",
            vec![property("intent")?, property("intent")?],
        );

        assert_eq!(
            result,
            Err(CommitTypeDefinitionError::DuplicateProperty(
                duplicate_key.clone()
            ))
        );
        assert!(
            CommitTypeDefinitionError::DuplicateProperty(duplicate_key)
                .to_string()
                .contains("intent")
        );

        Ok(())
    }

    #[test]
    fn the_same_property_key_can_belong_to_distinct_commit_types() -> Result<(), Box<dyn Error>> {
        let first = CommitTypeDefinition::new(
            SchemaVersion::V1,
            CommitTypeId::new("first")?,
            "First contract.",
            vec![property("change")?],
        );
        let second = CommitTypeDefinition::new(
            SchemaVersion::V1,
            CommitTypeId::new("second")?,
            "Second contract.",
            vec![property("change")?],
        );

        assert!(first.is_ok());
        assert!(second.is_ok());

        Ok(())
    }
}
