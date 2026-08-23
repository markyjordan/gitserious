use crate::{ProjectConfig, ProjectLock, ProjectState, RepositoryRoot};

/// Persists the repository-local project configuration aggregate.
pub trait ProjectStateStore {
    /// The adapter-specific persistence failure.
    type Error;

    /// Reads the known project files without modifying them.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when state cannot be inspected or
    /// parsed safely.
    fn inspect(&self, root: &RepositoryRoot) -> Result<ProjectState, Self::Error>;

    /// Ensures the ignored repository-local state directory is available.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when the local state directory or
    /// its ignore marker cannot be inspected or prepared safely.
    fn ensure_local_state(&self, root: &RepositoryRoot) -> Result<(), Self::Error>;

    /// Creates authored configuration and its lock without overwriting either.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when exclusive creation cannot be
    /// completed safely.
    fn initialize(
        &self,
        root: &RepositoryRoot,
        config: &ProjectConfig,
        lock: &ProjectLock,
    ) -> Result<(), Self::Error>;

    /// Creates a missing generated lock without modifying authored config.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when exclusive creation fails.
    fn create_lock(&self, root: &RepositoryRoot, lock: &ProjectLock) -> Result<(), Self::Error>;

    /// Atomically replaces a recognized generated lock.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when replacement cannot complete
    /// safely.
    fn replace_lock(
        &self,
        root: &RepositoryRoot,
        current: &ProjectLock,
        replacement: &ProjectLock,
    ) -> Result<(), Self::Error>;

    /// Replaces an observed project configuration and lock as one guarded pair.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when either observed artifact has
    /// changed or the staged replacement cannot be installed safely.
    fn compare_and_swap(
        &self,
        root: &RepositoryRoot,
        current_config: &ProjectConfig,
        current_lock: &ProjectLock,
        replacement_config: &ProjectConfig,
        replacement_lock: &ProjectLock,
    ) -> Result<(), Self::Error>;
}
