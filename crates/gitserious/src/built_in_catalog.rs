use std::convert::Infallible;

use gitserious_app::EffectiveDefinitions;
use gitserious_core::{CommitTypeDefinition, TemplateId, built_in_commit_types};

/// Immutable adapter over the commit types compiled into this binary.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BuiltInCommitTypeCatalog;

impl EffectiveDefinitions for BuiltInCommitTypeCatalog {
    type Error = Infallible;

    fn for_template(
        &self,
        _template: &TemplateId,
    ) -> Result<Vec<CommitTypeDefinition>, Self::Error> {
        Ok(built_in_commit_types().to_vec())
    }
}
