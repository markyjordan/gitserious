use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::{CommitTypeId, PropertyKey, PropertyValue, PropertyValues};

/// An optional semantic area affected by a commit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CommitScope(Box<str>);

impl CommitScope {
    /// Creates a scope suitable for the Conventional Commit header grammar.
    ///
    /// # Errors
    ///
    /// Returns [`CommitScopeError`] when the value is blank, spans lines, has
    /// surrounding whitespace, or contains a header delimiter.
    pub fn new(value: impl Into<String>) -> Result<Self, CommitScopeError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(CommitScopeError::Blank);
        }
        if value.contains(['\n', '\r']) {
            return Err(CommitScopeError::LineBreak);
        }
        if value.trim() != value {
            return Err(CommitScopeError::SurroundingWhitespace);
        }
        if let Some(delimiter) = value
            .chars()
            .find(|character| matches!(character, '(' | ')' | ':'))
        {
            return Err(CommitScopeError::Delimiter(delimiter));
        }
        Ok(Self(value.into_boxed_str()))
    }

    /// Returns the authored scope.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for CommitScope {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Display for CommitScope {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A scope that cannot be represented unambiguously in a commit header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitScopeError {
    /// The scope contains no non-whitespace text.
    Blank,
    /// The scope contains a line break.
    LineBreak,
    /// The scope begins or ends with whitespace.
    SurroundingWhitespace,
    /// The scope contains a header delimiter.
    Delimiter(char),
}

impl Display for CommitScopeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blank => formatter.write_str("commit scope must contain non-whitespace text"),
            Self::LineBreak => formatter.write_str("commit scope must fit on one line"),
            Self::SurroundingWhitespace => {
                formatter.write_str("commit scope must not have surrounding whitespace")
            }
            Self::Delimiter(delimiter) => {
                write!(formatter, "commit scope must not contain {delimiter:?}")
            }
        }
    }
}

impl Error for CommitScopeError {}

/// The required summary portion of a commit header.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CommitSubject(Box<str>);

impl CommitSubject {
    /// Creates a non-empty, single-line commit subject.
    ///
    /// # Errors
    ///
    /// Returns [`CommitSubjectError`] when the subject is blank, spans lines,
    /// or has surrounding whitespace.
    pub fn new(value: impl Into<String>) -> Result<Self, CommitSubjectError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(CommitSubjectError::Blank);
        }
        if value.contains(['\n', '\r']) {
            return Err(CommitSubjectError::LineBreak);
        }
        if value.trim() != value {
            return Err(CommitSubjectError::SurroundingWhitespace);
        }
        Ok(Self(value.into_boxed_str()))
    }

    /// Returns the authored subject.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for CommitSubject {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Display for CommitSubject {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A subject that cannot be represented as a valid commit header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitSubjectError {
    /// The subject contains no non-whitespace text.
    Blank,
    /// The subject contains a line break.
    LineBreak,
    /// The subject begins or ends with whitespace.
    SurroundingWhitespace,
}

impl Display for CommitSubjectError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blank => formatter.write_str("commit subject must contain non-whitespace text"),
            Self::LineBreak => formatter.write_str("commit subject must fit on one line"),
            Self::SurroundingWhitespace => {
                formatter.write_str("commit subject must not have surrounding whitespace")
            }
        }
    }
}

impl Error for CommitSubjectError {}

/// Authored values associated with one durable property key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthoredProperty {
    key: PropertyKey,
    values: PropertyValues,
}

impl AuthoredProperty {
    /// Associates a property key with one or more authored values.
    #[must_use]
    pub const fn new(key: PropertyKey, values: PropertyValues) -> Self {
        Self { key, values }
    }

    /// Returns the durable property key.
    #[must_use]
    pub const fn key(&self) -> &PropertyKey {
        &self.key
    }

    /// Returns the authored values and their declared multiplicity.
    #[must_use]
    pub const fn values(&self) -> &PropertyValues {
        &self.values
    }
}

/// A validated authored commit before canonical message rendering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitDraft {
    commit_type: CommitTypeId,
    scope: Option<CommitScope>,
    subject: CommitSubject,
    properties: Vec<AuthoredProperty>,
    breaking_change: Option<PropertyValue>,
}

impl CommitDraft {
    /// Creates a draft with unique keyed properties.
    ///
    /// # Errors
    ///
    /// Returns [`CommitDraftError`] when the same property key appears more
    /// than once. Repeatable values belong in one [`PropertyValues`] value.
    pub fn new(
        commit_type: CommitTypeId,
        scope: Option<CommitScope>,
        subject: CommitSubject,
        properties: Vec<AuthoredProperty>,
    ) -> Result<Self, CommitDraftError> {
        let mut keys = BTreeSet::new();
        for property in &properties {
            if !keys.insert(property.key()) {
                return Err(CommitDraftError::DuplicateProperty(property.key().clone()));
            }
        }
        Ok(Self {
            commit_type,
            scope,
            subject,
            properties,
            breaking_change: None,
        })
    }

    /// Adds an optional Conventional Commits breaking-change footer.
    #[must_use]
    pub fn with_breaking_change(mut self, breaking_change: PropertyValue) -> Self {
        self.breaking_change = Some(breaking_change);
        self
    }

    /// Returns the selected commit-type identifier.
    #[must_use]
    pub const fn commit_type(&self) -> &CommitTypeId {
        &self.commit_type
    }

    /// Returns the optional authored scope.
    #[must_use]
    pub const fn scope(&self) -> Option<&CommitScope> {
        self.scope.as_ref()
    }

    /// Returns the required authored subject.
    #[must_use]
    pub const fn subject(&self) -> &CommitSubject {
        &self.subject
    }

    /// Returns keyed properties in authored order.
    #[must_use]
    pub fn properties(&self) -> &[AuthoredProperty] {
        &self.properties
    }

    /// Returns the optional breaking-change footer description.
    #[must_use]
    pub const fn breaking_change(&self) -> Option<&PropertyValue> {
        self.breaking_change.as_ref()
    }
}

/// A structural invariant violation in an authored commit draft.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitDraftError {
    /// The same property key was supplied more than once.
    DuplicateProperty(PropertyKey),
}

impl Display for CommitDraftError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateProperty(key) => {
                write!(formatter, "commit draft repeats property {key:?}")
            }
        }
    }
}

impl Error for CommitDraftError {}
