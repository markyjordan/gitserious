use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::num::NonZeroU16;

use crate::TaxonomyId;

/// A positive semantic version for one taxonomy definition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TaxonomyVersion(NonZeroU16);

impl TaxonomyVersion {
    /// The first taxonomy version.
    pub const V1: Self = Self(NonZeroU16::MIN);

    /// Creates a positive taxonomy version.
    ///
    /// # Errors
    ///
    /// Returns [`TaxonomyVersionError`] when `value` is zero.
    pub const fn new(value: u16) -> Result<Self, TaxonomyVersionError> {
        match NonZeroU16::new(value) {
            Some(value) => Ok(Self(value)),
            None => Err(TaxonomyVersionError),
        }
    }

    /// Returns the integer taxonomy version.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

impl Display for TaxonomyVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.get(), formatter)
    }
}

/// One taxonomy version written as `<id>-v<N>`, such as `conventional-v1`.
///
/// Taxonomy identifiers cannot end in `-v<digits>`, so the text splits back one way.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VersionedTaxonomy<'a> {
    id: &'a TaxonomyId,
    version: TaxonomyVersion,
}

impl<'a> VersionedTaxonomy<'a> {
    #[must_use]
    pub const fn new(id: &'a TaxonomyId, version: TaxonomyVersion) -> Self {
        Self { id, version }
    }
}

impl Display for VersionedTaxonomy<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}-v{}", self.id, self.version)
    }
}

/// A zero taxonomy version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaxonomyVersionError;

impl Display for TaxonomyVersionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("taxonomy version must be greater than zero")
    }
}

impl Error for TaxonomyVersionError {}
