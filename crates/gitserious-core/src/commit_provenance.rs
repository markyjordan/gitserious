use crate::{Fingerprint, ResolvedTaxonomy};

/// Immutable resolved policy used to validate and identify a generated commit.
///
/// Keeping the resolved schema with its provenance prevents callers from
/// validating against one type definition while rendering another template's
/// identities. The application supplies the schema's semantic fingerprint;
/// core owns its representation but does not compute or verify the digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitProvenance {
    schema: ResolvedTaxonomy,
    fingerprint: Fingerprint,
}

impl CommitProvenance {
    /// Binds resolved policy to its application-computed semantic fingerprint.
    ///
    /// The caller must compute the fingerprint from this exact resolved schema.
    /// No identifiers or versions are taken from authored commit fields.
    #[must_use]
    pub const fn new(schema: ResolvedTaxonomy, fingerprint: Fingerprint) -> Self {
        Self {
            schema,
            fingerprint,
        }
    }

    /// Returns the schema used for both validation and provenance rendering.
    #[must_use]
    pub const fn schema(&self) -> &ResolvedTaxonomy {
        &self.schema
    }

    /// Returns the semantic fingerprint supplied by the policy resolver.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}
