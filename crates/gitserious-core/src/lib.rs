//! Domain types for durable, type-specific commit-message properties.

mod built_in;
mod built_in_template;
mod commit_draft;
mod commit_message;
mod commit_type;
mod identifier;
mod property;
mod schema_version;
mod template;
mod template_version;

pub use built_in::built_in_commit_types;
pub use built_in_template::default_commit_message_template;
pub use commit_draft::{
    AuthoredProperty, CommitDraft, CommitDraftError, CommitScope, CommitScopeError, CommitSubject,
    CommitSubjectError,
};
pub use commit_message::{
    COMMIT_MESSAGE_WIDTH, CommitMessage, CommitValidationError, CommitValidationErrors,
    render_commit_message, validate_commit_draft,
};
pub use commit_type::{CommitTypeDefinition, CommitTypeDefinitionError};
pub use identifier::{
    CommitTypeId, ConditionId, IdentifierError, IdentifierErrorKind, PropertyKey, TemplateId,
};
pub use property::{
    PropertyCondition, PropertyConditionError, PropertyDefinition, PropertyDefinitionError,
    PropertyMultiplicity, PropertyRequirement, PropertyValue, PropertyValueError, PropertyValues,
    PropertyValuesError,
};
pub use schema_version::{SchemaVersion, SchemaVersionError};
pub use template::{CommitMessageTemplateDefinition, CommitMessageTemplateDefinitionError};
pub use template_version::{TemplateVersion, TemplateVersionError};
