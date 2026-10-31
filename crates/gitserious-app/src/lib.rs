//! Application ports and use cases for gitserious.

#![allow(clippy::missing_errors_doc)]

mod commit_draft_author;
mod commit_writer;
mod create_taxonomy_commit;
mod directory_creator;
mod global_path_resolver;
mod global_paths;
mod initialize_taxonomy_project;
mod interactive;
mod library_draft;
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
pub use commit_writer::{CommitOutput, CommitWriter};
pub use create_taxonomy_commit::{
    CommitOutcome, CreateCommitError, create_commit_with_taxonomy, prepare_commit_with_taxonomy,
    validate_reviewed_commit,
};
pub use directory_creator::DirectoryCreator;
pub use gitserious_core::Fingerprint;
pub use global_path_resolver::GlobalPathResolver;
pub use global_paths::GlobalPaths;
pub use initialize_taxonomy_project::{
    InitOutcome, InitStatus, InitializeProjectError, initialize_project,
};
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
    ConfigurationError, EffectiveConfiguration, GlobalConfiguration, ProjectConfig, ProjectLock,
    ProjectState, TaxonomyCatalog, TaxonomyOrigin, fingerprint_taxonomy,
    resolve_effective_configuration,
};
pub use taxonomy_ports::{GlobalConfigurationStore, ProjectStateStore};

pub use interactive::{
    HelpPage, InteractiveEntry, InteractiveUi, InteractiveWorkspace, run_interactive,
};
pub use library_draft::{
    DraftEdit, DraftField, EditRefusal, LibraryDraft, NodeId, NodeRef, Policy, Problem,
    ProblemKind, ProblemSite, PropertyNode, TaxonomyNode, TypeNode,
};
pub use repository_status::{BranchState, RepositoryStatus, RepositoryStatusReader, StatusEntry};
