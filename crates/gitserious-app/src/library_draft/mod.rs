//! The Library as plain drafts: every taxonomy, built-ins first, with its types and properties.
//!
//! A draft may hold text that does not build, such as a blank name; `problems` reports it and
//! `build` refuses it. `edit`, `create_taxonomy`, and `fork` are the only writers. Versions,
//! problems, changes, and whether the draft differs from what it was loaded from are derived on
//! each call.
//!
//! Building a draft from the same configuration twice yields equal node ids, so a view can keep
//! a node id across rebuilds. A node created in this session gets a local id that is never reused.

mod changes;
mod problems;

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::num::NonZeroU32;

use gitserious_core::{
    CommitTypeDefinition, PropertyDefinition, PropertyRequirement, SchemaVersion, Taxonomy,
    TaxonomyId, TaxonomyLineage, TaxonomyVersion, built_in_taxonomies,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::{ConfigurationError, GlobalConfiguration, TaxonomyOrigin};
pub use changes::{ChangeSet, Mark};
pub use problems::{Problem, ProblemKind, ProblemSite};

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

impl LoadedPath {
    /// The ids from the taxonomy down to the node.
    #[must_use]
    pub fn names(&self) -> &[String] {
        &self.0
    }
}

impl NodeId {
    fn loaded(path: &[&str]) -> Self {
        Self::Loaded(LoadedPath(path.iter().map(|&id| id.to_owned()).collect()))
    }

    const fn path(&self) -> Option<&LoadedPath> {
        match self {
            Self::Loaded(path) => Some(path),
            Self::Local(_) => None,
        }
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

    /// The next policy in display order, or the previous one, wrapping at either end.
    #[must_use]
    pub fn cycle(self, forward: bool) -> Self {
        let index = Self::ORDER
            .iter()
            .position(|policy| *policy == self)
            .unwrap_or(0);
        let count = Self::ORDER.len();
        Self::ORDER[if forward {
            (index + 1) % count
        } else {
            (index + count - 1) % count
        }]
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

    /// The most characters an edit may leave in this field, or none for a choice.
    #[must_use]
    pub const fn limit(self) -> Option<usize> {
        match self {
            Self::Name | Self::Condition => Some(NAME_LIMIT),
            Self::Description | Self::Rationale => Some(100),
            Self::Policy => None,
        }
    }
}

const NAME_LIMIT: usize = 80;
const FORK_SUFFIX: &str = "-fork";

const TAXONOMY_FIELDS: &[DraftField] = &[DraftField::Name, DraftField::Description];
const PERSISTED_TAXONOMY_FIELDS: &[DraftField] = &[DraftField::Description];
const TYPE_FIELDS: &[DraftField] = &[DraftField::Name, DraftField::Description];
const PROPERTY_FIELDS: &[DraftField] = &[
    DraftField::Name,
    DraftField::Description,
    DraftField::Policy,
];
const CONDITIONAL_PROPERTY_FIELDS: &[DraftField] = &[
    DraftField::Name,
    DraftField::Description,
    DraftField::Policy,
    DraftField::Condition,
    DraftField::Rationale,
];

/// One change to one node.
#[derive(Clone, Copy, Debug)]
pub enum DraftEdit<'a> {
    /// Replaces the field's whole text.
    Text(DraftField, &'a str),
    Policy(Policy),
    /// Appends a blank type to a taxonomy.
    AddType,
    /// Appends a blank property to a type.
    AddProperty,
    /// Removes the node and everything under it.
    Remove,
    /// Swaps a type or property with its next sibling, or its previous one; at either end it
    /// stays where it is.
    Move {
        down: bool,
    },
}

/// Why `edit` changed nothing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditRefusal {
    /// The node belongs to a built-in taxonomy.
    ReadOnly,
    /// The node is a taxonomy that the configuration already stores, whose name projects pin.
    NameLocked,
    /// The node has no such field, or the edit does not apply to its kind.
    NotApplicable,
    /// No node has this id.
    Missing,
    /// The text is longer than the field's limit and longer than before.
    TooLong,
}

impl Display for EditRefusal {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ReadOnly => "built-in taxonomies are read-only",
            Self::NameLocked => "a stored taxonomy keeps its name",
            Self::NotApplicable => "the node has no such field",
            Self::Missing => "no node has this id",
            Self::TooLong => "the text is longer than the field allows",
        })
    }
}

impl Error for EditRefusal {}

#[derive(Clone, Debug)]
pub struct TaxonomyNode {
    id: NodeId,
    baseline: Option<LoadedPath>,
    origin: TaxonomyOrigin,
    name: String,
    description: String,
    /// The version it was loaded at; a taxonomy created or forked here starts at 1.
    version: TaxonomyVersion,
    derived_from: Option<TaxonomyLineage>,
    types: Vec<TypeNode>,
}

