use std::fmt::{self, Display, Formatter};

use gitserious_core::{CommitTypeDefinition, CommitTypeId, Taxonomy};

use crate::GlobalConfiguration;

/// One custom taxonomy whose content differs between two global configurations.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum TaxonomyChange<'a> {
    Created(&'a Taxonomy),
    Edited {
        before: &'a Taxonomy,
        after: &'a Taxonomy,
        parts: Vec<EditedPart<'a>>,
    },
    Deleted(&'a Taxonomy),
}

/// One part of a custom taxonomy that an edit changed.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum EditedPart<'a> {
    Description,
    TypeAdded(&'a CommitTypeId),
    TypeChanged(&'a CommitTypeId),
    TypeRemoved(&'a CommitTypeId),
    TypesReordered,
}

/// Lists created and edited taxonomies in `working` order, then deleted ones in `original` order.
pub(crate) fn taxonomy_changes<'a>(
    original: &'a GlobalConfiguration,
    working: &'a GlobalConfiguration,
) -> Vec<TaxonomyChange<'a>> {
    let find = |library: &'a GlobalConfiguration, taxonomy: &Taxonomy| {
        library
            .custom_taxonomies()
            .iter()
            .find(|candidate| candidate.id() == taxonomy.id())
    };
    let created_or_edited =
        working
            .custom_taxonomies()
            .iter()
            .filter_map(|after| match find(original, after) {
                None => Some(TaxonomyChange::Created(after)),
                Some(before) if before != after => Some(TaxonomyChange::Edited {
                    before,
                    after,
                    parts: edited_parts(before, after),
                }),
                Some(_) => None,
            });
    let deleted = original
        .custom_taxonomies()
        .iter()
        .filter(|before| find(working, before).is_none())
        .map(TaxonomyChange::Deleted);
    created_or_edited.chain(deleted).collect()
}

fn edited_parts<'a>(before: &'a Taxonomy, after: &'a Taxonomy) -> Vec<EditedPart<'a>> {
    let find = |types: &'a [CommitTypeDefinition], id: &CommitTypeId| {
        types.iter().find(|definition| definition.id() == id)
    };
    let kept_order = |types: &'a [CommitTypeDefinition], other: &'a [CommitTypeDefinition]| {
        types
            .iter()
            .map(CommitTypeDefinition::id)
            .filter(|id| find(other, id).is_some())
            .collect::<Vec<_>>()
    };
    let mut parts = Vec::new();
    if before.description() != after.description() {
        parts.push(EditedPart::Description);
    }
    for definition in after.commit_types() {
        match find(before.commit_types(), definition.id()) {
            None => parts.push(EditedPart::TypeAdded(definition.id())),
            Some(previous) if previous != definition => {
                parts.push(EditedPart::TypeChanged(definition.id()));
            }
            Some(_) => {}
        }
    }
    parts.extend(
        before
            .commit_types()
            .iter()
            .filter(|definition| find(after.commit_types(), definition.id()).is_none())
            .map(|definition| EditedPart::TypeRemoved(definition.id())),
    );
    if kept_order(before.commit_types(), after.commit_types())
        != kept_order(after.commit_types(), before.commit_types())
    {
        parts.push(EditedPart::TypesReordered);
    }
    parts
}

impl Display for TaxonomyChange<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created(taxonomy) => {
                let count = taxonomy.commit_types().len();
                let noun = if count == 1 { "type" } else { "types" };
                write!(
                    formatter,
                    "created  {} v{}, {count} {noun}",
                    taxonomy.id(),
                    taxonomy.version()
                )
            }
            Self::Edited {
                before,
                after,
                parts,
            } => {
                write!(formatter, "edited   {} v{}", after.id(), before.version())?;
                if after.version() != before.version() {
                    write!(formatter, " -> v{}", after.version())?;
                }
                for (index, part) in parts.iter().enumerate() {
                    let separator = if index == 0 { ": " } else { "; " };
                    write!(formatter, "{separator}{part}")?;
                }
                Ok(())
            }
            Self::Deleted(taxonomy) => {
                write!(
                    formatter,
                    "deleted  {} v{}",
                    taxonomy.id(),
                    taxonomy.version()
                )
            }
        }
    }
}

impl Display for EditedPart<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Description => formatter.write_str("description"),
            Self::TypeAdded(id) => write!(formatter, "type {id} added"),
            Self::TypeChanged(id) => write!(formatter, "type {id} changed"),
            Self::TypeRemoved(id) => write!(formatter, "type {id} removed"),
            Self::TypesReordered => formatter.write_str("types reordered"),
        }
    }
}
