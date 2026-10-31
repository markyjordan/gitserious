//! What apply would change in the custom taxonomies, node by node, against each node's baseline.
//!
//! A fork's nodes compare with the nodes of its source, so a fresh fork carries no marks below
//! its taxonomy. Text compares as apply would write it, so restoring a field clears its mark.

use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};

use gitserious_core::{
    CommitTypeId, ConditionId, LabelError, PropertyKey, TaxonomyId, TaxonomyVersion,
    built_in_taxonomies,
};

use super::{LibraryDraft, LoadedPath, NodeId, Policy, PropertyNode, TaxonomyNode, TypeNode};
use crate::TaxonomyOrigin;

/// How a node differs from its baseline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mark {
    /// Created, forked, or added in this session.
    New,
    /// Its own fields, the order of its children, or something beneath it differs.
    Modified,
}

/// Every unapplied change to the custom taxonomies: a mark per changed node and the changes
/// the review lists and the count adds up.
#[derive(Clone, Debug, Default)]
pub struct ChangeSet {
    marks: HashMap<NodeId, Mark>,
    taxonomies: Vec<TaxonomyChange>,
}

impl ChangeSet {
    #[must_use]
    pub fn mark(&self, id: &NodeId) -> Option<Mark> {
        self.marks.get(id).copied()
    }

    /// One per created, forked, or deleted taxonomy and per new, edited, deleted, or reordered
    /// part beneath one. A taxonomy or type modified only through its descendants adds nothing.
    #[must_use]
    pub fn count(&self) -> usize {
        self.taxonomies.iter().map(TaxonomyChange::count).sum()
    }

    /// Changed taxonomies in outline order, then deleted ones in stored order.
    pub(crate) fn taxonomies(&self) -> &[TaxonomyChange] {
        &self.taxonomies
    }
}

/// One taxonomy with changes, as the review names it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TaxonomyChange {
    name: String,
    status: Status,
    parts: Vec<Part>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Status {
    Created {
        version: TaxonomyVersion,
        types: usize,
    },
    Edited {
        from: TaxonomyVersion,
        to: TaxonomyVersion,
    },
    Deleted {
        version: TaxonomyVersion,
    },
}

/// One counted change beneath a taxonomy, or to its own description or type order. Names are
/// the identifiers apply would write, or for a removed node the stored one.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Part {
    Description,
    TypesReordered,
    Type(String, Kind),
    PropertiesReordered(String),
    Property(String, String, Kind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Added,
    Edited,
    Removed,
}

impl TaxonomyChange {
    fn count(&self) -> usize {
        let own = match self.status {
            Status::Created { .. } | Status::Deleted { .. } => 1,
            Status::Edited { .. } => 0,
        };
        own + self.parts.len()
    }
}

impl LibraryDraft {
    /// The changes apply would write to the custom taxonomies. The selection of default and
    /// available taxonomies is not part of it.
    #[must_use]
    pub fn changes(&self) -> ChangeSet {
        let stored = built_in_taxonomies()
            .iter()
            .map(|taxonomy| TaxonomyNode::new(taxonomy, TaxonomyOrigin::BuiltIn))
            .chain(
                self.persisted
                    .custom_taxonomies()
                    .iter()
                    .map(|taxonomy| TaxonomyNode::new(taxonomy, TaxonomyOrigin::Custom)),
            )
            .collect::<Vec<_>>();
        let mut changes = ChangeSet::default();
        for taxonomy in &self.taxonomies {
            if taxonomy.origin == TaxonomyOrigin::Custom {
                let baseline = matching(&stored, taxonomy.baseline(), TaxonomyNode::baseline);
                changes.taxonomy(self, taxonomy, baseline);
            }
        }
        for gone in stored.iter().filter(|stored| {
            stored.origin == TaxonomyOrigin::Custom
                && !self
                    .taxonomies
                    .iter()
                    .any(|taxonomy| taxonomy.id == stored.id)
        }) {
            changes.taxonomies.push(TaxonomyChange {
                name: gone.name.clone(),
                status: Status::Deleted {
                    version: gone.version,
                },
                parts: Vec::new(),
            });
        }
        changes
    }
}

