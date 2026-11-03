use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::{ConditionId, PropertyKey};

fn is_blank(value: &str) -> bool {
    value.trim().is_empty()
}

/// Authored text for one durable message property.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PropertyValue(Box<str>);

impl PropertyValue {
    /// Creates a value that contains at least one non-whitespace character.
    ///
    /// # Errors
    ///
    /// Returns [`PropertyValueError`] when the supplied text is empty or
    /// whitespace-only.
    pub fn new(value: impl Into<String>) -> Result<Self, PropertyValueError> {
        let value = value.into();
        if is_blank(&value) {
            return Err(PropertyValueError);
        }
        Ok(Self(value.into_boxed_str()))
    }

    /// Returns the authored text exactly as supplied.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for PropertyValue {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Display for PropertyValue {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<&str> for PropertyValue {
    type Error = PropertyValueError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for PropertyValue {
    type Error = PropertyValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// An empty or whitespace-only property value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PropertyValueError;

impl Display for PropertyValueError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("property value must contain non-whitespace text")
    }
}

impl Error for PropertyValueError {}

/// Metadata describing when a conditional property becomes required.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyCondition {
    id: ConditionId,
    rationale: Box<str>,
}

impl PropertyCondition {
    /// Creates conditional rule metadata with a non-empty rationale.
    ///
    /// # Errors
    ///
    /// Returns [`PropertyConditionError`] when the rationale is empty or
    /// whitespace-only.
    pub fn new(
        id: ConditionId,
        rationale: impl Into<String>,
    ) -> Result<Self, PropertyConditionError> {
        let rationale = rationale.into();
        if is_blank(&rationale) {
            return Err(PropertyConditionError);
        }
        Ok(Self {
            id,
            rationale: rationale.into_boxed_str(),
        })
    }

    /// Returns the stable condition identifier.
    #[must_use]
    pub const fn id(&self) -> &ConditionId {
        &self.id
    }

    /// Returns the human-readable condition rationale.
    #[must_use]
    pub fn rationale(&self) -> &str {
        &self.rationale
    }

    pub(crate) fn from_trusted(id: ConditionId, rationale: &'static str) -> Self {
        Self {
            id,
            rationale: Box::from(rationale),
        }
    }
}

/// An empty or whitespace-only conditional rationale.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PropertyConditionError;

impl Display for PropertyConditionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("property condition rationale must contain non-whitespace text")
    }
}

impl Error for PropertyConditionError {}

/// How strongly a commit-type schema expects a property.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PropertyRequirement {
    /// The type-specific semantic chain is incomplete without the property.
    Required,
    /// The property is normally useful but can be legitimately absent.
    Recommended,
    /// The property is useful only when applicable.
    Optional,
    /// The property becomes required when the attached condition holds.
    Conditional(PropertyCondition),
}

/// The semantic contract for one property within a commit type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyDefinition {
    key: PropertyKey,
    description: Box<str>,
    requirement: PropertyRequirement,
}

impl PropertyDefinition {
    /// Creates a property definition with a non-empty semantic description.
    ///
    /// # Errors
    ///
    /// Returns [`PropertyDefinitionError`] when the description is empty or
    /// whitespace-only.
    pub fn new(
        key: PropertyKey,
        description: impl Into<String>,
        requirement: PropertyRequirement,
    ) -> Result<Self, PropertyDefinitionError> {
        let description = description.into();
        if is_blank(&description) {
            return Err(PropertyDefinitionError);
        }
        Ok(Self {
            key,
            description: description.into_boxed_str(),
            requirement,
        })
    }

    /// Returns the machine-readable property key.
    #[must_use]
    pub const fn key(&self) -> &PropertyKey {
        &self.key
    }

    /// Returns the property's type-specific semantic meaning.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Returns the property's requirement metadata.
    #[must_use]
    pub const fn requirement(&self) -> &PropertyRequirement {
        &self.requirement
    }

    pub(crate) fn from_trusted(
        key: PropertyKey,
        description: &'static str,
        requirement: PropertyRequirement,
    ) -> Self {
        Self {
            key,
            description: Box::from(description),
            requirement,
        }
    }
}

/// An empty or whitespace-only property definition description.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PropertyDefinitionError;

impl Display for PropertyDefinitionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("property definition description must contain non-whitespace text")
    }
}

impl Error for PropertyDefinitionError {}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use crate::{ConditionId, PropertyKey};

    use super::{
        PropertyCondition, PropertyConditionError, PropertyDefinition, PropertyDefinitionError,
        PropertyRequirement, PropertyValue, PropertyValueError,
    };

    #[test]
    fn property_values_preserve_non_empty_unicode_and_multiline_text() -> Result<(), Box<dyn Error>>
    {
        let text = "  Causal context 🦀\nremains intact.  ";
        let value = PropertyValue::new(text)?;
        let borrowed = PropertyValue::try_from(text)?;
        let owned = PropertyValue::try_from(String::from(text))?;

        assert_eq!(value.as_str(), text);
        assert_eq!(AsRef::<str>::as_ref(&value), text);
        assert_eq!(value.to_string(), text);
        assert_eq!(value, borrowed);
        assert_eq!(borrowed, owned);

        Ok(())
    }

    #[test]
    fn property_values_reject_empty_and_whitespace_only_text() {
        for text in ["", " ", "\n\t", "\u{2003}"] {
            assert_eq!(PropertyValue::new(text), Err(PropertyValueError));
        }
        assert_eq!(
            PropertyValueError.to_string(),
            "property value must contain non-whitespace text"
        );
    }

    #[test]
    fn conditions_require_rationale_and_preserve_typed_metadata() -> Result<(), Box<dyn Error>> {
        let id = ConditionId::new("known-cost")?;
        let condition = PropertyCondition::new(id.clone(), "A known cost exists.")?;

        assert_eq!(condition.id(), &id);
        assert_eq!(condition.rationale(), "A known cost exists.");
        assert_eq!(
            PropertyCondition::new(id, " \n"),
            Err(PropertyConditionError)
        );
        assert!(!PropertyConditionError.to_string().is_empty());

        Ok(())
    }

    #[test]
    fn property_definitions_preserve_all_schema_dimensions() -> Result<(), Box<dyn Error>> {
        let condition =
            PropertyCondition::new(ConditionId::new("known-cost")?, "Required for known costs.")?;
        let definition = PropertyDefinition::new(
            PropertyKey::new("tradeoff")?,
            "The accepted cost.",
            PropertyRequirement::Conditional(condition.clone()),
        )?;

        assert_eq!(definition.key().as_str(), "tradeoff");
        assert_eq!(definition.description(), "The accepted cost.");
        assert_eq!(
            definition.requirement(),
            &PropertyRequirement::Conditional(condition)
        );

        let cloned = definition.clone();
        assert_eq!(cloned, definition);

        Ok(())
    }

    #[test]
    fn property_definitions_reject_blank_descriptions() -> Result<(), Box<dyn Error>> {
        let result = PropertyDefinition::new(
            PropertyKey::new("intent")?,
            "\t",
            PropertyRequirement::Required,
        );

        assert_eq!(result, Err(PropertyDefinitionError));
        assert!(!PropertyDefinitionError.to_string().is_empty());

        Ok(())
    }
}
