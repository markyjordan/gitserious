//! Application ports and use cases for gitserious.

#![allow(clippy::missing_errors_doc)]

// Legacy template-based authoring retained in commit_authoring_context.rs.
// mod commit_authoring_context;
mod commit_draft_author;
mod commit_type_catalog;
mod commit_writer;
// Legacy configuration modules remain in place for manual verification.
// mod configuration_catalog;
// mod configuration_crud;
// mod configuration_editor;
// mod create_commit;
// mod custom_configuration;
mod directory_creator;
// mod effective_catalog;
mod ensure_storage_directory;
mod find_commit_type;
// mod fork_configuration;
// mod global_configuration_store;
mod global_path_resolver;
mod global_paths;
// mod initialize_project;
mod list_commit_types;
// mod project_config;
// mod project_configuration;
// mod project_lock;
// mod project_state;
// mod project_state_store;
mod create_taxonomy_commit;
mod initialize_taxonomy_project;
mod repository_locator;
mod resolve_global_paths;
mod storage_directory;
mod taxonomy_authoring_context;
mod taxonomy_configuration_editor;
mod taxonomy_policy;
mod taxonomy_ports;

// pub use commit_authoring_context::{...};
pub use commit_draft_author::{CommitDraftAuthor, CommitDraftAuthorOutcome};
pub use commit_type_catalog::CommitTypeCatalog;
pub use commit_writer::{CommitOutput, CommitWriter};
pub use taxonomy_authoring_context::{
    AuthoredCommit, CommitAuthoringContext, CommitAuthoringOutcome, CommitTaxonomy,
};
// Legacy catalog/CRUD/editor exports intentionally disabled.
pub use create_taxonomy_commit::{
    CommitOutcome, CommitPolicyError, CreateCommitError, CreateCommitResult, create_commit,
    create_commit_with_taxonomy,
};
pub use directory_creator::DirectoryCreator;
pub use ensure_storage_directory::ensure_storage_directory;
pub use find_commit_type::find_commit_type;
pub use gitserious_core::{Fingerprint, FingerprintError};
pub use global_path_resolver::GlobalPathResolver;
pub use global_paths::GlobalPaths;
pub use initialize_taxonomy_project::{
    InitOutcome, InitStatus, InitializeProjectError, initialize_project,
};
pub use list_commit_types::list_commit_types;
pub use repository_locator::{RepositoryLocator, RepositoryRoot, RepositoryRootError};
pub use resolve_global_paths::resolve_global_paths;
pub use storage_directory::StorageDirectory;
pub use taxonomy_configuration_editor::{
    ConfigurationDestination, ConfigurationEditor, ConfigurationSession, ConfigurationWorkspace,
    edit_configuration, revised_taxonomy,
};
pub use taxonomy_policy::{
    ConfigurationError, EffectiveConfiguration, GLOBAL_CONFIG_VERSION, GlobalConfiguration,
    PROJECT_CONFIG_VERSION, PROJECT_LOCK_VERSION, ProjectConfig, ProjectLock, ProjectState,
    TaxonomyCatalog, TaxonomyOrigin, fingerprint_project_config, fingerprint_taxonomy,
    resolve_effective_configuration,
};
pub use taxonomy_ports::{GlobalConfigurationStore, ProjectStateStore};
