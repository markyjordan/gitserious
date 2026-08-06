//! Domain types for durable, type-specific commit-message properties.

mod built_in;
mod commit_type;
mod identifier;
mod property;
mod schema_version;

pub use built_in::built_in_commit_types;
pub use commit_type::{CommitTypeDefinition, CommitTypeDefinitionError};
pub use identifier::{
    CommitTypeId, ConditionId, IdentifierError, IdentifierErrorKind, PropertyKey,
};
pub use property::{
    PropertyCondition, PropertyConditionError, PropertyDefinition, PropertyDefinitionError,
    PropertyMultiplicity, PropertyRequirement, PropertyValue, PropertyValueError, PropertyValues,
    PropertyValuesError,
};
pub use schema_version::{SchemaVersion, SchemaVersionError};
