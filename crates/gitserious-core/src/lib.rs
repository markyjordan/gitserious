//! Domain types for durable, type-specific commit-message properties.

mod built_in;
mod built_in_configuration;
mod built_in_template;
mod commit_draft;
mod commit_message;
mod commit_type;
mod configuration_template;
mod description;
mod identifier;
mod property;
mod property_validation;
mod resolved_taxonomy;
mod schema_version;
mod taxonomy;
mod taxonomy_version;
mod template;
mod template_version;
mod typeset;
mod typeset_version;

pub use built_in::built_in_commit_types;
pub use built_in_configuration::{BuiltInConfiguration, built_in_configuration};
pub use built_in_template::default_commit_message_template;
pub use commit_draft::{
    AuthoredProperty, CommitDraft, CommitDraftError, CommitScope, CommitScopeError, CommitSubject,
    CommitSubjectError,
};
pub use commit_message::{
    COMMIT_MESSAGE_WIDTH, CommitMessage, CommitValidationError, CommitValidationErrors,
    CommitValidationReport, render_commit_message, validate_commit_draft,
    validate_commit_draft_report,
};
pub use commit_type::{CommitTypeDefinition, CommitTypeDefinitionError};
pub use configuration_template::TemplateDefinition;
pub use description::{Description, DescriptionError};
pub use identifier::{
    ChangeTypeId, CommitTypeId, ConditionId, IdentifierError, IdentifierErrorKind, PropertyKey,
    TaxonomyId, TemplateId, TypesetId,
};
pub use property::{
    PropertyCondition, PropertyConditionError, PropertyDefinition, PropertyDefinitionError,
    PropertyMultiplicity, PropertyRequirement, PropertyValue, PropertyValueError, PropertyValues,
    PropertyValuesError,
};
pub use property_validation::{
    ConditionalApplicability, PropertyResponse, PropertyValidationIssue,
    PropertyValidationIssueKind, PropertyValidationReport, ValidationSeverity,
    validate_property_responses,
};
pub use resolved_taxonomy::{ResolveTaxonomyError, ResolvedChangeType, ResolvedTaxonomy};
pub use schema_version::{SchemaVersion, SchemaVersionError};
pub use taxonomy::{ChangeTypeDefinition, TaxonomyDefinition, TaxonomyDefinitionError};
pub use taxonomy_version::{TaxonomyVersion, TaxonomyVersionError};
pub use template::{CommitMessageTemplateDefinition, CommitMessageTemplateDefinitionError};
pub use template_version::{TemplateVersion, TemplateVersionError};
pub use typeset::{
    ChangeTypeSchema, ChangeTypeSchemaError, TypesetDefinition, TypesetDefinitionError,
};
pub use typeset_version::{TypesetVersion, TypesetVersionError};