impl TaxonomyNode {
    fn new(taxonomy: &Taxonomy, origin: TaxonomyOrigin) -> Self {
        let name = taxonomy.id().as_str();
        let id = NodeId::loaded(&[name]);
        Self {
            baseline: id.path().cloned(),
            id,
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

    /// The stored node this draft started from, or for a fork the node its source started
    /// from; none for a node created in this session.
    #[must_use]
    pub const fn baseline(&self) -> Option<&LoadedPath> {
        self.baseline.as_ref()
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
    pub const fn derived_from(&self) -> Option<&TaxonomyLineage> {
        self.derived_from.as_ref()
    }

    #[must_use]
    pub fn types(&self) -> &[TypeNode] {
        &self.types
    }

    /// Whether the configuration stores this custom taxonomy under its current name.
    fn persisted(&self) -> bool {
        self.origin == TaxonomyOrigin::Custom && matches!(self.id, NodeId::Loaded(_))
    }
}

#[derive(Clone, Debug)]
pub struct TypeNode {
    id: NodeId,
    baseline: Option<LoadedPath>,
    schema_version: SchemaVersion,
    name: String,
    description: String,
    properties: Vec<PropertyNode>,
}

impl TypeNode {
    fn new(taxonomy: &str, commit_type: &CommitTypeDefinition) -> Self {
        let name = commit_type.id().as_str();
        let id = NodeId::loaded(&[taxonomy, name]);
        Self {
            baseline: id.path().cloned(),
            id,
            schema_version: commit_type.schema_version(),
            name: name.to_owned(),
            description: commit_type.description().to_owned(),
            properties: commit_type
                .properties()
                .iter()
                .map(|property| PropertyNode::new(taxonomy, name, property))
                .collect(),
        }
    }

    /// The stored node this draft started from, or for a fork the node its source started
    /// from; none for a node created in this session.
    #[must_use]
    pub const fn baseline(&self) -> Option<&LoadedPath> {
        self.baseline.as_ref()
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
    pub fn properties(&self) -> &[PropertyNode] {
        &self.properties
    }
}

/// A property. `condition` and `rationale` stay while the policy is not conditional, so
/// cycling back to conditional restores them; nothing else reads them until then.
#[derive(Clone, Debug)]
pub struct PropertyNode {
    id: NodeId,
    baseline: Option<LoadedPath>,
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
        let id = NodeId::loaded(&[taxonomy, commit_type, name]);
        Self {
            baseline: id.path().cloned(),
            id,
            name: name.to_owned(),
            description: property.description().to_owned(),
            policy,
            condition,
            rationale,
        }
    }

    /// The stored node this draft started from, or for a fork the node its source started
    /// from; none for a node created in this session.
    #[must_use]
    pub const fn baseline(&self) -> Option<&LoadedPath> {
        self.baseline.as_ref()
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
    pub const fn policy(&self) -> Policy {
        self.policy
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

/// Where a node sits: indexes of its taxonomy, type, and property.
#[derive(Clone, Copy)]
enum Address {
    Taxonomy(usize),
    Type(usize, usize),
    Property(usize, usize, usize),
}

/// Every taxonomy of one Library: built-ins, then custom taxonomies in storage order, with the
/// selection of default and available taxonomies.
#[derive(Clone, Debug)]
pub struct LibraryDraft {
    persisted: GlobalConfiguration,
    taxonomies: Vec<TaxonomyNode>,
    default: TaxonomyId,
    available: Vec<TaxonomyId>,
    next_local: u32,
}

impl LibraryDraft {
    #[must_use]
    pub fn from_configuration(configuration: GlobalConfiguration) -> Self {
        let built_in = built_in_taxonomies()
            .iter()
            .map(|taxonomy| TaxonomyNode::new(taxonomy, TaxonomyOrigin::BuiltIn));
        let custom = configuration
            .custom_taxonomies()
            .iter()
            .map(|taxonomy| TaxonomyNode::new(taxonomy, TaxonomyOrigin::Custom));
        let taxonomies = built_in.chain(custom).collect();
        Self {
            default: configuration.default_taxonomy().clone(),
            available: configuration.available_taxonomies().to_vec(),
            persisted: configuration,
            taxonomies,
            next_local: 0,
        }
    }

    /// The configuration this draft was loaded from.
    #[must_use]
    pub const fn persisted(&self) -> &GlobalConfiguration {
        &self.persisted
    }

    #[must_use]
    pub fn taxonomies(&self) -> &[TaxonomyNode] {
        &self.taxonomies
    }

    #[must_use]
    pub const fn default_taxonomy(&self) -> &TaxonomyId {
        &self.default
    }

    #[must_use]
    pub fn available_taxonomies(&self) -> &[TaxonomyId] {
        &self.available
    }

    #[must_use]
    pub fn node(&self, id: &NodeId) -> Option<NodeRef<'_>> {
        Some(match self.address(id)? {
            Address::Taxonomy(t) => NodeRef::Taxonomy(&self.taxonomies[t]),
            Address::Type(t, c) => NodeRef::Type {
                taxonomy: &self.taxonomies[t],
                commit_type: &self.taxonomies[t].types[c],
            },
            Address::Property(t, c, p) => NodeRef::Property {
                taxonomy: &self.taxonomies[t],
                commit_type: &self.taxonomies[t].types[c],
                property: &self.taxonomies[t].types[c].properties[p],
            },
        })
    }

    fn address(&self, id: &NodeId) -> Option<Address> {
        for (t, taxonomy) in self.taxonomies.iter().enumerate() {
            if &taxonomy.id == id {
                return Some(Address::Taxonomy(t));
            }
            for (c, commit_type) in taxonomy.types.iter().enumerate() {
                if &commit_type.id == id {
                    return Some(Address::Type(t, c));
                }
                if let Some(p) = commit_type.properties.iter().position(|p| &p.id == id) {
                    return Some(Address::Property(t, c, p));
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
    pub fn find_path<S: AsRef<str>>(&self, names: &[S]) -> Option<NodeId> {
        let (first, rest) = names.split_first()?;
        let taxonomy = self
            .taxonomies
            .iter()
            .find(|node| node.name == first.as_ref())?;
        let Some((type_name, rest)) = rest.split_first() else {
            return Some(taxonomy.id.clone());
        };
        let commit_type = taxonomy
            .types
            .iter()
            .find(|node| node.name == type_name.as_ref())?;
        match rest {
            [] => Some(commit_type.id.clone()),
            [property] => commit_type
                .properties
                .iter()
                .find(|node| node.name == property.as_ref())
                .map(|node| node.id.clone()),
            _ => None,
        }
    }

    /// The fields an edit may change, in display order: none on a built-in, no Name on a
    /// taxonomy the configuration stores, and Condition and Rationale only while conditional.
    #[must_use]
    pub fn fields(&self, id: &NodeId) -> &'static [DraftField] {
        match self.node(id) {
            None => &[],
            Some(node) if node.taxonomy().origin == TaxonomyOrigin::BuiltIn => &[],
            Some(NodeRef::Taxonomy(taxonomy)) if taxonomy.persisted() => PERSISTED_TAXONOMY_FIELDS,
            Some(NodeRef::Taxonomy(_)) => TAXONOMY_FIELDS,
            Some(NodeRef::Type { .. }) => TYPE_FIELDS,
            Some(NodeRef::Property { property, .. }) if property.policy == Policy::Conditional => {
                CONDITIONAL_PROPERTY_FIELDS
            }
            Some(NodeRef::Property { .. }) => PROPERTY_FIELDS,
        }
    }

    /// A text field's current text, whether or not the node shows it now.
    #[must_use]
    pub fn text(&self, id: &NodeId, field: DraftField) -> Option<&str> {
        let node = self.node(id)?;
        let (name, description) = match node {
            NodeRef::Taxonomy(taxonomy) => (&taxonomy.name, &taxonomy.description),
            NodeRef::Type { commit_type, .. } => (&commit_type.name, &commit_type.description),
            NodeRef::Property { property, .. } => (&property.name, &property.description),
        };
        match (field, node) {
            (DraftField::Name, _) => Some(name),
            (DraftField::Description, _) => Some(description),
            (DraftField::Condition, NodeRef::Property { property, .. }) => {
                Some(&property.condition)
            }
            (DraftField::Rationale, NodeRef::Property { property, .. }) => {
                Some(&property.rationale)
            }
            _ => None,
        }
    }

    /// The version apply would write for the taxonomy that owns `id`: the stored version, one
    /// past it once the content differs, and 1 for a taxonomy created or forked here.
    #[must_use]
    pub fn version(&self, id: &NodeId) -> Option<TaxonomyVersion> {
        let taxonomy = self.node(id)?.taxonomy();
        if !taxonomy.persisted() {
            return Some(taxonomy.version);
        }
        let stored = self
            .persisted
            .custom_taxonomies()
            .iter()
            .find(|stored| stored.id().as_str() == taxonomy.name)?;
        let unchanged = problems::content(self, taxonomy).is_ok_and(|(description, types)| {
            &description == stored.description() && types == stored.commit_types()
        });
        if unchanged {
            Some(stored.version())
        } else {
            TaxonomyVersion::new(stored.version().get().saturating_add(1)).ok()
        }
    }

    /// Whether apply would write anything: the draft does not build to what it was loaded from.
    #[must_use]
    pub fn is_changed(&self) -> bool {
        !self.build().is_ok_and(|built| built == self.persisted)
    }

    /// Every reason the draft does not build, in outline order.
    #[must_use]
    pub fn problems(&self) -> Vec<Problem> {
        self.build().err().unwrap_or_default()
    }

    /// The Library apply would write. The error lists every problem and is never empty.
    pub fn build(&self) -> Result<GlobalConfiguration, Vec<Problem>> {
        problems::build(self)
    }

    /// Why removing `id` would break the selection: for a taxonomy that is the default or
    /// available, and that no other taxonomy of the same name stands in for, the error apply
    /// would report once it is gone.
    #[must_use]
    pub fn removal_blocker(&self, id: &NodeId) -> Option<ConfigurationError> {
        let NodeRef::Taxonomy(taxonomy) = self.node(id)? else {
            return None;
        };
        let selected = std::iter::once(&self.default)
            .chain(&self.available)
            .find(|selected| selected.as_str() == taxonomy.name)?;
        let replaced = self
            .taxonomies
            .iter()
            .any(|other| other.id != taxonomy.id && other.name == taxonomy.name);
        (!replaced).then(|| ConfigurationError::UnknownTaxonomy(selected.clone()))
    }

    /// Replaces the selection of default and available taxonomies.
    pub fn select(&mut self, default: TaxonomyId, available: Vec<TaxonomyId>) {
        self.default = default;
        self.available = available;
    }

    /// Applies one edit and returns the node a cursor should land on: the edited node, the new
    /// child, or after `Remove` the next sibling, else the previous one, else the parent.
    pub fn edit(&mut self, id: &NodeId, edit: DraftEdit<'_>) -> Result<NodeId, EditRefusal> {
        let node = self.node(id).ok_or(EditRefusal::Missing)?;
        if node.taxonomy().origin == TaxonomyOrigin::BuiltIn {
            return Err(EditRefusal::ReadOnly);
        }
        let address = self.address(id).ok_or(EditRefusal::Missing)?;
        match edit {
            DraftEdit::Text(field, text) => {
                if field == DraftField::Name
                    && matches!(node, NodeRef::Taxonomy(t) if t.persisted())
                {
                    return Err(EditRefusal::NameLocked);
                }
                let (Some(limit), Some(current)) = (field.limit(), self.text(id, field)) else {
                    return Err(EditRefusal::NotApplicable);
                };
                let length = text.graphemes(true).count();
                if length > limit && length > current.graphemes(true).count() {
                    return Err(EditRefusal::TooLong);
                }
                text.clone_into(self.text_mut(address, field));
                Ok(id.clone())
            }
            DraftEdit::Policy(policy) => match address {
                Address::Property(t, c, p) => {
                    self.taxonomies[t].types[c].properties[p].policy = policy;
                    Ok(id.clone())
                }
                _ => Err(EditRefusal::NotApplicable),
            },
            DraftEdit::AddType => match address {
                Address::Taxonomy(t) => {
                    let id = self.mint();
                    self.taxonomies[t].types.push(TypeNode {
                        id: id.clone(),
                        baseline: None,
                        schema_version: SchemaVersion::new(1)
                            .unwrap_or_else(|_| unreachable!("schema version 1 is valid")),
                        name: String::new(),
                        description: String::new(),
                        properties: Vec::new(),
                    });
                    Ok(id)
                }
                _ => Err(EditRefusal::NotApplicable),
            },
            DraftEdit::AddProperty => match address {
                Address::Type(t, c) => {
                    let id = self.mint();
                    self.taxonomies[t].types[c].properties.push(PropertyNode {
                        id: id.clone(),
                        baseline: None,
                        name: String::new(),
                        description: String::new(),
                        policy: Policy::Required,
                        condition: String::new(),
                        rationale: String::new(),
                    });
                    Ok(id)
                }
                _ => Err(EditRefusal::NotApplicable),
            },
            DraftEdit::Remove => Ok(match address {
                Address::Taxonomy(t) => {
                    self.taxonomies.remove(t);
                    landing(&self.taxonomies, t, TaxonomyNode::id, None)
                }
                Address::Type(t, c) => {
                    let taxonomy = &mut self.taxonomies[t];
                    taxonomy.types.remove(c);
                    landing(&taxonomy.types, c, TypeNode::id, Some(&taxonomy.id))
                }
                Address::Property(t, c, p) => {
                    let commit_type = &mut self.taxonomies[t].types[c];
                    commit_type.properties.remove(p);
                    landing(
                        &commit_type.properties,
                        p,
                        PropertyNode::id,
                        Some(&commit_type.id),
                    )
                }
            }),
            DraftEdit::Move { down } => {
                match address {
                    Address::Taxonomy(_) => return Err(EditRefusal::NotApplicable),
                    Address::Type(t, c) => shift(&mut self.taxonomies[t].types, c, down),
                    Address::Property(t, c, p) => {
                        shift(&mut self.taxonomies[t].types[c].properties, p, down);
                    }
                }
                Ok(id.clone())
            }
        }
    }

    fn text_mut(&mut self, address: Address, field: DraftField) -> &mut String {
        match (address, field) {
            (Address::Taxonomy(t), DraftField::Name) => &mut self.taxonomies[t].name,
            (Address::Taxonomy(t), _) => &mut self.taxonomies[t].description,
            (Address::Type(t, c), DraftField::Name) => &mut self.taxonomies[t].types[c].name,
            (Address::Type(t, c), _) => &mut self.taxonomies[t].types[c].description,
            (Address::Property(t, c, p), field) => {
                let property = &mut self.taxonomies[t].types[c].properties[p];
                match field {
                    DraftField::Name => &mut property.name,
                    DraftField::Condition => &mut property.condition,
                    DraftField::Rationale => &mut property.rationale,
                    DraftField::Description | DraftField::Policy => &mut property.description,
                }
            }
        }
    }

    fn mint(&mut self) -> NodeId {
        self.next_local += 1;
        NodeId::Local(
            NonZeroU32::new(self.next_local)
                .unwrap_or_else(|| unreachable!("local ids count up from 1")),
        )
    }

    /// Appends a blank custom taxonomy and returns it.
    pub fn create_taxonomy(&mut self) -> NodeId {
        let id = self.mint();
        self.taxonomies.push(TaxonomyNode {
            id: id.clone(),
            baseline: None,
            origin: TaxonomyOrigin::Custom,
            name: String::new(),
            description: String::new(),
            version: TaxonomyVersion::V1,
            derived_from: None,
            types: Vec::new(),
        });
        id
    }

    /// Appends a copy of the taxonomy that owns `id`, named `<source>-fork` with the source part
    /// cut to fit the Name limit, with lineage to the source's version. Each copied node keeps
    /// the baseline of the node it copies.
    pub fn fork(&mut self, id: &NodeId) -> Result<NodeId, EditRefusal> {
        let source = self.node(id).ok_or(EditRefusal::Missing)?.taxonomy();
        let lineage = TaxonomyId::from_label(&source.name)
            .ok()
            .zip(self.version(&source.id))
            .map(|(id, version)| TaxonomyLineage::new(id, version))
            .ok_or(EditRefusal::NotApplicable)?;
        let mut fork = source.clone();
        fork.origin = TaxonomyOrigin::Custom;
        let room = NAME_LIMIT - FORK_SUFFIX.len();
        fork.name = source.name.graphemes(true).take(room).collect::<String>() + FORK_SUFFIX;
        fork.version = TaxonomyVersion::V1;
        fork.derived_from = Some(lineage);
        fork.id = self.mint();
        for commit_type in &mut fork.types {
            commit_type.id = self.mint();
            for property in &mut commit_type.properties {
                property.id = self.mint();
            }
        }
        let id = fork.id.clone();
        self.taxonomies.push(fork);
        Ok(id)
    }
}

/// Swaps `siblings[index]` with the sibling after it, or before it, when there is one.
fn shift<T>(siblings: &mut [T], index: usize, down: bool) {
    let other = if down {
        index.checked_add(1)
    } else {
        index.checked_sub(1)
    };
    if let Some(other) = other.filter(|&other| other < siblings.len()) {
        siblings.swap(index, other);
    }
}

/// After removing index `removed` from `siblings`: the next sibling, else the previous one,
/// else `parent`.
fn landing<T>(
    siblings: &[T],
    removed: usize,
    id: fn(&T) -> &NodeId,
    parent: Option<&NodeId>,
) -> NodeId {
    siblings
        .get(removed)
        .or_else(|| removed.checked_sub(1).and_then(|index| siblings.get(index)))
        .map(id)
        .or(parent)
        .cloned()
        .unwrap_or_else(|| unreachable!("the built-in taxonomies always remain"))
}

#[cfg(test)]
mod tests;
