use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::{ChangeTypeId, Description, TaxonomyId, TaxonomyVersion};

/// One semantic category in a software change taxonomy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeTypeDefinition {
    id: ChangeTypeId,
    description: Description,
}

impl ChangeTypeDefinition {
    /// Creates a described software change type.
    #[must_use]
    pub const fn new(id: ChangeTypeId, description: Description) -> Self {
        Self { id, description }
    }

    /// Returns the identifier scoped to its containing taxonomy.
    #[must_use]
    pub const fn id(&self) -> &ChangeTypeId {
        &self.id
    }

    /// Returns the semantic meaning of this change type.
    #[must_use]
    pub const fn description(&self) -> &Description {
        &self.description
    }
}

/// A versioned, ordered classification system for software changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaxonomyDefinition {
    id: TaxonomyId,
    version: TaxonomyVersion,
    description: Description,
    change_types: Vec<ChangeTypeDefinition>,
}

impl TaxonomyDefinition {
    /// Creates a taxonomy and enforces its structural invariants.
    ///
    /// # Errors
    ///
    /// Returns [`TaxonomyDefinitionError`] when the taxonomy is empty or
    /// repeats a change-type identifier.
    pub fn new(
        id: TaxonomyId,
        version: TaxonomyVersion,
        description: Description,
        change_types: Vec<ChangeTypeDefinition>,
    ) -> Result<Self, TaxonomyDefinitionError> {
        if change_types.is_empty() {
            return Err(TaxonomyDefinitionError::EmptyChangeTypes);
        }
        let mut ids = BTreeSet::new();
        for change_type in &change_types {
            if !ids.insert(change_type.id()) {
                return Err(TaxonomyDefinitionError::DuplicateChangeType(
                    change_type.id().clone(),
                ));
            }
        }
        Ok(Self {
            id,
            version,
            description,
            change_types,
        })
    }

    /// Returns the taxonomy identifier.
    #[must_use]
    pub const fn id(&self) -> &TaxonomyId {
        &self.id
    }

    /// Returns the taxonomy's semantic version.
    #[must_use]
    pub const fn version(&self) -> TaxonomyVersion {
        self.version
    }

    /// Returns the taxonomy's semantic purpose.
    #[must_use]
    pub const fn description(&self) -> &Description {
        &self.description
    }

    /// Returns change types in their canonical order.
    #[must_use]
    pub fn change_types(&self) -> &[ChangeTypeDefinition] {
        &self.change_types
    }

    /// Assembles a taxonomy from previously validated definitions.
    ///
    /// Copy and adapter operations may reconstruct a taxonomy from an already
    /// validated aggregate without repeating its structural checks.
    #[must_use]
    pub fn from_trusted(
        id: TaxonomyId,
        version: TaxonomyVersion,
        description: Description,
        change_types: Vec<ChangeTypeDefinition>,
    ) -> Self {
        Self {
            id,
            version,
            description,
            change_types,
        }
    }
}

/// A structural taxonomy-definition failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaxonomyDefinitionError {
    /// No change types were supplied.
    EmptyChangeTypes,
    /// A change-type identifier appears more than once.
    DuplicateChangeType(ChangeTypeId),
}

impl Display for TaxonomyDefinitionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyChangeTypes => {
                formatter.write_str("taxonomy must contain at least one change type")
            }
            Self::DuplicateChangeType(id) => {
                write!(formatter, "taxonomy repeats change type {id:?}")
            }
        }
    }
}

impl Error for TaxonomyDefinitionError {}
