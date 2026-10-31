//! The Library as plain drafts: every taxonomy, built-ins first, with its types and properties.
//!
//! Building a draft from the same configuration twice yields equal node ids, so a view can keep
//! a node id across rebuilds.

use std::num::NonZeroU32;

use gitserious_core::{
    CommitTypeDefinition, PropertyDefinition, PropertyRequirement, Taxonomy, TaxonomyLineage,
    TaxonomyVersion, built_in_taxonomies,
};

use crate::{GlobalConfiguration, TaxonomyOrigin};

/// The identity of one taxonomy, type, or property draft. Never persisted.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NodeId {
    /// A node read from the configuration. It keeps this id when it is renamed.
    Loaded(LoadedPath),
    /// A node created in this session.
    Local(NonZeroU32),
}

/// The taxonomy, type, and property ids of a node as the configuration names it.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LoadedPath(Vec<String>);

impl NodeId {
    fn loaded(path: &[&str]) -> Self {
        Self::Loaded(LoadedPath(path.iter().map(|&id| id.to_owned()).collect()))
    }
}

/// How strongly a commit type expects a property.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Policy {
    Required,
    Recommended,
    Conditional,
    Optional,
}

impl Policy {
    /// Every policy in display order.
    pub const ORDER: [Self; 4] = [
        Self::Required,
        Self::Recommended,
        Self::Conditional,
        Self::Optional,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Recommended => "recommended",
            Self::Conditional => "conditional",
            Self::Optional => "optional",
        }
    }
}

/// A field a taxonomy, type, or property shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftField {
    Name,
    Description,
    Policy,
    Condition,
    Rationale,
}

impl DraftField {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Description => "Description",
            Self::Policy => "Policy",
            Self::Condition => "Condition",
            Self::Rationale => "Rationale",
        }
    }
}

#[derive(Clone, Debug)]
pub struct TaxonomyNode {
    id: NodeId,
    origin: TaxonomyOrigin,
    name: String,
    description: String,
    version: TaxonomyVersion,
    derived_from: Option<TaxonomyLineage>,
    types: Vec<TypeNode>,
}

impl TaxonomyNode {
    fn new(taxonomy: &Taxonomy, origin: TaxonomyOrigin) -> Self {
        let name = taxonomy.id().as_str();
        Self {
            id: NodeId::loaded(&[name]),
            origin,
            name: name.to_owned(),
            description: taxonomy.description().to_string(),
            version: taxonomy.version(),
            derived_from: taxonomy.derived_from().cloned(),
            types: taxonomy
                .commit_types()
                .iter()
                .map(|commit_type| TypeNode::new(name, commit_type))
                .collect(),
        }
    }

    #[must_use]
    pub const fn id(&self) -> &NodeId {
        &self.id
    }

    #[must_use]
    pub const fn origin(&self) -> TaxonomyOrigin {
        self.origin
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    #[must_use]
    pub const fn derived_from(&self) -> Option<&TaxonomyLineage> {
        self.derived_from.as_ref()
    }

    #[must_use]
    pub fn types(&self) -> &[TypeNode] {
        &self.types
    }
}

#[derive(Clone, Debug)]
pub struct TypeNode {
    id: NodeId,
    name: String,
    description: String,
    properties: Vec<PropertyNode>,
}

impl TypeNode {
    fn new(taxonomy: &str, commit_type: &CommitTypeDefinition) -> Self {
        let name = commit_type.id().as_str();
        Self {
            id: NodeId::loaded(&[taxonomy, name]),
            name: name.to_owned(),
            description: commit_type.description().to_owned(),
            properties: commit_type
                .properties()
                .iter()
                .map(|property| PropertyNode::new(taxonomy, name, property))
                .collect(),
        }
    }

