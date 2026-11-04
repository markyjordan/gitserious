//! Application ports and use cases for gitserious.

#![allow(clippy::missing_errors_doc)]

#[cfg(test)]
extern crate self as gitserious_app;

mod commit_draft_author;
mod commit_writer;
mod create_taxonomy_commit;
mod directory_creator;
mod global_path_resolver;
mod global_paths;
mod initialize_taxonomy_project;
mod interactive;
mod library_draft;
mod library_taxonomy;
pub use library_taxonomy::LibraryTaxonomy;
mod repository_locator;
mod repository_status;
mod resolve_global_paths;
mod storage_directory;
mod taxonomy_authoring_context;
mod taxonomy_configuration_editor;
mod taxonomy_policy;
mod taxonomy_ports;
mod verify_project_configuration;

pub use commit_draft_author::{CommitDraftAuthor, CommitDraftAuthorOutcome};
pub use commit_writer::{CommitOutput, CommitWriter};
pub use create_taxonomy_commit::{
    CommitOutcome, CreateCommitError, create_commit_with_taxonomy, prepare_commit_with_taxonomy,
    validate_reviewed_commit,
};
pub use directory_creator::DirectoryCreator;
pub use global_path_resolver::GlobalPathResolver;
pub use global_paths::GlobalPaths;
pub use initialize_taxonomy_project::{
    InitOutcome, InitStatus, InitializeProjectError, initial_project_configuration,
    initialize_project,
};
pub use repository_locator::{RepositoryLocator, RepositoryRoot, RepositoryRootError};
pub use resolve_global_paths::resolve_global_paths;
pub use storage_directory::StorageDirectory;
pub use taxonomy_authoring_context::{
    AuthoredCommit, CommitAuthoringContext, CommitAuthoringOutcome, CommitTaxonomy,
};
pub use taxonomy_configuration_editor::{
    ConfigurationDestination, ConfigurationEditor, ConfigurationSession, ConfigurationWorkspace,
    Review, edit_configuration,
};
pub use taxonomy_policy::{
    ConfigurationError, GlobalConfiguration, ProjectConfiguration, ProjectRevision, StoredProject,
    TaxonomyCatalog, TaxonomyOrigin, fingerprint_project_configuration, fingerprint_taxonomy,
};
pub use taxonomy_ports::{GlobalConfigurationStore, ProjectConfigurationStore};
pub use verify_project_configuration::{
    ProjectVerification, VerifyProjectError, verify_project_configuration,
};

pub use interactive::{
    HelpPage, InteractiveEntry, InteractiveUi, InteractiveWorkspace, run_interactive,
};
pub use library_draft::{
    DraftEdit, DraftField, EditRefusal, LibraryDraft, NodeId, NodeRef, Policy, Problem,
    ProblemKind, ProblemSite, PropertyNode, TaxonomyNode, TypeNode,
};
pub use repository_status::{BranchState, RepositoryStatus, RepositoryStatusReader, StatusEntry};
