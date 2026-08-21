use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::num::NonZeroU16;

/// A positive semantic version for one durable-property typeset.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TypesetVersion(NonZeroU16);

impl TypesetVersion {
    /// The first typeset version.
    pub const V1: Self = Self(NonZeroU16::MIN);

    /// Creates a positive typeset version.
    ///
    /// # Errors
    ///
    /// Returns [`TypesetVersionError`] when `value` is zero.
    pub const fn new(value: u16) -> Result<Self, TypesetVersionError> {
        match NonZeroU16::new(value) {
            Some(value) => Ok(Self(value)),
            None => Err(TypesetVersionError),
        }
    }

    /// Returns the integer typeset version.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

impl Display for TypesetVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.get(), formatter)
    }
}

/// A zero typeset version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypesetVersionError;

impl Display for TypesetVersionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("typeset version must be greater than zero")
    }
}

impl Error for TypesetVersionError {}
