use gitserious_core::{CommitTypeDefinition, CommitTypeId};

use crate::CommitTypeCatalog;

/// Finds one commit-type definition without imposing presentation behavior.
///
/// # Errors
///
/// Returns the catalog adapter's error unchanged when lookup fails.
pub fn find_commit_type<C>(
    catalog: &C,
    id: &CommitTypeId,
) -> Result<Option<CommitTypeDefinition>, C::Error>
where
    C: CommitTypeCatalog + ?Sized,
{
    catalog.find(id)
}
