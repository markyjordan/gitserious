use std::borrow::Borrow;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

/// The reason an open domain identifier is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentifierErrorKind {
    /// The identifier contains no characters.
    Empty,
    /// The first character is not a lowercase ASCII letter.
    InvalidStart,
    /// A later character is neither a lowercase ASCII letter, a digit, nor a hyphen.
    InvalidCharacter {
        /// The byte index of the invalid character.
        index: usize,
    },
    /// Two adjacent hyphens appear in the identifier.
    ConsecutiveHyphen {
        /// The byte index of the second hyphen.
        index: usize,
    },
    /// The identifier ends with a hyphen.
    TrailingHyphen,
}

/// An invalid commit-type, property, or condition identifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentifierError {
    value: Box<str>,
    kind: IdentifierErrorKind,
}

impl IdentifierError {
    /// Returns the rejected identifier text.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Returns the validation failure.
    #[must_use]
    pub const fn kind(&self) -> IdentifierErrorKind {
        self.kind
    }
}

impl Display for IdentifierError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self.kind {
            IdentifierErrorKind::Empty => formatter.write_str("identifier must not be empty"),
            IdentifierErrorKind::InvalidStart => write!(
                formatter,
                "identifier {:?} must start with a lowercase ASCII letter",
                self.value
            ),
            IdentifierErrorKind::InvalidCharacter { index } => write!(
                formatter,
                "identifier {:?} has an invalid character at byte index {index}",
                self.value
            ),
            IdentifierErrorKind::ConsecutiveHyphen { index } => write!(
                formatter,
                "identifier {:?} has consecutive hyphens at byte index {index}",
                self.value
            ),
            IdentifierErrorKind::TrailingHyphen => {
                write!(
                    formatter,
                    "identifier {:?} must not end with a hyphen",
                    self.value
                )
            }
        }
    }
}

impl Error for IdentifierError {}

fn validate_identifier(value: &str) -> Result<(), IdentifierErrorKind> {
    let bytes = value.as_bytes();
    let Some(first) = bytes.first() else {
        return Err(IdentifierErrorKind::Empty);
    };

    if !first.is_ascii_lowercase() {
        return Err(IdentifierErrorKind::InvalidStart);
    }

    let mut previous_was_hyphen = false;
    for (index, byte) in bytes.iter().copied().enumerate().skip(1) {
        if byte == b'-' {
            if previous_was_hyphen {
                return Err(IdentifierErrorKind::ConsecutiveHyphen { index });
            }
            previous_was_hyphen = true;
        } else if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
            previous_was_hyphen = false;
        } else {
            return Err(IdentifierErrorKind::InvalidCharacter { index });
        }
    }

    if previous_was_hyphen {
        return Err(IdentifierErrorKind::TrailingHyphen);
    }

    Ok(())
}

fn validate_and_box(value: String) -> Result<Box<str>, IdentifierError> {
    validate_identifier(&value).map_err(|kind| IdentifierError {
        value: value.clone().into_boxed_str(),
        kind,
    })?;
    Ok(value.into_boxed_str())
}

macro_rules! define_identifier {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(Box<str>);

        impl $name {
            /// Creates a validated identifier.
            ///
            /// # Errors
            ///
            /// Returns [`IdentifierError`] when the value does not use lowercase
            /// ASCII kebab syntax.
            pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
                validate_and_box(value.into()).map(Self)
            }

            /// Returns the identifier text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub(crate) fn from_trusted(value: &'static str) -> Self {
                Self(Box::from(value))
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                self.as_str()
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = IdentifierError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = IdentifierError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdentifierError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
    };
}

define_identifier!(
    CommitTypeId,
    "An open, validated identifier for a commit type."
);
define_identifier!(
    PropertyKey,
    "An open, validated identifier for a durable message property."
);
define_identifier!(
    ConditionId,
    "An open, validated identifier for a conditional requirement rule."
);
