use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::slice;

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

/// Whether a property accepts one value or a non-empty sequence of values.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PropertyMultiplicity {
    /// Exactly one value.
    Single,
    /// One or more independently represented values.
    Multiple,
}

/// A non-empty collection of authored values with explicit multiplicity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyValues {
    multiplicity: PropertyMultiplicity,
    values: Vec<PropertyValue>,
}

impl PropertyValues {
    /// Creates a single-valued collection.
    #[must_use]
    pub fn single(value: PropertyValue) -> Self {
        Self {
            multiplicity: PropertyMultiplicity::Single,
            values: vec![value],
        }
    }

    /// Creates a repeatable collection containing at least one value.
    ///
    /// # Errors
    ///
    /// Returns [`PropertyValuesError`] when the iterator yields no values.
    pub fn multiple(
        values: impl IntoIterator<Item = PropertyValue>,
    ) -> Result<Self, PropertyValuesError> {
        let values = values.into_iter().collect::<Vec<_>>();
        if values.is_empty() {
            return Err(PropertyValuesError);
        }
        Ok(Self {
            multiplicity: PropertyMultiplicity::Multiple,
            values,
        })
    }

    /// Returns the collection's declared multiplicity.
    #[must_use]
    pub const fn multiplicity(&self) -> PropertyMultiplicity {
        self.multiplicity
    }

    /// Returns the number of values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns whether the collection contains no values.
    ///
    /// This always returns `false`; it is provided alongside [`Self::len`] for
    /// conventional collection access.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Returns the values in authored order.
    #[must_use]
    pub fn as_slice(&self) -> &[PropertyValue] {
        &self.values
    }

    /// Iterates over values in authored order.
    pub fn iter(&self) -> slice::Iter<'_, PropertyValue> {
        self.values.iter()
    }

    /// Consumes the collection and returns its values.
    #[must_use]
    pub fn into_values(self) -> Vec<PropertyValue> {
        self.values
    }
}

impl<'a> IntoIterator for &'a PropertyValues {
    type Item = &'a PropertyValue;
    type IntoIter = slice::Iter<'a, PropertyValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// An attempted repeatable property collection with no values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PropertyValuesError;

impl Display for PropertyValuesError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("a property value collection must not be empty")
    }
}

impl Error for PropertyValuesError {}

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
    multiplicity: PropertyMultiplicity,
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
        multiplicity: PropertyMultiplicity,
    ) -> Result<Self, PropertyDefinitionError> {
        let description = description.into();
        if is_blank(&description) {
            return Err(PropertyDefinitionError);
        }
        Ok(Self {
            key,
            description: description.into_boxed_str(),
            requirement,
            multiplicity,
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

    /// Returns the property's allowed multiplicity.
    #[must_use]
    pub const fn multiplicity(&self) -> PropertyMultiplicity {
        self.multiplicity
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
            multiplicity: PropertyMultiplicity::Single,
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
