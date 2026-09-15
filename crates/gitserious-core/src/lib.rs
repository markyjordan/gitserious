//! Domain types for durable, type-specific commit-message properties.

mod built_in;
#[allow(dead_code)]
mod built_in_configuration;
#[allow(dead_code)]
mod built_in_domain;
#[allow(dead_code)]
mod built_in_infra_ops;
#[allow(dead_code)]
mod built_in_ml_research;
// Legacy configurable message-template renderer retained in built_in_template.rs.
// mod built_in_template;
mod commit_draft;
mod commit_message;
// Legacy selectable-template provenance retained in commit_provenance.rs.
// mod commit_provenance;
mod commit_type;
#[allow(dead_code)]
mod configuration_template;
mod configured_taxonomy;
mod description;
mod fingerprint;
mod identifier;
mod property;
mod property_validation;
#[allow(dead_code)]
mod resolved_taxonomy;
mod schema_version;
#[allow(dead_code)]
mod taxonomy;
mod taxonomy_provenance;
mod taxonomy_version;
// Legacy configurable message-template renderer retained in template.rs.
// mod template;
#[allow(dead_code)]
mod template_version;
#[allow(dead_code)]
mod typeset;
#[allow(dead_code)]
mod typeset_version;

pub use built_in::built_in_commit_types;
// Legacy template/typeset catalog exports retained privately for the bridge.
#[allow(unused_imports)]
pub(crate) use built_in_configuration::{BuiltInConfiguration, built_in_configuration};
pub use commit_draft::{
    AuthoredProperty, CommitDraft, CommitDraftError, CommitScope, CommitScopeError, CommitSubject,
    CommitSubjectError,
};
pub use commit_message::{
    COMMIT_MESSAGE_WIDTH, CommitMessage, CommitValidationError, CommitValidationErrors,
    CommitValidationReport, render_commit_message, render_commit_message_with_provenance,
    validate_commit_draft, validate_commit_draft_report,
};
// Legacy configuration provenance remains in commit_provenance.rs for manual inspection.
// pub use commit_provenance::CommitProvenance;
pub use commit_type::{CommitTypeDefinition, CommitTypeDefinitionError};
#[allow(unused_imports)]
pub(crate) use configuration_template::TemplateDefinition;
pub use configured_taxonomy::{Taxonomy, TaxonomyError, TaxonomyLineage, built_in_taxonomies};
pub use description::{Description, DescriptionError};
pub use fingerprint::{Fingerprint, FingerprintError};
pub(crate) use identifier::{ChangeTypeId, TemplateId, TypesetId};
pub use identifier::{
    CommitTypeId, ConditionId, IdentifierError, IdentifierErrorKind, PropertyKey, TaxonomyId,
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
#[allow(unused_imports)]
pub(crate) use resolved_taxonomy::{ResolveTaxonomyError, ResolvedChangeType, ResolvedTaxonomy};
pub use schema_version::{SchemaVersion, SchemaVersionError};
#[allow(unused_imports)]
pub(crate) use taxonomy::{ChangeTypeDefinition, TaxonomyDefinition, TaxonomyDefinitionError};
pub use taxonomy_provenance::CommitProvenance;
pub use taxonomy_version::{TaxonomyVersion, TaxonomyVersionError};
#[allow(unused_imports)]
pub(crate) use template_version::{TemplateVersion, TemplateVersionError};
#[allow(unused_imports)]
pub(crate) use typeset::{
    ChangeTypeSchema, ChangeTypeSchemaError, TypesetDefinition, TypesetDefinitionError,
};
#[allow(unused_imports)]
pub(crate) use typeset_version::{TypesetVersion, TypesetVersionError};
