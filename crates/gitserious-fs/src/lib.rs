//! Filesystem paths, Git worktree discovery, and local-storage adapters for gitserious.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::needless_pass_by_value)]

mod directory;
mod git;
mod git_commit;
mod global;
// Legacy template/typeset persistence retained in global_configuration.rs.
// mod global_configuration;
mod platform;
// Legacy project persistence retained in project.rs.
// mod project;
mod taxonomy_global;
mod taxonomy_project;
mod taxonomy_wire;

pub use directory::LocalDirectoryCreator;
pub use git::{GitRepositoryError, GitRepositoryLocator};
pub use git_commit::{GitCommitError, GitCommitWriter};
pub use global::{GlobalPathError, SystemGlobalPathResolver};
pub use taxonomy_global::{GlobalConfigurationError, TomlGlobalConfigurationStore};
pub use taxonomy_project::{ProjectStateError, TomlProjectStateStore};

// Legacy filesystem unit-test registration retained for manual verification.
// #[cfg(test)]
// #[path = "../tests/unit/mod.rs"]
// mod tests;
