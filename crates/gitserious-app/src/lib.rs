//! Application ports and use cases for gitserious.

mod commit_draft_author;
mod commit_type_catalog;
mod commit_writer;
mod configuration_catalog;
mod configuration_crud;
mod create_commit;
mod directory_creator;
mod ensure_storage_directory;
mod find_commit_type;
mod fingerprint;
mod global_path_resolver;
mod global_paths;
mod initialize_project;
mod list_commit_types;
mod project_config;
mod project_lock;
mod project_state;
mod project_state_store;
mod repository_locator;
mod resolve_global_paths;
mod storage_directory;
mod user_configuration;
mod user_configuration_store;

pub use commit_draft_author::{CommitDraftAuthor, CommitDraftAuthorOutcome};
pub use commit_type_catalog::CommitTypeCatalog;
pub use commit_writer::{CommitOutput, CommitWriter};
pub use configuration_catalog::{ConfigurationCatalog, ConfigurationCatalogError};
pub use configuration_crud::{
    ConfigurationEdit, ConfigurationEntity, ConfigurationMutationError, apply_configuration_edits,
    create_taxonomy, create_template, create_typeset, delete_taxonomy, delete_template,
    delete_typeset, find_taxonomy, find_template, find_typeset, list_taxonomies, list_templates,
    list_typesets, update_taxonomy, update_template, update_typeset,
};
pub use create_commit::{
    CommitOutcome, CommitPolicyError, CreateCommitError, CreateCommitResult, create_commit,
};
pub use directory_creator::DirectoryCreator;
pub use ensure_storage_directory::ensure_storage_directory;
pub use find_commit_type::find_commit_type;
pub use fingerprint::{Fingerprint, FingerprintError};
pub use global_path_resolver::GlobalPathResolver;
pub use global_paths::GlobalPaths;
pub use initialize_project::{InitOutcome, InitStatus, InitializeProjectError, initialize_project};
pub use list_commit_types::list_commit_types;
pub use project_config::{PROJECT_CONFIG_VERSION, ProjectConfig, ProjectConfigError};
pub use project_lock::{
    PROJECT_LOCK_VERSION, ProjectLock, ProjectLockError, ResolveProjectPolicyError,
    ResolvedCommitType, ResolvedTemplate, ResolvedTemplateError,
    fingerprint_commit_message_template, fingerprint_commit_type_definition,
    fingerprint_project_config, fingerprint_resolved_taxonomy, resolve_project_lock,
};
pub use project_state::ProjectState;
pub use project_state_store::ProjectStateStore;
pub use repository_locator::{RepositoryLocator, RepositoryRoot, RepositoryRootError};
pub use resolve_global_paths::resolve_global_paths;
pub use storage_directory::StorageDirectory;
pub use user_configuration::{UserConfiguration, UserConfigurationError};
pub use user_configuration_store::UserConfigurationStore;
