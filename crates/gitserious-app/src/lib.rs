//! Application ports and use cases for gitserious.

#![allow(clippy::missing_errors_doc)]

mod commit;
mod config;
mod directory_creator;
mod global_path_resolver;
mod global_paths;
mod project;
mod repository_locator;
mod resolve_global_paths;
mod storage_directory;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use commit::{
    AuthoredCommit, BranchState, CommitAuthoringContext, CommitOutput, CommitServices,
    CommitTaxonomy, CommitWorkspace, CommitWriter, RepositoryStatus, RepositoryStatusReader,
    StatusEntry, WorktreePaths, prepare_commit_with_taxonomy, validate_reviewed_commit,
};
pub use config::{
    ConfigurationError, ConfigurationServices, ConfigurationWorkspace, DraftEdit, DraftField,
    EditRefusal, GlobalConfiguration, GlobalConfigurationStore, LibraryDraft, LibraryTaxonomy,
    NodeId, NodeRef, Policy, Problem, ProblemKind, ProblemSite, ProjectConfiguration,
    ProjectConfigurationStore, ProjectRevision, ProjectSnapshot, PropertyNode, StoredProject,
    TaxonomyDate, TaxonomyNode, TaxonomyOrigin, TypeNode, fingerprint_taxonomy,
};
pub use directory_creator::DirectoryCreator;
pub use global_path_resolver::GlobalPathResolver;
pub use global_paths::GlobalPaths;
pub use project::{
    InitOutcome, InitStatus, InitializeProjectError, ProjectOverview, ProjectServices,
    ProjectWorkspace, initialize_project,
};
pub use repository_locator::{
    RepositoryInitializer, RepositoryLocator, RepositoryRoot, RepositoryRootError,
};
pub use resolve_global_paths::resolve_global_paths;
pub use storage_directory::StorageDirectory;
