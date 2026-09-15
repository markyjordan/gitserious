use crate::{Fingerprint, Taxonomy};

/// Immutable taxonomy snapshot used to validate and identify a generated commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitProvenance {
    taxonomy: Taxonomy,
    fingerprint: Fingerprint,
}

impl CommitProvenance {
    /// Binds an exact taxonomy snapshot to its semantic fingerprint.
    #[must_use]
    pub const fn new(taxonomy: Taxonomy, fingerprint: Fingerprint) -> Self {
        Self {
            taxonomy,
            fingerprint,
        }
    }

    /// Returns the taxonomy used for validation and provenance rendering.
    #[must_use]
    pub const fn taxonomy(&self) -> &Taxonomy {
        &self.taxonomy
    }

    /// Returns the semantic fingerprint supplied by the policy resolver.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        CommitDraft, CommitSubject, CommitTypeDefinition, CommitTypeId, Description, Fingerprint,
        SchemaVersion, Taxonomy, TaxonomyId, TaxonomyVersion,
        render_commit_message_with_provenance,
    };

    use super::CommitProvenance;

    #[test]
    fn rendering_emits_only_taxonomy_and_schema_trailers() {
        let definition = CommitTypeDefinition::new(
            SchemaVersion::V1,
            CommitTypeId::new("note").unwrap_or_else(|_| unreachable!()),
            "Record a note.",
            Vec::new(),
        )
        .unwrap_or_else(|_| unreachable!());
        let taxonomy = Taxonomy::new(
            TaxonomyId::new("plain").unwrap_or_else(|_| unreachable!()),
            TaxonomyVersion::V1,
            Description::new("A plain taxonomy.").unwrap_or_else(|_| unreachable!()),
            None,
            vec![definition],
        )
        .unwrap_or_else(|_| unreachable!());
        let draft = CommitDraft::new(
            CommitTypeId::new("note").unwrap_or_else(|_| unreachable!()),
            None,
            CommitSubject::new("record context").unwrap_or_else(|_| unreachable!()),
            Vec::new(),
        )
        .unwrap_or_else(|_| unreachable!());
        let message = render_commit_message_with_provenance(
            &CommitProvenance::new(taxonomy, Fingerprint::from_bytes([0; 32])),
            &draft,
        )
        .unwrap_or_else(|_| unreachable!());
        assert!(message.as_str().contains("Gitserious-Taxonomy: plain@1"));
        assert!(message.as_str().contains("Gitserious-Schema: sha256:"));
        assert!(!message.as_str().contains("Gitserious-Template"));
        assert!(!message.as_str().contains("Gitserious-Typeset"));
    }
}
