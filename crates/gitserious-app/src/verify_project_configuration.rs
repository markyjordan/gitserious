use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

use crate::{ProjectConfiguration, ProjectConfigurationStore, RepositoryLocator};

/// What a read-only check of the root project file found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectVerification {
    /// The file exists and is a complete, valid project configuration.
    Valid(ProjectConfiguration),
    /// The repository has no gitserious.toml.
    NotInitialized,
    /// The file exists but is not a valid project configuration.
    Invalid(String),
}

/// The project could not be checked at all.
#[derive(Debug)]
pub enum VerifyProjectError<LocatorError, StoreError> {
    Repository(LocatorError),
    Store(StoreError),
}

impl<L: Display, S: Display> Display for VerifyProjectError<L, S> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repository(error) => Display::fmt(error, formatter),
            Self::Store(error) => Display::fmt(error, formatter),
        }
    }
}

impl<L: Error + 'static, S: Error + 'static> Error for VerifyProjectError<L, S> {}

/// Checks the root project file without writing anything or reading the Library.
pub fn verify_project_configuration<L, S>(
    locator: &L,
    store: &S,
    start: &Path,
) -> Result<ProjectVerification, VerifyProjectError<L::Error, S::Error>>
where
    L: RepositoryLocator + ?Sized,
    S: ProjectConfigurationStore + ?Sized,
    S::Error: Display,
{
    let root = locator
        .locate(start)
        .map_err(VerifyProjectError::Repository)?;
    match store.load(&root) {
        Ok(Some(stored)) => Ok(ProjectVerification::Valid(stored.configuration)),
        Ok(None) => Ok(ProjectVerification::NotInitialized),
        Err(error) if S::is_invalid_document(&error) => {
            Ok(ProjectVerification::Invalid(error.to_string()))
        }
        Err(error) => Err(VerifyProjectError::Store(error)),
    }
}
