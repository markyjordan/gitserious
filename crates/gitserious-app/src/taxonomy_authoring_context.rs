use std::collections::BTreeSet;

use gitserious_core::{
    CommitDraft, CommitMessage, CommitProvenance, CommitTypeDefinition, CommitTypeId,
    CommitValidationErrors, Taxonomy, TaxonomyId, render_commit_message_with_provenance,
};

use crate::fingerprint_taxonomy;

/// One portable taxonomy and its schema-ordered authoring definitions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitTaxonomy {
    taxonomy: Taxonomy,
}

impl CommitTaxonomy {
    /// Captures one immutable taxonomy for authoring.
    #[must_use]
    pub const fn new(taxonomy: Taxonomy) -> Self {
        Self { taxonomy }
    }

    /// Renders canonical content and taxonomy provenance.
    pub fn render(&self, draft: &CommitDraft) -> Result<CommitMessage, CommitValidationErrors> {
        let provenance =
            CommitProvenance::new(self.taxonomy.clone(), fingerprint_taxonomy(&self.taxonomy));
        render_commit_message_with_provenance(&provenance, draft)
    }

    /// Returns the taxonomy identity.
    #[must_use]
    pub const fn id(&self) -> &TaxonomyId {
        self.taxonomy.id()
    }

    /// Returns the complete taxonomy snapshot.
    #[must_use]
    pub const fn taxonomy(&self) -> &Taxonomy {
        &self.taxonomy
    }

    /// Returns commit types in taxonomy order.
    #[must_use]
    pub fn definitions(&self) -> &[CommitTypeDefinition] {
        self.taxonomy.commit_types()
    }
}

/// Available project taxonomies and the initial CLI/project selection.
#[derive(Clone, Debug)]
pub struct CommitAuthoringContext {
    taxonomies: Vec<CommitTaxonomy>,
    initial: usize,
    requested_type: Option<CommitTypeId>,
}

impl CommitAuthoringContext {
    /// Creates an authoring snapshot with an optional type preselection.
    pub fn new(
        taxonomies: Vec<Taxonomy>,
        initial: &TaxonomyId,
        requested_type: Option<&CommitTypeId>,
    ) -> Result<Self, String> {
        let mut identities = BTreeSet::new();
        for taxonomy in &taxonomies {
            if !identities.insert(taxonomy.id()) {
                return Err(format!("duplicate authoring taxonomy {}", taxonomy.id()));
            }
        }
        let taxonomies: Vec<_> = taxonomies.into_iter().map(CommitTaxonomy::new).collect();
        let initial = taxonomies
            .iter()
            .position(|taxonomy| taxonomy.id() == initial)
            .ok_or_else(|| format!("initial taxonomy {initial} is unavailable"))?;
        if let Some(id) = requested_type
            && !taxonomies[initial]
                .definitions()
                .iter()
                .any(|definition| definition.id() == id)
        {
            return Err(format!(
                "type {id} is unavailable in taxonomy {}",
                taxonomies[initial].id()
            ));
        }
        Ok(Self {
            taxonomies,
            initial,
            requested_type: requested_type.cloned(),
        })
    }

    /// Returns the immutable choices available for this commit.
    #[must_use]
    pub fn taxonomies(&self) -> &[CommitTaxonomy] {
        &self.taxonomies
    }

    /// Returns the explicit CLI selection or pinned project default.
    #[must_use]
    pub fn initial_taxonomy(&self) -> &CommitTaxonomy {
        &self.taxonomies[self.initial]
    }

    /// Finds a taxonomy within this captured policy.
    #[must_use]
    pub fn find_taxonomy(&self, id: &TaxonomyId) -> Option<&CommitTaxonomy> {
        self.taxonomies.iter().find(|taxonomy| taxonomy.id() == id)
    }

    /// Returns a type preselected within the initial taxonomy only.
    #[must_use]
    pub fn preselected_type(&self) -> Option<&CommitTypeDefinition> {
        self.requested_type.as_ref().and_then(|id| {
            self.initial_taxonomy()
                .definitions()
                .iter()
                .find(|definition| definition.id() == id)
        })
    }
}

/// An authored draft bound to the taxonomy chosen for this commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthoredCommit {
    taxonomy: TaxonomyId,
    draft: CommitDraft,
    reviewed: Option<CommitMessage>,
}

impl AuthoredCommit {
    /// Records a draft without certified reviewed bytes.
    #[must_use]
    pub const fn new(taxonomy: TaxonomyId, draft: CommitDraft) -> Self {
        Self {
            taxonomy,
            draft,
            reviewed: None,
        }
    }

    /// Records exactly the message approved by the user.
    #[must_use]
    pub const fn reviewed(
        taxonomy: TaxonomyId,
        draft: CommitDraft,
        message: CommitMessage,
    ) -> Self {
        Self {
            taxonomy,
            draft,
            reviewed: Some(message),
        }
    }

    /// Returns the approved message.
    #[must_use]
    pub const fn reviewed_message(&self) -> Option<&CommitMessage> {
        self.reviewed.as_ref()
    }

    /// Returns the selected taxonomy.
    #[must_use]
    pub const fn taxonomy(&self) -> &TaxonomyId {
        &self.taxonomy
    }

    /// Returns the authored draft.
    #[must_use]
    pub const fn draft(&self) -> &CommitDraft {
        &self.draft
    }
}

/// Result of a taxonomy-aware authoring interaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitAuthoringOutcome {
    /// Continue with the selected taxonomy and draft.
    Authored(AuthoredCommit),
    /// Leave the repository unchanged.
    Cancelled,
}
