//! Application ports and use cases for gitserious.

#![allow(clippy::missing_errors_doc)]

mod commit_draft_author;
mod commit_type_catalog;
mod commit_writer;
mod create_taxonomy_commit;
mod directory_creator;
mod ensure_storage_directory;
mod find_commit_type;
mod global_path_resolver;
mod global_paths;
mod initialize_taxonomy_project;
mod interactive;
mod list_commit_types;
mod repository_locator;
mod repository_status;
mod resolve_global_paths;
mod storage_directory;
mod taxonomy_authoring_context;
mod taxonomy_changes;
mod taxonomy_configuration_editor;
mod taxonomy_policy;
mod taxonomy_ports;

pub use commit_draft_author::{CommitDraftAuthor, CommitDraftAuthorOutcome};
pub use commit_type_catalog::CommitTypeCatalog;
pub use commit_writer::{CommitOutput, CommitWriter};
pub use create_taxonomy_commit::{
    CommitOutcome, CommitPolicyError, CommitPreparationResult, CreateCommitError,
    CreateCommitResult, create_commit, create_commit_with_taxonomy, prepare_commit_with_taxonomy,
    validate_reviewed_commit,
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
pub use taxonomy_authoring_context::{
    AuthoredCommit, CommitAuthoringContext, CommitAuthoringOutcome, CommitTaxonomy,
};
pub use taxonomy_configuration_editor::{
    ConfigurationDestination, ConfigurationEditor, ConfigurationSession, ConfigurationWorkspace,
    edit_configuration,
};
pub use taxonomy_policy::{
    ConfigurationError, EffectiveConfiguration, GLOBAL_CONFIG_VERSION, GlobalConfiguration,
    PROJECT_CONFIG_VERSION, PROJECT_LOCK_VERSION, ProjectConfig, ProjectLock, ProjectState,
    TaxonomyCatalog, TaxonomyOrigin, fingerprint_project_config, fingerprint_taxonomy,
    resolve_effective_configuration,
};
pub use taxonomy_ports::{GlobalConfigurationStore, ProjectStateStore};

pub use interactive::{
    HelpPage, InteractiveEntry, InteractiveUi, InteractiveWorkspace, run_interactive,
};
pub use repository_status::{BranchState, RepositoryStatus, RepositoryStatusReader, StatusEntry};
