//! A commit message of a property's type, cut to that property's block, with the trailers a
//! commit of its taxonomy would carry.

use gitserious_core::{
    CommitSample, Fingerprint, TaxonomyVersion, built_in_sample, built_in_taxonomies,
};

use super::{LibraryDraft, NodeId, NodeRef, PropertyNode, TaxonomyNode, problems};
use crate::TaxonomyOrigin;
use crate::taxonomy_policy::fingerprint_taxonomy;

/// One property's block in a message of its type. Later properties are left out.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Preview<'a> {
    /// The type's sample subject, else `<type>: <type description>`.
    pub subject: String,
    /// The keys of the properties above this one, in order.
    pub earlier: Vec<&'a str>,
    pub key: &'a str,
    pub value: PreviewValue<'a>,
    pub taxonomy: &'a str,
    /// The version apply would write.
    pub version: TaxonomyVersion,
    pub schema: Schema,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewValue<'a> {
    /// The value the type's sample gives this key.
    Sample(&'static str),
    /// No sample has this key, so the property's description stands in for a value.
    Placeholder(&'a str),
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
            taxonomy,
            commit_type,
            property,
        } = self.node(id)?
        else {
            return None;
        };
        let sample = self.sample(taxonomy, commit_type.name());
        Some(Preview {
            subject: sample.map_or_else(
                || format!("{}: {}", commit_type.name(), commit_type.description()),
                |sample| sample.subject().to_owned(),
            ),
            earlier: commit_type
                .properties()
                .iter()
                .take_while(|other| other.id() != id)
                .map(PropertyNode::name)
                .collect(),
            key: property.name(),
            value: sample
                .and_then(|sample| sample.value(property.name()))
                .map_or(
                    PreviewValue::Placeholder(property.description()),
                    PreviewValue::Sample,
                ),
            taxonomy: taxonomy.name(),
            version: self.version(id)?,
            schema: self.schema(taxonomy),
        })
    }

    /// The sample of the type named `commit_type` in `taxonomy`, or in the built-in that
    /// `taxonomy` was forked from, directly or through other forks.
    fn sample(&self, taxonomy: &TaxonomyNode, commit_type: &str) -> Option<&'static CommitSample> {
        std::iter::successors(Some(taxonomy), |node| {
            let source = node.derived_from()?.id().as_str();
            self.taxonomies.iter().find(|other| other.name == source)
        })
        .take(self.taxonomies.len())
        .filter(|node| node.origin == TaxonomyOrigin::BuiltIn)
        .find_map(|node| built_in_sample(&node.name, commit_type))
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
