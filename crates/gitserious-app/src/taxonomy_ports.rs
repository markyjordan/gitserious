use std::path::PathBuf;

use crate::{
    GlobalConfiguration, ProjectConfiguration, ProjectRevision, RepositoryRoot, StoredProject,
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

    /// Returns the file that holds the snapshot, without reading it.
    fn path(&self) -> PathBuf;

    /// Loads the complete current snapshot.
    fn load(&self) -> Result<GlobalConfiguration, Self::Error>;

    /// Replaces the observed snapshot only when it remains current.
    fn compare_and_swap(
        &self,
        expected: &GlobalConfiguration,
        replacement: &GlobalConfiguration,
    ) -> Result<(), Self::Error>;
}
