use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::num::NonZeroU16;

/// A positive semantic-schema version.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SchemaVersion(NonZeroU16);

impl SchemaVersion {
    /// The first built-in commit-type schema version.
    pub const V1: Self = Self(NonZeroU16::MIN);

    /// Creates a positive schema version.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaVersionError`] when `value` is zero.
    pub const fn new(value: u16) -> Result<Self, SchemaVersionError> {
        match NonZeroU16::new(value) {
            Some(version) => Ok(Self(version)),
            None => Err(SchemaVersionError),
        }
    }

    /// Returns the integer schema version.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

impl Display for SchemaVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.get(), formatter)
    }
}

impl TryFrom<u16> for SchemaVersion {
    type Error = SchemaVersionError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// A zero schema version, which cannot identify a released schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchemaVersionError;

impl Display for SchemaVersionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("schema version must be greater than zero")
    }
}

impl Error for SchemaVersionError {}
