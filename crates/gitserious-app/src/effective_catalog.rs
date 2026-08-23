use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::{ConfigurationCatalog, ConfigurationCatalogError, UserConfigurationStore};

/// Loads and validates the effective catalog from one user-configuration
/// store.
///
/// # Errors
///
/// Returns [`EffectiveCatalogError`] when loading fails or the resulting
/// aggregate is invalid.
pub fn load_effective_catalog<S>(
    store: &S,
) -> Result<ConfigurationCatalog, EffectiveCatalogError<S::Error>>
where
    S: UserConfigurationStore + ?Sized,
{
    let configuration = store.load().map_err(EffectiveCatalogError::Store)?;
    ConfigurationCatalog::new(&configuration).map_err(EffectiveCatalogError::Catalog)
}

/// Failure to obtain an effective configuration catalog.
#[derive(Debug)]
pub enum EffectiveCatalogError<StoreError> {
    /// The user-configuration store could not be read.
    Store(StoreError),
    /// The loaded aggregate is not a valid effective catalog.
    Catalog(ConfigurationCatalogError),
}

impl<StoreError> Display for EffectiveCatalogError<StoreError>
where
    StoreError: Display,
{
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(error) => Display::fmt(error, formatter),
            Self::Catalog(error) => Display::fmt(error, formatter),
        }
    }
}

impl<StoreError> Error for EffectiveCatalogError<StoreError>
where
    StoreError: Error + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Store(error) => Some(error),
            Self::Catalog(error) => Some(error),
        }
    }
}
