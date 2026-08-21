use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::{
    ChangeTypeId, Description, PropertyDefinition, PropertyKey, TaxonomyId, TypesetId,
    TypesetVersion,
};

/// The ordered durable-property contract for one taxonomy change type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeTypeSchema {
    change_type: ChangeTypeId,
    properties: Vec<PropertyDefinition>,
}

impl ChangeTypeSchema {
    /// Creates a property schema, permitting an explicitly empty property set.
    ///
    /// # Errors
    ///
    /// Returns [`ChangeTypeSchemaError`] when a property key is repeated.
    pub fn new(
        change_type: ChangeTypeId,
        properties: Vec<PropertyDefinition>,
    ) -> Result<Self, ChangeTypeSchemaError> {
        let mut keys = BTreeSet::new();
        for property in &properties {
            if !keys.insert(property.key()) {
                return Err(ChangeTypeSchemaError::DuplicateProperty(
                    property.key().clone(),
                ));
            }
        }
        Ok(Self {
            change_type,
            properties,
        })
    }

    /// Returns the taxonomy-scoped change-type identifier.
    #[must_use]
    pub const fn change_type(&self) -> &ChangeTypeId {
        &self.change_type
    }

    /// Returns durable properties in canonical message order.
    #[must_use]
    pub fn properties(&self) -> &[PropertyDefinition] {
        &self.properties
    }

    pub(crate) fn from_trusted(
        change_type: ChangeTypeId,
        properties: Vec<PropertyDefinition>,
    ) -> Self {
        Self {
            change_type,
            properties,
        }
    }
}

/// A duplicate property in one change-type schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChangeTypeSchemaError {
    /// A durable-property key appears more than once.
    DuplicateProperty(PropertyKey),
}

impl Display for ChangeTypeSchemaError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateProperty(key) => {
                write!(formatter, "change-type schema repeats property {key:?}")
            }
        }
    }
}

impl Error for ChangeTypeSchemaError {}

/// A versioned durable-property schema set attached to one taxonomy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypesetDefinition {
    taxonomy: TaxonomyId,
    id: TypesetId,
    version: TypesetVersion,
    description: Description,
    schemas: Vec<ChangeTypeSchema>,
}

impl TypesetDefinition {
    /// Creates a typeset and enforces its local structural invariants.
    ///
    /// Coverage against the referenced taxonomy is enforced when resolving a
    /// template.
    ///
    /// # Errors
    ///
    /// Returns [`TypesetDefinitionError`] when no schemas are supplied or a
    /// change-type schema is repeated.
    pub fn new(
        taxonomy: TaxonomyId,
        id: TypesetId,
        version: TypesetVersion,
        description: Description,
        schemas: Vec<ChangeTypeSchema>,
    ) -> Result<Self, TypesetDefinitionError> {
        if schemas.is_empty() {
            return Err(TypesetDefinitionError::EmptySchemas);
        }
        let mut ids = BTreeSet::new();
        for schema in &schemas {
            if !ids.insert(schema.change_type()) {
                return Err(TypesetDefinitionError::DuplicateChangeType(
                    schema.change_type().clone(),
                ));
            }
        }
        Ok(Self {
            taxonomy,
            id,
            version,
            description,
            schemas,
        })
    }

    /// Returns the taxonomy this typeset describes.
    #[must_use]
    pub const fn taxonomy(&self) -> &TaxonomyId {
        &self.taxonomy
    }

    /// Returns the identifier scoped to the referenced taxonomy.
    #[must_use]
    pub const fn id(&self) -> &TypesetId {
        &self.id
    }

    /// Returns the typeset's semantic version.
    #[must_use]
    pub const fn version(&self) -> TypesetVersion {
        self.version
    }

    /// Returns the typeset's purpose.
    #[must_use]
    pub const fn description(&self) -> &Description {
        &self.description
    }

    /// Returns the explicitly covered change-type schemas.
    #[must_use]
    pub fn schemas(&self) -> &[ChangeTypeSchema] {
        &self.schemas
    }

    pub(crate) fn from_trusted(
        taxonomy: TaxonomyId,
        id: TypesetId,
        version: TypesetVersion,
        description: Description,
        schemas: Vec<ChangeTypeSchema>,
    ) -> Self {
        Self {
            taxonomy,
            id,
            version,
            description,
            schemas,
        }
    }
}

/// A structural typeset-definition failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypesetDefinitionError {
    /// No change-type schemas were supplied.
    EmptySchemas,
    /// A change type appears more than once.
    DuplicateChangeType(ChangeTypeId),
}

impl Display for TypesetDefinitionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySchemas => {
                formatter.write_str("typeset must explicitly cover at least one change type")
            }
            Self::DuplicateChangeType(id) => {
                write!(formatter, "typeset repeats change type {id:?}")
            }
        }
    }
}

impl Error for TypesetDefinitionError {}
