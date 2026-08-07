use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::{CommitTypeDefinition, CommitTypeId, TemplateId, TemplateVersion};

/// A versioned, ordered commit-message policy template.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitMessageTemplateDefinition {
    version: TemplateVersion,
    id: TemplateId,
    description: Box<str>,
    commit_types: Vec<CommitTypeDefinition>,
}

impl CommitMessageTemplateDefinition {
    /// Creates a template definition and enforces its structural invariants.
    ///
    /// # Errors
    ///
    /// Returns [`CommitMessageTemplateDefinitionError`] when the description is
    /// blank, no commit types are supplied, or a commit-type ID is repeated.
    pub fn new(
        version: TemplateVersion,
        id: TemplateId,
        description: impl Into<String>,
        commit_types: Vec<CommitTypeDefinition>,
    ) -> Result<Self, CommitMessageTemplateDefinitionError> {
        let description = description.into();
        if description.trim().is_empty() {
            return Err(CommitMessageTemplateDefinitionError::EmptyDescription);
        }
        if commit_types.is_empty() {
            return Err(CommitMessageTemplateDefinitionError::EmptyCommitTypes);
        }

        let mut ids = BTreeSet::new();
        for definition in &commit_types {
            if !ids.insert(definition.id()) {
                return Err(CommitMessageTemplateDefinitionError::DuplicateCommitType(
                    definition.id().clone(),
                ));
            }
        }

        Ok(Self {
            version,
            id,
            description: description.into_boxed_str(),
            commit_types,
        })
    }

    /// Returns the concrete template version.
    #[must_use]
    pub const fn version(&self) -> TemplateVersion {
        self.version
    }

    /// Returns the concrete template identifier.
    #[must_use]
    pub const fn id(&self) -> &TemplateId {
        &self.id
    }

    /// Returns the template's semantic purpose.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Returns commit-type definitions in template order.
    #[must_use]
    pub fn commit_types(&self) -> &[CommitTypeDefinition] {
        &self.commit_types
    }

    pub(crate) fn from_trusted(
        version: TemplateVersion,
        id: TemplateId,
        description: &'static str,
        commit_types: Vec<CommitTypeDefinition>,
    ) -> Self {
        Self {
            version,
            id,
            description: Box::from(description),
            commit_types,
        }
    }
}

/// A structural invariant violation in a commit-message template.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitMessageTemplateDefinitionError {
    /// The template has no semantic description.
    EmptyDescription,
    /// The template contains no commit-type definitions.
    EmptyCommitTypes,
    /// Two definitions use the same commit-type identifier.
    DuplicateCommitType(CommitTypeId),
}

impl Display for CommitMessageTemplateDefinitionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyDescription => formatter
                .write_str("commit-message template description must contain non-whitespace text"),
            Self::EmptyCommitTypes => {
                formatter.write_str("commit-message template must contain at least one commit type")
            }
            Self::DuplicateCommitType(id) => {
                write!(
                    formatter,
                    "commit-message template repeats commit type {id:?}"
                )
            }
        }
    }
}

impl Error for CommitMessageTemplateDefinitionError {}
