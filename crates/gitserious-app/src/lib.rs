//! Application ports and use cases for gitserious.

mod commit_type_catalog;
mod find_commit_type;
mod fingerprint;
mod initialize_project;
mod list_commit_types;
mod project_config;
mod project_lock;
mod project_state;
mod project_state_store;
mod repository_locator;

pub use commit_type_catalog::CommitTypeCatalog;
pub use find_commit_type::find_commit_type;
pub use fingerprint::{Fingerprint, FingerprintError};
pub use initialize_project::{InitOutcome, InitStatus, InitializeProjectError, initialize_project};
pub use list_commit_types::list_commit_types;
pub use project_config::{PROJECT_CONFIG_VERSION, ProjectConfig, ProjectConfigError};
pub use project_lock::{
    PROJECT_LOCK_VERSION, ProjectLock, ProjectLockError, ResolveProjectPolicyError,
    ResolvedCommitType, ResolvedTemplate, ResolvedTemplateError,
    fingerprint_commit_message_template, fingerprint_commit_type_definition,
    fingerprint_project_config, resolve_project_lock,
};
pub use project_state::ProjectState;
pub use project_state_store::ProjectStateStore;
pub use repository_locator::{RepositoryLocator, RepositoryRoot, RepositoryRootError};
