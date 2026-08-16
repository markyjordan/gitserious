use std::error::Error;
use std::fmt::{self, Display, Formatter, Write as _};

use crate::{
    CommitDraft, CommitTypeDefinition, CommitTypeId, PropertyKey, PropertyMultiplicity,
    PropertyRequirement,
};

/// A canonical, schema-validated commit message.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CommitMessage(Box<str>);

impl CommitMessage {
    /// Returns the canonical message text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for CommitMessage {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Display for CommitMessage {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Validates and canonically renders an authored draft.
///
/// # Errors
///
/// Returns [`CommitValidationErrors`] when the draft does not satisfy the
/// selected commit-type schema.
pub fn render_commit_message(
    definition: &CommitTypeDefinition,
    draft: &CommitDraft,
) -> Result<CommitMessage, CommitValidationErrors> {
    validate_commit_draft(definition, draft)?;

    let mut message = draft.commit_type().to_string();
    if let Some(scope) = draft.scope() {
        let _ = write!(message, "({})", normalized_scope(scope.as_str()));
    }
    if draft.breaking_change().is_some() {
        message.push('!');
    }
    let _ = writeln!(message, ": {}", draft.subject());

    for definition_property in definition.properties() {
        let Some(authored) = draft
            .properties()
            .iter()
            .find(|property| property.key() == definition_property.key())
        else {
            continue;
        };
        for value in authored.values() {
            let _ = write!(message, "\n{}:\n", authored.key());
            for line in value.as_str().lines() {
                let _ = writeln!(message, "{line}");
            }
        }
    }

    if let Some(breaking_change) = draft.breaking_change() {
        let mut lines = breaking_change.as_str().lines();
        if let Some(first) = lines.next() {
            let _ = writeln!(message, "\nBREAKING CHANGE: {first}");
        }
        for line in lines {
            let _ = writeln!(message, "{line}");
        }
    }

    Ok(CommitMessage(message.into_boxed_str()))
}

fn normalized_scope(scope: &str) -> String {
    scope.split_whitespace().collect::<Vec<_>>().join("-")
}

/// Validates a draft against a selected commit-type schema.
///
/// # Errors
///
/// Returns all type, property, requiredness, and multiplicity violations.
pub fn validate_commit_draft(
    definition: &CommitTypeDefinition,
    draft: &CommitDraft,
) -> Result<(), CommitValidationErrors> {
    let mut errors = Vec::new();
    if definition.id() != draft.commit_type() {
        errors.push(CommitValidationError::TypeMismatch {
            expected: definition.id().clone(),
            actual: draft.commit_type().clone(),
        });
    }

    for property in draft.properties() {
        let Some(expected) = definition
            .properties()
            .iter()
            .find(|candidate| candidate.key() == property.key())
        else {
            errors.push(CommitValidationError::UnknownProperty(
                property.key().clone(),
            ));
            continue;
        };
        if expected.multiplicity() != property.values().multiplicity() {
            errors.push(CommitValidationError::Multiplicity {
                key: property.key().clone(),
                expected: expected.multiplicity(),
                actual: property.values().multiplicity(),
            });
        }
    }

    for property in definition.properties() {
        if matches!(property.requirement(), PropertyRequirement::Required)
            && !draft
                .properties()
                .iter()
                .any(|authored| authored.key() == property.key())
        {
            errors.push(CommitValidationError::MissingRequired(
                property.key().clone(),
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(CommitValidationErrors::new(errors))
    }
}

/// One violation between an authored draft and its selected schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitValidationError {
    /// The draft identifies a different type than the selected schema.
    TypeMismatch {
        /// Selected schema type.
        expected: CommitTypeId,
        /// Authored header type.
        actual: CommitTypeId,
    },
    /// The draft supplies a property absent from the selected schema.
    UnknownProperty(PropertyKey),
    /// A required property is absent.
    MissingRequired(PropertyKey),
    /// Authored values use a different multiplicity than the schema.
    Multiplicity {
        /// Property with the mismatch.
        key: PropertyKey,
        /// Schema multiplicity.
        expected: PropertyMultiplicity,
        /// Authored multiplicity.
        actual: PropertyMultiplicity,
    },
}

impl Display for CommitValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::TypeMismatch { expected, actual } => write!(
                formatter,
                "selected type {expected:?} cannot use header type {actual:?}"
            ),
            Self::UnknownProperty(key) => {
                write!(
                    formatter,
                    "property {key:?} is not defined for the selected type"
                )
            }
            Self::MissingRequired(key) => {
                write!(formatter, "complete required property {key:?}")
            }
            Self::Multiplicity {
                key,
                expected,
                actual,
            } => write!(
                formatter,
                "property {key:?} requires {expected:?} values, not {actual:?} values"
            ),
        }
    }
}

impl Error for CommitValidationError {}

/// Every schema violation in an authored commit draft.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitValidationErrors(Vec<CommitValidationError>);

impl CommitValidationErrors {
    fn new(errors: Vec<CommitValidationError>) -> Self {
        Self(errors)
    }

    /// Returns failures in schema-validation order.
    #[must_use]
    pub fn as_slice(&self) -> &[CommitValidationError] {
        &self.0
    }
}

impl Display for CommitValidationErrors {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        for (index, error) in self.0.iter().enumerate() {
            if index > 0 {
                formatter.write_str("; ")?;
            }
            Display::fmt(error, formatter)?;
        }
        Ok(())
    }
}

impl Error for CommitValidationErrors {}
