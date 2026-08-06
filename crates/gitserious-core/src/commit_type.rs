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
