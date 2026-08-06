use gitserious_core::CommitTypeDefinition;

use crate::CommitTypeCatalog;

/// Lists commit-type definitions without imposing presentation behavior.
///
/// # Errors
///
/// Returns the catalog adapter's error unchanged when listing fails.
pub fn list_commit_types<C>(catalog: &C) -> Result<Vec<CommitTypeDefinition>, C::Error>
where
    C: CommitTypeCatalog + ?Sized,
{
    catalog.list()
}
