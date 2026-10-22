//! Filesystem paths, Git worktree discovery, and local-storage adapters for gitserious.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::needless_pass_by_value)]

mod directory;
mod git;
mod git_commit;
mod git_status;
mod global;
mod platform;
mod taxonomy_global;
mod taxonomy_project;
mod taxonomy_wire;

pub use directory::LocalDirectoryCreator;
pub use git::{GitRepositoryError, GitRepositoryLocator};
pub use git_commit::{GitCommitError, GitCommitWriter};
pub use global::{GlobalPathError, SystemGlobalPathResolver};
pub use taxonomy_global::{GlobalConfigurationError, TomlGlobalConfigurationStore};
pub use taxonomy_project::{ProjectStateError, TomlProjectStateStore};

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod tests;

pub use git_status::GitRepositoryStatusReader;