impl ChangeSet {
    fn taxonomy(
        &mut self,
        draft: &LibraryDraft,
        taxonomy: &TaxonomyNode,
        baseline: Option<&TaxonomyNode>,
    ) {
        let mut parts = Vec::new();
        if let Some(baseline) = baseline {
            if !same_text(&taxonomy.description, &baseline.description) {
                parts.push(Part::Description);
            }
            if reordered(&taxonomy.types, &baseline.types, TypeNode::baseline) {
                parts.push(Part::TypesReordered);
            }
        }
        for commit_type in &taxonomy.types {
            let stored = baseline.and_then(|baseline| {
                matching(&baseline.types, commit_type.baseline(), TypeNode::baseline)
            });
            self.commit_type(&mut parts, commit_type, stored, baseline.is_some());
        }
        if let Some(baseline) = baseline {
            parts.extend(
                removed(&taxonomy.types, &baseline.types, TypeNode::baseline)
                    .map(|gone| Part::Type(gone.name.clone(), Kind::Removed)),
            );
        }
        let (status, mark) = match &taxonomy.id {
            NodeId::Local(_) => (
                Status::Created {
                    version: taxonomy.version,
                    types: taxonomy.types.len(),
                },
                Mark::New,
            ),
            NodeId::Loaded(_) if parts.is_empty() => return,
            NodeId::Loaded(_) => (
                Status::Edited {
                    from: taxonomy.version,
                    to: draft.version(&taxonomy.id).unwrap_or(taxonomy.version),
                },
                Mark::Modified,
            ),
        };
        self.marks.insert(taxonomy.id.clone(), mark);
        self.taxonomies.push(TaxonomyChange {
            name: written(TaxonomyId::from_label, &taxonomy.name),
            status,
            parts,
        });
    }

    /// Marks `commit_type` and its properties and records their parts. A new type counts only
    /// when `counted`, which is false inside a created taxonomy, and its properties never do.
    fn commit_type(
        &mut self,
        parts: &mut Vec<Part>,
        commit_type: &TypeNode,
        baseline: Option<&TypeNode>,
        counted: bool,
    ) {
        let name = written(CommitTypeId::from_label, &commit_type.name);
        let Some(baseline) = baseline else {
            self.marks.insert(commit_type.id.clone(), Mark::New);
            for property in &commit_type.properties {
                self.marks.insert(property.id.clone(), Mark::New);
            }
            if counted {
                parts.push(Part::Type(name.clone(), Kind::Added));
            }
            return;
        };
        let before = parts.len();
        if !same_label(CommitTypeId::from_label, &commit_type.name, &baseline.name)
            || !same_text(&commit_type.description, &baseline.description)
        {
            parts.push(Part::Type(name.clone(), Kind::Edited));
        }
        if reordered(
            &commit_type.properties,
            &baseline.properties,
            PropertyNode::baseline,
        ) {
            parts.push(Part::PropertiesReordered(name.clone()));
        }
        for property in &commit_type.properties {
            let kind = match matching(
                &baseline.properties,
                property.baseline(),
                PropertyNode::baseline,
            ) {
                None => Kind::Added,
                Some(stored) if !same_property(property, stored) => Kind::Edited,
                Some(_) => continue,
            };
            let mark = if kind == Kind::Added {
                Mark::New
            } else {
                Mark::Modified
            };
            self.marks.insert(property.id.clone(), mark);
            let key = written(PropertyKey::from_label, &property.name);
            parts.push(Part::Property(name.clone(), key, kind));
        }
        parts.extend(
            removed(
                &commit_type.properties,
                &baseline.properties,
                PropertyNode::baseline,
            )
            .map(|gone| Part::Property(name.clone(), gone.name.clone(), Kind::Removed)),
        );
        if parts.len() > before {
            self.marks.insert(commit_type.id.clone(), Mark::Modified);
        }
    }
}

