use std::error::Error;
use std::fmt::{self, Display, Formatter};

/// A nonblank semantic description attached to configuration domain values.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Description(Box<str>);

impl Description {
    /// Creates a description containing non-whitespace text.
    ///
    /// # Errors
    ///
    /// Returns [`DescriptionError`] when `value` is empty or whitespace-only.
    pub fn new(value: impl Into<String>) -> Result<Self, DescriptionError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(DescriptionError);
        }
        Ok(Self(value.into_boxed_str()))
    }

    /// Returns the description exactly as supplied.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn from_trusted(value: &'static str) -> Self {
        Self(Box::from(value))
    }

    pub(crate) fn from_validated(value: impl Into<String>) -> Self {
        Self(value.into().into_boxed_str())
    }
}

impl AsRef<str> for Description {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Display for Description {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<&str> for Description {
    type Error = DescriptionError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for Description {
    type Error = DescriptionError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// An empty or whitespace-only semantic description.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DescriptionError;

impl Display for DescriptionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("description must contain non-whitespace text")
    }
}

impl Error for DescriptionError {}