    #[must_use]
    pub const fn id(&self) -> &NodeId {
        &self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    #[must_use]
    pub fn properties(&self) -> &[PropertyNode] {
        &self.properties
    }
}

/// A property. `condition` and `rationale` are empty unless the policy is conditional.
#[derive(Clone, Debug)]
pub struct PropertyNode {
    id: NodeId,
    name: String,
    description: String,
    policy: Policy,
    condition: String,
    rationale: String,
}

impl PropertyNode {
    fn new(taxonomy: &str, commit_type: &str, property: &PropertyDefinition) -> Self {
        let name = property.key().as_str();
        let (policy, condition, rationale) = match property.requirement() {
            PropertyRequirement::Required => (Policy::Required, String::new(), String::new()),
            PropertyRequirement::Recommended => (Policy::Recommended, String::new(), String::new()),
            PropertyRequirement::Optional => (Policy::Optional, String::new(), String::new()),
            PropertyRequirement::Conditional(condition) => (
                Policy::Conditional,
                condition.id().to_string(),
                condition.rationale().to_owned(),
            ),
        };
        Self {
            id: NodeId::loaded(&[taxonomy, commit_type, name]),
            name: name.to_owned(),
            description: property.description().to_owned(),
            policy,
            condition,
            rationale,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &NodeId {
        &self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    #[must_use]
    pub const fn policy(&self) -> Policy {
        self.policy
    }

    #[must_use]
    pub fn condition(&self) -> &str {
        &self.condition
    }

    #[must_use]
    pub fn rationale(&self) -> &str {
        &self.rationale
    }
}

/// A node with its ancestors.
#[derive(Clone, Copy, Debug)]
pub enum NodeRef<'a> {
    Taxonomy(&'a TaxonomyNode),
    Type {
        taxonomy: &'a TaxonomyNode,
        commit_type: &'a TypeNode,
    },
    Property {
        taxonomy: &'a TaxonomyNode,
        commit_type: &'a TypeNode,
        property: &'a PropertyNode,
    },
}

impl<'a> NodeRef<'a> {
    #[must_use]
    pub const fn id(self) -> &'a NodeId {
        match self {
            Self::Taxonomy(taxonomy) => &taxonomy.id,
            Self::Type { commit_type, .. } => &commit_type.id,
            Self::Property { property, .. } => &property.id,
        }
    }

    /// The taxonomy this node belongs to, or the node itself.
    #[must_use]
    pub const fn taxonomy(self) -> &'a TaxonomyNode {
        match self {
            Self::Taxonomy(taxonomy)
            | Self::Type { taxonomy, .. }
            | Self::Property { taxonomy, .. } => taxonomy,
        }
    }

    /// Names from the taxonomy down to this node.
    #[must_use]
    pub fn names(self) -> Vec<&'a str> {
        match self {
            Self::Taxonomy(taxonomy) => vec![taxonomy.name()],
            Self::Type {
                taxonomy,
                commit_type,
            } => vec![taxonomy.name(), commit_type.name()],
            Self::Property {
                taxonomy,
                commit_type,
                property,
            } => vec![taxonomy.name(), commit_type.name(), property.name()],
        }
    }
}

/// Every taxonomy of one Library: built-ins, then custom taxonomies in storage order.
#[derive(Clone, Debug)]
pub struct LibraryDraft {
    taxonomies: Vec<TaxonomyNode>,
}

impl LibraryDraft {
    #[must_use]
    pub fn from_configuration(configuration: &GlobalConfiguration) -> Self {
        let built_in = built_in_taxonomies()
            .iter()
            .map(|taxonomy| TaxonomyNode::new(taxonomy, TaxonomyOrigin::BuiltIn));
        let custom = configuration
            .custom_taxonomies()
            .iter()
            .map(|taxonomy| TaxonomyNode::new(taxonomy, TaxonomyOrigin::Custom));
        Self {
            taxonomies: built_in.chain(custom).collect(),
        }
    }

    #[must_use]
    pub fn taxonomies(&self) -> &[TaxonomyNode] {
        &self.taxonomies
    }

    #[must_use]
    pub fn node(&self, id: &NodeId) -> Option<NodeRef<'_>> {
        for taxonomy in &self.taxonomies {
            if &taxonomy.id == id {
                return Some(NodeRef::Taxonomy(taxonomy));
            }
            for commit_type in &taxonomy.types {
                if &commit_type.id == id {
                    return Some(NodeRef::Type {
                        taxonomy,
                        commit_type,
                    });
                }
                if let Some(property) = commit_type.properties.iter().find(|p| &p.id == id) {
                    return Some(NodeRef::Property {
                        taxonomy,
                        commit_type,
                        property,
                    });
                }
            }
        }
        None
    }

    #[must_use]
    pub fn parent(&self, id: &NodeId) -> Option<NodeId> {
        match self.node(id)? {
            NodeRef::Taxonomy(_) => None,
            NodeRef::Type { taxonomy, .. } => Some(taxonomy.id.clone()),
            NodeRef::Property { commit_type, .. } => Some(commit_type.id.clone()),
        }
    }

    /// Finds a node by its current names, from the taxonomy down.
    #[must_use]
    #[cfg(test)]
    fn find_path(&self, names: &[&str]) -> Option<NodeId> {
        let (first, rest) = names.split_first()?;
        let taxonomy = self.taxonomies.iter().find(|node| node.name == *first)?;
        let Some((type_name, rest)) = rest.split_first() else {
            return Some(taxonomy.id.clone());
        };
        let commit_type = taxonomy.types.iter().find(|node| node.name == *type_name)?;
        match rest {
            [] => Some(commit_type.id.clone()),
            [property] => commit_type
                .properties
                .iter()
                .find(|node| node.name == *property)
                .map(|node| node.id.clone()),
            _ => None,
        }
    }

