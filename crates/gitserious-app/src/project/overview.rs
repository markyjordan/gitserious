use std::path::PathBuf;

use gitserious_core::{TaxonomyId, TaxonomyVersion};

use crate::RepositoryRoot;

/// The Git repository and gitserious project that contain a directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectOverview {
    /// No Git worktree contains the directory.
    NoRepository { directory: PathBuf },
    /// A Git worktree without a gitserious project.
    Uninitialized { root: RepositoryRoot },
    /// A gitserious project and the taxonomy commits start from.
    Initialized {
        root: RepositoryRoot,
        default: TaxonomyId,
        version: Option<TaxonomyVersion>,
        /// Git does not track `gitserious.toml` yet, so the policy stays local.
        policy_untracked: bool,
    },
    /// Git or the project file could not be read.
    Unavailable { directory: PathBuf, error: String },
}
