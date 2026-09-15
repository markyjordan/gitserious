use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::sync::LazyLock;

use crate::{
    CommitTypeDefinition, CommitTypeId, Description, TaxonomyId, TaxonomyVersion,
    built_in_configuration,
};

/// Informational source recorded when one taxonomy is forked from another.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaxonomyLineage {
    id: TaxonomyId,
    version: TaxonomyVersion,
}

impl TaxonomyLineage {
    /// Records the exact source snapshot without creating an update relationship.
    #[must_use]
    pub const fn new(id: TaxonomyId, version: TaxonomyVersion) -> Self {
        Self { id, version }
    }

    /// Returns the source taxonomy identifier.
    #[must_use]
    pub const fn id(&self) -> &TaxonomyId {
        &self.id
    }

    /// Returns the source taxonomy version at fork time.
    #[must_use]
    pub const fn version(&self) -> TaxonomyVersion {
        self.version
    }
}

/// One complete, selectable commit taxonomy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Taxonomy {
    id: TaxonomyId,
    version: TaxonomyVersion,
    description: Description,
    derived_from: Option<TaxonomyLineage>,
    commit_types: Vec<CommitTypeDefinition>,
}

impl Taxonomy {
    /// Creates a complete taxonomy with unique, ordered commit types.
    ///
    /// # Errors
    /// Returns an error when no commit types are supplied or an identifier is repeated.
    pub fn new(
        id: TaxonomyId,
        version: TaxonomyVersion,
        description: Description,
        derived_from: Option<TaxonomyLineage>,
        commit_types: Vec<CommitTypeDefinition>,
    ) -> Result<Self, TaxonomyError> {
        if commit_types.is_empty() {
            return Err(TaxonomyError::EmptyCommitTypes);
        }
        let mut ids = BTreeSet::new();
        for definition in &commit_types {
            if !ids.insert(definition.id()) {
                return Err(TaxonomyError::DuplicateCommitType(definition.id().clone()));
            }
        }
        Ok(Self {
            id,
            version,
            description,
            derived_from,
            commit_types,
        })
    }

    /// Returns the taxonomy identifier.
    #[must_use]
    pub const fn id(&self) -> &TaxonomyId {
        &self.id
    }

    /// Returns the taxonomy semantic version.
    #[must_use]
    pub const fn version(&self) -> TaxonomyVersion {
        self.version
    }

    /// Returns the taxonomy purpose.
    #[must_use]
    pub const fn description(&self) -> &Description {
        &self.description
    }

    /// Returns optional fork provenance.
    #[must_use]
    pub const fn derived_from(&self) -> Option<&TaxonomyLineage> {
        self.derived_from.as_ref()
    }

    /// Returns commit types in canonical picker order.
    #[must_use]
    pub fn commit_types(&self) -> &[CommitTypeDefinition] {
        &self.commit_types
    }

    /// Copies this taxonomy under a new identity and records snapshot lineage.
    #[must_use]
    pub fn fork(&self, id: TaxonomyId) -> Self {
        Self {
            id,
            version: TaxonomyVersion::V1,
            description: self.description.clone(),
            derived_from: Some(TaxonomyLineage::new(self.id.clone(), self.version)),
            commit_types: self.commit_types.clone(),
        }
    }
}

/// A structural taxonomy failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaxonomyError {
    /// No commit types were supplied.
    EmptyCommitTypes,
    /// A commit-type identity is repeated.
    DuplicateCommitType(CommitTypeId),
}

impl Display for TaxonomyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCommitTypes => {
                formatter.write_str("taxonomy must contain at least one commit type")
            }
            Self::DuplicateCommitType(id) => {
                write!(formatter, "taxonomy repeats commit type {id:?}")
            }
        }
    }
}

impl Error for TaxonomyError {}

static BUILT_INS: LazyLock<Vec<Taxonomy>> = LazyLock::new(|| {
    built_in_configuration()
        .templates()
        .iter()
        .map(|template| {
            let taxonomy = built_in_configuration()
                .find_taxonomy(template.taxonomy())
                .unwrap_or_else(|| unreachable!("compiled template taxonomy must exist"));
            let typeset = built_in_configuration()
                .find_typeset(template.taxonomy(), template.typeset())
                .unwrap_or_else(|| unreachable!("compiled template typeset must exist"));
            let resolved = crate::ResolvedTaxonomy::resolve(template, taxonomy, typeset)
                .unwrap_or_else(|_| unreachable!("compiled taxonomy must resolve"));
            let id = if resolved.taxonomy_id().as_str() == "ml-research" {
                TaxonomyId::from_trusted("research")
            } else {
                resolved.taxonomy_id().clone()
            };
            Taxonomy::new(
                id,
                resolved.taxonomy_version(),
                resolved.taxonomy_description().clone(),
                None,
                resolved
                    .change_types()
                    .iter()
                    .map(crate::ResolvedChangeType::commit_type_definition)
                    .collect(),
            )
            .unwrap_or_else(|_| unreachable!("compiled taxonomy must be valid"))
        })
        .collect()
});

/// Returns the immutable built-in taxonomy library.
#[must_use]
pub fn built_in_taxonomies() -> &'static [Taxonomy] {
    &BUILT_INS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_ins_have_the_taxonomy_first_order() {
        assert_eq!(
            built_in_taxonomies()
                .iter()
                .map(|taxonomy| taxonomy.id().as_str())
                .collect::<Vec<_>>(),
            ["conventional", "research", "infra-ops"]
        );
    }

    #[test]
    fn fork_is_an_informational_snapshot() {
        let source = &built_in_taxonomies()[0];
        let fork = source.fork(TaxonomyId::from_trusted("team"));
        assert_eq!(fork.version(), TaxonomyVersion::V1);
        assert_eq!(fork.commit_types(), source.commit_types());
        assert_eq!(
            fork.derived_from().map(TaxonomyLineage::id),
            Some(source.id())
        );
    }
}
