//! A property's complete example message and its taxonomy provenance.

use gitserious_core::{Fingerprint, TaxonomyVersion, built_in_taxonomies};

use super::{LibraryDraft, NodeId, NodeRef, TaxonomyNode, problems};
use crate::TaxonomyOrigin;
use crate::taxonomy_policy::fingerprint_taxonomy;

/// The resolved Example field text and the taxonomy it belongs to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Preview<'a> {
    /// The same complete message displayed by the Example field.
    pub message: &'a str,
    pub taxonomy: &'a str,
    /// The version apply would write.
    pub version: TaxonomyVersion,
    pub schema: Schema,
}

/// The schema fingerprint a commit of the taxonomy would carry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Schema {
    /// The taxonomy builds; this is its fingerprint now.
    Current(Fingerprint),
    /// The taxonomy has problems; this is the fingerprint of its last valid build.
    LastValid(Fingerprint),
    /// The taxonomy has problems and has not built since it was created.
    Unavailable,
}

impl LibraryDraft {
    /// The preview of the property `id`; none for a taxonomy or type.
    #[must_use]
    pub fn preview(&self, id: &NodeId) -> Option<Preview<'_>> {
        let NodeRef::Property {
            taxonomy, property, ..
        } = self.node(id)?
        else {
            return None;
        };
        Some(Preview {
            message: property.example.text(),
            taxonomy: taxonomy.name(),
            version: self.version(id)?,
            schema: self.schema(taxonomy),
        })
    }

    fn schema(&self, taxonomy: &TaxonomyNode) -> Schema {
        let current = match taxonomy.origin {
            TaxonomyOrigin::BuiltIn => built_in_taxonomies()
                .iter()
                .find(|built_in| built_in.id().as_str() == taxonomy.name)
                .map(fingerprint_taxonomy),
            TaxonomyOrigin::Custom => problems::custom(self)
                .0
                .into_iter()
                .find(|(index, _)| self.taxonomies[*index].id == taxonomy.id)
                .map(|(_, built)| fingerprint_taxonomy(&built)),
        };
        match (current, &taxonomy.last_valid) {
            (Some(fingerprint), _) => Schema::Current(fingerprint),
            (None, Some(last_valid)) => Schema::LastValid(fingerprint_taxonomy(last_valid)),
            (None, None) => Schema::Unavailable,
        }
    }
}
