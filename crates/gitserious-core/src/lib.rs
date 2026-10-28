//! Domain types for durable, type-specific commit-message properties.

mod built_in;
mod built_in_domain;
mod built_in_infra_ops;
mod built_in_ml_research;
mod commit_draft;
mod commit_message;
mod commit_type;
mod configured_taxonomy;
mod description;
mod fingerprint;
mod identifier;
mod property;
mod property_validation;
mod schema_version;
mod taxonomy_provenance;
mod taxonomy_version;

pub use built_in::built_in_commit_types;
pub use commit_draft::{
    AuthoredProperty, CommitDraft, CommitDraftError, CommitScope, CommitScopeError, CommitSubject,
    CommitSubjectError,
};
pub use commit_message::{
    COMMIT_MESSAGE_WIDTH, CommitMessage, CommitValidationError, CommitValidationErrors,
    CommitValidationReport, validate_commit_draft, validate_commit_draft_with_provenance,
};
pub use commit_type::{CommitTypeDefinition, CommitTypeDefinitionError};
pub use configured_taxonomy::{Taxonomy, TaxonomyError, TaxonomyLineage, built_in_taxonomies};
pub use description::{Description, DescriptionError};
pub use fingerprint::Fingerprint;
pub use identifier::{
    CommitTypeId, ConditionId, IdentifierError, LabelError, PropertyKey, TaxonomyId,
    normalize_identifier,
};
pub use property::{
    PropertyCondition, PropertyConditionError, PropertyDefinition, PropertyDefinitionError,
    PropertyRequirement, PropertyValue, PropertyValueError,
};
pub use property_validation::{
    ConditionalApplicability, PropertyResponse, PropertyValidationIssue,
    PropertyValidationIssueKind, ValidationSeverity,
};
pub use schema_version::SchemaVersion;
pub use taxonomy_provenance::CommitProvenance;
pub use taxonomy_version::{TaxonomyVersion, TaxonomyVersionError};