    /// The version of the taxonomy that owns `id`.
    #[must_use]
    pub fn version(&self, id: &NodeId) -> Option<TaxonomyVersion> {
        self.node(id).map(|node| node.taxonomy().version)
    }
}

#[cfg(test)]
mod tests {
    use gitserious_core::{
        CommitTypeId, ConditionId, Description, PropertyCondition, PropertyKey, SchemaVersion,
        TaxonomyId,
    };

    use super::*;

    fn team() -> Result<GlobalConfiguration, Box<dyn std::error::Error>> {
        let release = CommitTypeDefinition::new(
            SchemaVersion::new(1)?,
            CommitTypeId::new("release")?,
            "Release notes",
            vec![
                PropertyDefinition::new(
                    PropertyKey::new("summary")?,
                    "What shipped",
                    PropertyRequirement::Required,
                )?,
                PropertyDefinition::new(
                    PropertyKey::new("rollback")?,
                    "How to undo it",
                    PropertyRequirement::Conditional(PropertyCondition::new(
                        ConditionId::new("risky")?,
                        "Risky releases need a way back.",
                    )?),
                )?,
            ],
        )?;
        let team = Taxonomy::new(
            TaxonomyId::new("team")?,
            TaxonomyVersion::new(3)?,
            Description::new("Team taxonomy")?,
            None,
            vec![release],
        )?;
        let zeta = built_in_taxonomies()[0].fork(TaxonomyId::new("zeta")?);
        let conventional = TaxonomyId::new("conventional")?;
        Ok(GlobalConfiguration::new(
            1,
            conventional.clone(),
            vec![conventional],
            vec![zeta, team],
        )?)
    }

    #[test]
    fn built_ins_come_first_then_custom_taxonomies_in_storage_order()
    -> Result<(), Box<dyn std::error::Error>> {
        let draft = LibraryDraft::from_configuration(&team()?);
        let rows = draft
            .taxonomies()
            .iter()
            .map(|taxonomy| (taxonomy.name(), taxonomy.origin()))
            .collect::<Vec<_>>();
        assert_eq!(
            rows,
            [
                ("conventional", TaxonomyOrigin::BuiltIn),
                ("research", TaxonomyOrigin::BuiltIn),
                ("infra-ops", TaxonomyOrigin::BuiltIn),
                ("zeta", TaxonomyOrigin::Custom),
                ("team", TaxonomyOrigin::Custom),
            ]
        );
        assert_eq!(
            draft.taxonomies()[3].derived_from().map(|source| format!(
                "{}@{}",
                source.id(),
                source.version()
            )),
            Some("conventional@2".into())
        );
        Ok(())
    }

    #[test]
    fn rebuilding_from_the_same_configuration_keeps_every_node_id()
    -> Result<(), Box<dyn std::error::Error>> {
        let configuration = team()?;
        let ids = |draft: &LibraryDraft| {
            draft
                .taxonomies()
                .iter()
                .flat_map(|taxonomy| {
                    std::iter::once(taxonomy.id().clone()).chain(taxonomy.types().iter().flat_map(
                        |commit_type| {
                            std::iter::once(commit_type.id().clone()).chain(
                                commit_type
                                    .properties()
                                    .iter()
                                    .map(|property| property.id().clone()),
                            )
                        },
                    ))
                })
                .collect::<Vec<_>>()
        };
        let first = ids(&LibraryDraft::from_configuration(&configuration));
        assert_eq!(
            first,
            ids(&LibraryDraft::from_configuration(&configuration))
        );
        assert_eq!(
            first.len(),
            first
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        );
        Ok(())
    }

    #[test]
    fn reads_walk_from_a_node_to_its_ancestors() -> Result<(), Box<dyn std::error::Error>> {
        let draft = LibraryDraft::from_configuration(&team()?);
        let rollback = draft
            .find_path(&["team", "release", "rollback"])
            .ok_or("rollback")?;
        let Some(NodeRef::Property { property, .. }) = draft.node(&rollback) else {
            return Err("rollback is a property".into());
        };
        assert_eq!(property.policy(), Policy::Conditional);
        assert_eq!(property.condition(), "risky");
        assert_eq!(property.rationale(), "Risky releases need a way back.");
        let release = draft.parent(&rollback).ok_or("release")?;
        assert_eq!(draft.find_path(&["team", "release"]), Some(release.clone()));
        let team = draft.parent(&release).ok_or("team")?;
        assert_eq!(draft.parent(&team), None);
        assert_eq!(
            draft.node(&rollback).map(NodeRef::names),
            Some(vec!["team", "release", "rollback"])
        );
        assert_eq!(draft.version(&rollback), TaxonomyVersion::new(3).ok());
        assert_eq!(draft.find_path(&["team", "missing"]), None);
        Ok(())
    }
}