/// The node of `nodes` whose baseline is `path`.
fn matching<'a, T>(
    nodes: &'a [T],
    path: Option<&LoadedPath>,
    baseline: fn(&T) -> Option<&LoadedPath>,
) -> Option<&'a T> {
    let path = path?;
    nodes.iter().find(|node| baseline(node) == Some(path))
}

/// The stored nodes no current node keeps the baseline of.
fn removed<'a, T>(
    current: &'a [T],
    stored: &'a [T],
    baseline: fn(&T) -> Option<&LoadedPath>,
) -> impl Iterator<Item = &'a T> {
    stored
        .iter()
        .filter(move |node| matching(current, baseline(node), baseline).is_none())
}

/// Whether the nodes both lists hold are in a different order.
fn reordered<T>(current: &[T], stored: &[T], baseline: fn(&T) -> Option<&LoadedPath>) -> bool {
    let kept = |nodes: &[T], others: &[T]| {
        nodes
            .iter()
            .filter_map(baseline)
            .filter(|path| matching(others, Some(path), baseline).is_some())
            .cloned()
            .collect::<Vec<_>>()
    };
    kept(current, stored) != kept(stored, current)
}

fn same_property(property: &PropertyNode, stored: &PropertyNode) -> bool {
    same_label(PropertyKey::from_label, &property.name, &stored.name)
        && same_text(&property.description, &stored.description)
        && property.policy == stored.policy
        && (property.policy != Policy::Conditional
            || same_label(
                ConditionId::from_label,
                &property.condition,
                &stored.condition,
            ) && same_text(&property.rationale, &stored.rationale))
}

/// Whether apply writes `draft` as `stored`: edited prose is trimmed at its edges.
fn same_text(draft: &str, stored: &str) -> bool {
    draft == stored || draft.trim() == stored
}

/// The identifier apply would write for `name`, or `name` itself when it is not one.
pub(super) fn written<T: AsRef<str>>(
    parse: fn(&str) -> Result<T, LabelError>,
    name: &str,
) -> String {
    parse(name).map_or_else(|_| name.to_owned(), |id| id.as_ref().to_owned())
}

/// Whether apply writes the identifier `draft` as `stored`.
fn same_label<T: AsRef<str>>(
    parse: fn(&str) -> Result<T, LabelError>,
    draft: &str,
    stored: &str,
) -> bool {
    draft == stored || parse(draft).is_ok_and(|id| id.as_ref() == stored)
}

impl Display for TaxonomyChange {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let name = &self.name;
        match self.status {
            Status::Created { version, types } => {
                let noun = if types == 1 { "type" } else { "types" };
                write!(formatter, "created  {name} v{version}, {types} {noun}")?;
            }
            Status::Edited { from, to } => {
                write!(formatter, "edited   {name} v{from}")?;
                if to != from {
                    write!(formatter, " -> v{to}")?;
                }
            }
            Status::Deleted { version } => write!(formatter, "deleted  {name} v{version}")?,
        }
        for (index, part) in self.parts.iter().enumerate() {
            let separator = if index == 0 { ": " } else { "; " };
            write!(formatter, "{separator}{part}")?;
        }
        Ok(())
    }
}

impl Display for Part {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Description => formatter.write_str("description"),
            Self::TypesReordered => formatter.write_str("types reordered"),
            Self::Type(name, kind) => write!(formatter, "type {name} {kind}"),
            Self::PropertiesReordered(name) => write!(formatter, "{name} properties reordered"),
            Self::Property(commit_type, name, kind) => {
                write!(formatter, "property {commit_type}.{name} {kind}")
            }
        }
    }
}

impl Display for Kind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Added => "added",
            Self::Edited => "edited",
            Self::Removed => "removed",
        })
    }
}
