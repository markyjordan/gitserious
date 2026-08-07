use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::num::NonZeroU16;

/// A positive version for one resolved commit-message template.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TemplateVersion(NonZeroU16);

impl TemplateVersion {
    /// The first built-in template version.
    pub const V1: Self = Self(NonZeroU16::MIN);

    /// Creates a positive template version.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateVersionError`] when `value` is zero.
    pub const fn new(value: u16) -> Result<Self, TemplateVersionError> {
        match NonZeroU16::new(value) {
            Some(version) => Ok(Self(version)),
            None => Err(TemplateVersionError),
        }
    }

    /// Returns the integer template version.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

impl Display for TemplateVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.get(), formatter)
    }
}

impl TryFrom<u16> for TemplateVersion {
    type Error = TemplateVersionError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// A zero template version, which cannot identify a released template.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TemplateVersionError;

impl Display for TemplateVersionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("template version must be greater than zero")
    }
}

impl Error for TemplateVersionError {}
