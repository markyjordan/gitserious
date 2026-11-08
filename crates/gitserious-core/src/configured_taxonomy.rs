use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::sync::LazyLock;

use crate::{
    CommitTypeDefinition, CommitTypeId, Description, TaxonomyId, TaxonomyReference,
    TaxonomyVersion, built_in_commit_types,
};

const BUILT_IN_IDS: [&str; 3] = ["conventional", "research", "infra-ops"];

/// Informational source recorded when one taxonomy is forked from another.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaxonomyLineage {
    id: TaxonomyId,
    version: Option<TaxonomyVersion>,
}

impl TaxonomyLineage {
    /// Records source identity and an optional built-in release without an update relationship.
    #[must_use]
    pub fn new(id: TaxonomyId, version: Option<TaxonomyVersion>) -> Self {
        let version = version.filter(|_| {
            built_in_taxonomies()
                .iter()
                .any(|source| source.id() == &id)
        });
        Self { id, version }
    }

    /// Returns the source taxonomy identifier.
    #[must_use]
    pub const fn id(&self) -> &TaxonomyId {
        &self.id
    }

    /// Returns the source taxonomy version at fork time.
    #[must_use]
    pub const fn version(&self) -> Option<TaxonomyVersion> {
        self.version
    }

    /// Returns the source identity with its optional built-in release.
    #[must_use]
    pub const fn reference(&self) -> TaxonomyReference<'_> {
        TaxonomyReference::new(&self.id, self.version)
    }
}

/// One complete, selectable commit taxonomy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Taxonomy {
    id: TaxonomyId,
    version: Option<TaxonomyVersion>,
    description: Description,
    derived_from: Option<TaxonomyLineage>,
    commit_types: Vec<CommitTypeDefinition>,
}

impl Taxonomy {
    /// Creates a complete taxonomy with unique, ordered commit types.
    /// Custom identities discard taxonomy and type release metadata.
    ///
    /// # Errors
    /// Returns an error when no commit types are supplied or an identifier is repeated.
    pub fn new(
        id: TaxonomyId,
        version: Option<TaxonomyVersion>,
        description: Description,
        derived_from: Option<TaxonomyLineage>,
        commit_types: Vec<CommitTypeDefinition>,
    ) -> Result<Self, TaxonomyError> {
        let version = version.filter(|_| BUILT_IN_IDS.contains(&id.as_str()));
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
            commit_types: if version.is_none() {
                commit_types
                    .into_iter()
                    .map(CommitTypeDefinition::into_custom)
                    .collect()
            } else {
                commit_types
            },
        })
    }

    /// Returns the taxonomy identifier.
    #[must_use]
    pub const fn id(&self) -> &TaxonomyId {
        &self.id
    }

    /// Returns the optional built-in taxonomy release version.
    #[must_use]
    pub const fn version(&self) -> Option<TaxonomyVersion> {
        self.version
    }

    /// Returns the taxonomy identity with its optional built-in release.
    #[must_use]
    pub const fn reference(&self) -> TaxonomyReference<'_> {
        TaxonomyReference::new(&self.id, self.version)
    }

    /// Removes release metadata from this taxonomy and all its types.
    #[must_use]
    pub fn into_custom(mut self) -> Self {
        self.version = None;
        self.commit_types = self
            .commit_types
            .into_iter()
            .map(CommitTypeDefinition::into_custom)
            .collect();
        self
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
                write!(formatter, "taxonomy repeats commit type \"{id}\"")
            }
        }
    }
}

impl Error for TaxonomyError {}

static BUILT_INS: LazyLock<Vec<Taxonomy>> = LazyLock::new(|| {
    vec![
        built_in(
            BUILT_IN_IDS[0],
            "The Conventional Commits classification system for software changes.",
            built_in_commit_types(),
        ),
        built_in(
            BUILT_IN_IDS[1],
            "Change categories for empirical machine-learning research.",
            crate::built_in_ml_research::commit_types(),
        ),
        built_in(
            BUILT_IN_IDS[2],
            "Change categories for infrastructure and operational systems.",
            crate::built_in_infra_ops::commit_types(),
        ),
    ]
});

fn built_in(
    id: &'static str,
    description: &'static str,
    commit_types: &[CommitTypeDefinition],
) -> Taxonomy {
    Taxonomy::new(
        TaxonomyId::from_trusted(id),
        Some(TaxonomyVersion::V1),
        Description::from_trusted(description),
        None,
        commit_types.to_vec(),
    )
    .unwrap_or_else(|_| unreachable!("compiled taxonomy must be valid"))
}

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
    fn snapshot_lineage_is_informational() -> Result<(), TaxonomyError> {
        let source = &built_in_taxonomies()[0];
        let fork = Taxonomy::new(
            TaxonomyId::from_trusted("team"),
            Some(TaxonomyVersion::V1),
            source.description().clone(),
            Some(TaxonomyLineage::new(source.id().clone(), source.version())),
            source.commit_types().to_vec(),
        )?;
        assert_eq!(fork.version(), None);
        assert_eq!(
            fork.commit_types(),
            source.clone().into_custom().commit_types()
        );
        assert_eq!(
            fork.derived_from().map(TaxonomyLineage::id),
            Some(source.id())
        );
        Ok(())
    }
}
