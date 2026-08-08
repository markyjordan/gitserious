use std::convert::Infallible;

use gitserious_app::CommitTypeCatalog;
use gitserious_core::{CommitTypeDefinition, CommitTypeId, built_in_commit_types};

/// Immutable adapter over the commit types compiled into this binary.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BuiltInCommitTypeCatalog;

impl CommitTypeCatalog for BuiltInCommitTypeCatalog {
    type Error = Infallible;

    fn find(&self, id: &CommitTypeId) -> Result<Option<CommitTypeDefinition>, Self::Error> {
        Ok(built_in_commit_types()
            .iter()
            .find(|definition| definition.id() == id)
            .cloned())
    }

    fn list(&self) -> Result<Vec<CommitTypeDefinition>, Self::Error> {
        Ok(built_in_commit_types().to_vec())
    }
}
