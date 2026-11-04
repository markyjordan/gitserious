use crate::{
    GlobalConfiguration, ProjectConfig, ProjectConfiguration, ProjectLock, ProjectRevision,
    ProjectState, RepositoryRoot, StoredProject,
};

/// Persists one self-contained project configuration file.
pub trait ProjectConfigurationStore {
    /// Adapter-specific persistence failure.
    type Error;

    /// Reads the project configuration, or none when the project is not initialized.
    fn load(&self, root: &RepositoryRoot) -> Result<Option<StoredProject>, Self::Error>;

    /// Creates the project configuration without overwriting an existing file.
    fn create(
        &self,
        root: &RepositoryRoot,
        configuration: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error>;

    /// Replaces the project configuration only while it still has the `expected` revision.
    fn replace(
        &self,
        root: &RepositoryRoot,
        expected: ProjectRevision,
        configuration: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error>;

    /// Ensures repository-local ignored state exists.
    fn ensure_local_state(&self, root: &RepositoryRoot) -> Result<(), Self::Error>;
}

/// Persists one user-level taxonomy environment atomically.
pub trait GlobalConfigurationStore {
    /// Adapter-specific persistence failure.
    type Error;

    /// Loads the complete current snapshot.
    fn load(&self) -> Result<GlobalConfiguration, Self::Error>;

    /// Replaces the observed snapshot only when it remains current.
    fn compare_and_swap(
        &self,
        expected: &GlobalConfiguration,
        replacement: &GlobalConfiguration,
    ) -> Result<(), Self::Error>;
}

/// Persists authored project overrides and their portable lock.
pub trait ProjectStateStore {
    /// Adapter-specific persistence failure.
    type Error;

    /// Reads known project state without changing it.
    fn inspect(&self, root: &RepositoryRoot) -> Result<ProjectState, Self::Error>;

    /// Ensures repository-local ignored state exists.
    fn ensure_local_state(&self, root: &RepositoryRoot) -> Result<(), Self::Error>;

    /// Creates both project artifacts without overwriting either.
    fn initialize(
        &self,
        root: &RepositoryRoot,
        config: &ProjectConfig,
        lock: &ProjectLock,
    ) -> Result<(), Self::Error>;

    /// Creates a missing lock.
    fn create_lock(&self, root: &RepositoryRoot, lock: &ProjectLock) -> Result<(), Self::Error>;

    /// Replaces a recognized lock.
    fn replace_lock(
        &self,
        root: &RepositoryRoot,
        current: &ProjectLock,
        replacement: &ProjectLock,
    ) -> Result<(), Self::Error>;

    /// Replaces a recognized config/lock pair atomically.
    fn compare_and_swap(
        &self,
        root: &RepositoryRoot,
        current_config: &ProjectConfig,
        current_lock: &ProjectLock,
        replacement_config: &ProjectConfig,
        replacement_lock: &ProjectLock,
    ) -> Result<(), Self::Error>;
}
