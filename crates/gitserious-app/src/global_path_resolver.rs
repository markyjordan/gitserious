use crate::GlobalPaths;

/// Resolves the effective user-scoped storage directories.
pub trait GlobalPathResolver {
    /// The adapter-specific failure returned by path resolution.
    type Error;

    /// Resolves one owned snapshot of global storage paths.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when the platform cannot provide
    /// the required storage locations.
    fn resolve(&self) -> Result<GlobalPaths, Self::Error>;
}
