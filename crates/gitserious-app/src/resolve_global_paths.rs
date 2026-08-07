use crate::{GlobalPathResolver, GlobalPaths};

/// Resolves global storage paths without imposing platform behavior.
///
/// # Errors
///
/// Returns the resolver adapter's error unchanged when resolution fails.
pub fn resolve_global_paths<R>(resolver: &R) -> Result<GlobalPaths, R::Error>
where
    R: GlobalPathResolver + ?Sized,
{
    resolver.resolve()
}
