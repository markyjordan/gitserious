use std::collections::BTreeSet;

use gitserious_core::{
    CommitTypeDefinition, Description, Taxonomy, TaxonomyError, TaxonomyId, TaxonomyLineage,
    TaxonomyVersion,
};

/// A validated stored Library entry, including entries not yet eligible for projects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryTaxonomy {
    id: TaxonomyId,
    version: TaxonomyVersion,
    description: Description,
    derived_from: Option<TaxonomyLineage>,
    commit_types: Vec<CommitTypeDefinition>,
    name_locked: bool,
}

impl LibraryTaxonomy {
    /// Validates type uniqueness. An entry with types always has a finalized name.
    pub fn new(
        id: TaxonomyId,
        version: TaxonomyVersion,
        description: Description,
        derived_from: Option<TaxonomyLineage>,
        commit_types: Vec<CommitTypeDefinition>,
        name_locked: bool,
    ) -> Result<Self, TaxonomyError> {
        let mut ids = BTreeSet::new();
        for definition in &commit_types {
            if !ids.insert(definition.id()) {
                return Err(TaxonomyError::DuplicateCommitType(definition.id().clone()));
            }
        }
        Ok(Self {
            name_locked: name_locked || !commit_types.is_empty(),
            id,
            version,
            description,
            derived_from,
            commit_types,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &TaxonomyId {
        &self.id
    }

    #[must_use]
    pub const fn version(&self) -> TaxonomyVersion {
        self.version
    }

    #[must_use]
    pub const fn description(&self) -> &Description {
        &self.description
    }

    #[must_use]
    pub const fn derived_from(&self) -> Option<&TaxonomyLineage> {
        self.derived_from.as_ref()
    }

    #[must_use]
    pub fn commit_types(&self) -> &[CommitTypeDefinition] {
        &self.commit_types
    }

    #[must_use]
    pub const fn name_locked(&self) -> bool {
        self.name_locked
    }

    /// Produces project policy only when at least one type exists.
    #[must_use]
    pub fn usable(&self) -> Option<Taxonomy> {
        Taxonomy::new(
            self.id.clone(),
            self.version,
            self.description.clone(),
            self.derived_from.clone(),
            self.commit_types.clone(),
        )
        .ok()
    }
}

impl From<Taxonomy> for LibraryTaxonomy {
    fn from(taxonomy: Taxonomy) -> Self {
        Self {
            id: taxonomy.id().clone(),
            version: taxonomy.version(),
            description: taxonomy.description().clone(),
            derived_from: taxonomy.derived_from().cloned(),
            commit_types: taxonomy.commit_types().to_vec(),
            name_locked: true,
        }
    }
}
