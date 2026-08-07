//! Git worktree discovery and repository-local persistence adapters.

mod git;
mod project;

pub use git::{GitRepositoryError, GitRepositoryLocator};
pub use project::{ProjectArtifact, ProjectStateError, TomlProjectStateStore};
