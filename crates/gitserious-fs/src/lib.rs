//! Filesystem paths, Git worktree discovery, and local-storage adapters for gitserious.

mod directory;
mod git;
mod git_commit;
mod global;
mod platform;
mod project;

pub use directory::LocalDirectoryCreator;
pub use git::{GitRepositoryError, GitRepositoryLocator};
pub use git_commit::{GitCommitError, GitCommitWriter};
pub use global::{GlobalPathError, SystemGlobalPathResolver};
pub use project::{ProjectArtifact, ProjectStateError, TomlProjectStateStore};

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod tests;
