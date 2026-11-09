//! Application ports and use cases for gitserious.

#![allow(clippy::missing_errors_doc)]

#[cfg(test)]
extern crate self as gitserious_app;

mod commit_draft_author;
mod commit_writer;
mod config;
mod create_taxonomy_commit;
mod directory_creator;
mod global_path_resolver;
mod global_paths;
mod initialize_taxonomy_project;
mod interactive;
mod repository_locator;
mod repository_status;
mod resolve_global_paths;
mod storage_directory;
mod taxonomy_authoring_context;

pub use commit_draft_author::{CommitDraftAuthor, CommitDraftAuthorOutcome};
pub use commit_writer::{CommitOutput, CommitWriter};
pub use config::{
    Collision, ConfigurationDestination, ConfigurationEditor, ConfigurationError,
    ConfigurationSession, ConfigurationWorkspace, CopyIn, DraftEdit, DraftField, EditRefusal,
    GlobalConfiguration, GlobalConfigurationStore, LibraryDraft, LibraryTaxonomy, NodeId, NodeRef,
    Policy, Problem, ProblemKind, ProblemSite, ProjectConfiguration, ProjectConfigurationStore,
    ProjectRevision, PropertyNode, Review, StoredProject, TaxonomyCatalog, TaxonomyDate,
    TaxonomyNode, TaxonomyOrigin, TypeNode, edit_configuration, fingerprint_taxonomy,
};
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
pub use interactive::{
    HelpLine, HelpPage, HelpSpan, HomeState, InteractiveEntry, InteractiveUi, InteractiveWorkspace,
    run_interactive,
};
pub use repository_locator::{
    RepositoryInitializer, RepositoryLocator, RepositoryRoot, RepositoryRootError,
};
pub use repository_status::{BranchState, RepositoryStatus, RepositoryStatusReader, StatusEntry};
pub use resolve_global_paths::resolve_global_paths;
pub use storage_directory::StorageDirectory;
pub use taxonomy_authoring_context::{
    AuthoredCommit, CommitAuthoringContext, CommitAuthoringOutcome, CommitTaxonomy,
};
