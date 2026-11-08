//! The Library as plain drafts: every taxonomy, built-ins first, with its types and properties.
//!
//! Unsaved fields may be incomplete; `problems` reports them and `build` validates them.
//! Successful saves acknowledge normalized values and baselines while retaining node ids.
//! Copy provenance stays in memory, and reopening the saved configuration drops it.

mod problems;

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::num::NonZeroU32;

use gitserious_core::{
    CommitTypeDefinition, PropertyDefinition, PropertyKey, PropertyRequirement, SchemaVersion,
    Taxonomy, TaxonomyId, TaxonomyLineage, TaxonomyVersion, built_in_taxonomies,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    ConfigurationError, GlobalConfiguration, LibraryTaxonomy, ProjectConfiguration, TaxonomyOrigin,
};
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
const COPY_SUFFIX: &str = "-copy";

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
    /// Copies a property into each of these types as a new property of its own. Every target
    /// must be another type of the property's taxonomy without the property's key, or nothing is
    /// copied.
    CopyTo(&'a [NodeId]),
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
    /// The copy target at this index already has the property's key.
    Taken { target: usize },
}

impl Display for EditRefusal {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ReadOnly => "built-in taxonomies are read-only",
            Self::NameLocked => "a stored taxonomy keeps its name",
            Self::NotApplicable => "the node has no such field",
            Self::Missing => "no node has this id",
            Self::TooLong => "the text is longer than the field allows",
            Self::Taken { .. } => "a target type already has this key",
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
    version: Option<TaxonomyVersion>,
    derived_from: Option<TaxonomyLineage>,
    types: Vec<TypeNode>,
    name_locked: bool,
    saved_name: Option<TaxonomyId>,
    created_on: Option<crate::TaxonomyDate>,
    modified_on: Option<crate::TaxonomyDate>,
}

impl TaxonomyNode {
    fn new(taxonomy: &LibraryTaxonomy, origin: TaxonomyOrigin) -> Self {
        let name = taxonomy.id().as_str();
        let id = NodeId::loaded(&[name]);
        Self {
            baseline: id.path().cloned(),
            id,
            origin,
            name: name.to_owned(),
            description: taxonomy.description().to_string(),
            version: taxonomy.version(),
            name_locked: taxonomy.name_locked(),
            saved_name: Some(taxonomy.id().clone()),
            created_on: taxonomy.created_on(),
            modified_on: taxonomy.modified_on(),
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
}

#[derive(Clone, Debug)]
pub struct TypeNode {
    id: NodeId,
    baseline: Option<LoadedPath>,
    schema_version: Option<SchemaVersion>,
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
    pub fn description(&self) -> &str {
        &self.description
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
    copied_from: Option<NodeId>,
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
            copied_from: None,
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
    pub fn description(&self) -> &str {
        &self.description
    }

    #[must_use]
    pub const fn policy(&self) -> Policy {
        self.policy
    }

    /// The type this property was copied from in this session.
    #[must_use]
    pub const fn copied_from(&self) -> Option<&NodeId> {
        self.copied_from.as_ref()
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

/// What a draft was loaded from and saves back to.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Owner {
    /// The user's Library, with the built-ins listed first and read-only.
    Library(GlobalConfiguration),
    /// A project's own taxonomies. None is read-only and any identity, built-in or not, is
    /// free; every taxonomy is available, so `available` follows the outline.
    Project {
        configuration: ProjectConfiguration,
        records: Vec<LibraryTaxonomy>,
    },
}

/// Every taxonomy of one Library or one project, with the selection of default and available
/// taxonomies. A Library lists built-ins, then custom taxonomies in storage order.
#[derive(Clone, Debug)]
pub struct LibraryDraft {
    owner: Owner,
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
            .map(|taxonomy| TaxonomyNode::new(&taxonomy.clone().into(), TaxonomyOrigin::BuiltIn));
        let custom = configuration
            .custom_taxonomies()
            .iter()
            .map(|taxonomy| TaxonomyNode::new(taxonomy, TaxonomyOrigin::Custom));
        let taxonomies = built_in.chain(custom).collect();
        Self {
            default: configuration.default_taxonomy().clone(),
            available: configuration.available_taxonomies().to_vec(),
            owner: Owner::Library(configuration),
            taxonomies,
            next_local: 0,
        }
    }

    /// Drafts a project's own taxonomies, every one editable.
    #[must_use]
    pub fn from_project(configuration: ProjectConfiguration) -> Self {
        let records = configuration
            .taxonomies()
            .iter()
            .cloned()
            .map(LibraryTaxonomy::from)
            .collect::<Vec<_>>();
        Self {
            default: configuration.default_taxonomy().clone(),
            available: records.iter().map(|record| record.id().clone()).collect(),
            taxonomies: records
                .iter()
                .map(|record| TaxonomyNode::new(record, TaxonomyOrigin::Custom))
                .collect(),
            owner: Owner::Project {
                configuration,
                records,
            },
            next_local: 0,
        }
    }

    /// The Library this draft was loaded from; none for a project draft.
    #[must_use]
    pub const fn persisted(&self) -> Option<&GlobalConfiguration> {
        match &self.owner {
            Owner::Library(configuration) => Some(configuration),
            Owner::Project { .. } => None,
        }
    }

    /// The project configuration this draft was loaded from; none for a Library draft.
    #[must_use]
    pub const fn persisted_project(&self) -> Option<&ProjectConfiguration> {
        match &self.owner {
            Owner::Library(_) => None,
            Owner::Project { configuration, .. } => Some(configuration),
        }
    }

    /// Whether this draft holds a project's taxonomies rather than the Library.
    #[must_use]
    pub const fn is_project(&self) -> bool {
        matches!(self.owner, Owner::Project { .. })
    }

    /// The taxonomies as stored when the draft was loaded or last saved.
    fn records(&self) -> &[LibraryTaxonomy] {
        match &self.owner {
            Owner::Library(configuration) => configuration.custom_taxonomies(),
            Owner::Project { records, .. } => records,
        }
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

    /// The types `CopyTo` may copy the property `id` into: the other types of its custom
    /// taxonomy, in order. Empty for anything else.
    #[must_use]
    pub fn copy_targets(&self, id: &NodeId) -> Vec<&TypeNode> {
        match self.node(id) {
            Some(NodeRef::Property {
                taxonomy,
                commit_type,
                ..
            }) if taxonomy.origin == TaxonomyOrigin::Custom => taxonomy
                .types
                .iter()
                .filter(|other| other.id != commit_type.id)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The fields an edit may change, in display order: none on a built-in, no Name on a
    /// taxonomy the configuration stores, and Condition and Rationale only while conditional.
    #[must_use]
    pub fn fields(&self, id: &NodeId) -> &'static [DraftField] {
        match self.node(id) {
            None => &[],
            Some(node) if node.taxonomy().origin == TaxonomyOrigin::BuiltIn => &[],
            Some(NodeRef::Taxonomy(taxonomy)) if taxonomy.name_locked => PERSISTED_TAXONOMY_FIELDS,
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
        let (name, description): (&str, &str) = match node {
            NodeRef::Taxonomy(taxonomy) => (&taxonomy.name, &taxonomy.description),
            NodeRef::Type { commit_type, .. } => (commit_type.name(), commit_type.description()),
            NodeRef::Property { property, .. } => (property.name(), property.description()),
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

    #[must_use]
    pub fn version(&self, id: &NodeId) -> Option<TaxonomyVersion> {
        self.node(id)?.taxonomy().version
    }

    /// Whether saving would write anything: the draft does not build to what it was loaded from.
    #[must_use]
    pub fn is_changed(&self) -> bool {
        !problems::build_owner(self).is_ok_and(|built| built == self.owner)
    }

    /// Every reason the draft does not build, in outline order.
    #[must_use]
    pub fn problems(&self) -> Vec<Problem> {
        self.build().err().unwrap_or_default()
    }

    /// The validated Library to persist. The error lists every problem and is never empty; a
    /// project draft builds through [`Self::build_project`] instead.
    pub fn build(&self) -> Result<GlobalConfiguration, Vec<Problem>> {
        match problems::build_owner(self)? {
            Owner::Library(configuration) => Ok(configuration),
            Owner::Project { .. } => Err(vec![problems::wrong_owner()]),
        }
    }

    /// The validated project configuration to apply. The error lists every problem and is never
    /// empty; a Library draft builds through [`Self::build`] instead.
    pub fn build_project(&self) -> Result<ProjectConfiguration, Vec<Problem>> {
        match problems::build_owner(self)? {
            Owner::Project { configuration, .. } => Ok(configuration),
            Owner::Library(_) => Err(vec![problems::wrong_owner()]),
        }
    }

    /// A validated snapshot acknowledged in place after persistence, retaining every node id.
    pub fn saved_on(&self, date: crate::TaxonomyDate) -> Result<Self, Vec<Problem>> {
        let owner = problems::build_owner(self)?;
        let mut saved = self.clone();
        if let Owner::Library(configuration) = &owner {
            for (node, record) in saved
                .taxonomies
                .iter_mut()
                .filter(|node| node.origin == TaxonomyOrigin::Custom)
                .zip(configuration.custom_taxonomies())
            {
                let previous = self.saved_taxonomy(&node.id);
                if previous != Some(record) {
                    node.modified_on = Some(date);
                    if previous.is_none() {
                        node.created_on = Some(date);
                    }
                }
            }
        }
        saved.saved()
    }

    /// A clock-free canonical snapshot used when an edit changes no persisted content.
    pub fn saved(&self) -> Result<Self, Vec<Problem>> {
        let mut saved = self.clone();
        saved.owner = problems::build_owner(self)?;
        let records = saved.records().to_vec();
        for (node, record) in saved
            .taxonomies
            .iter_mut()
            .filter(|node| node.origin == TaxonomyOrigin::Custom)
            .zip(&records)
        {
            let mut canonical = TaxonomyNode::new(record, TaxonomyOrigin::Custom);
            canonical.id.clone_from(&node.id);
            for (child, previous) in canonical.types.iter_mut().zip(&node.types) {
                child.id.clone_from(&previous.id);
                for (property, previous) in child.properties.iter_mut().zip(&previous.properties) {
                    property.id.clone_from(&previous.id);
                    property.copied_from.clone_from(&previous.copied_from);
                    if property.policy != Policy::Conditional {
                        property.condition.clone_from(&previous.condition);
                        property.rationale.clone_from(&previous.rationale);
                    }
                }
            }
            *node = canonical;
        }
        Ok(saved)
    }

    /// The saved record of the owning custom taxonomy, independent of its current field edits.
    #[must_use]
    pub fn saved_taxonomy(&self, id: &NodeId) -> Option<&LibraryTaxonomy> {
        let name = self.node(id)?.taxonomy().saved_name.as_ref()?;
        self.records().iter().find(|record| record.id() == name)
    }

    /// Whether a new node has no text or children and can be cancelled without saving.
    /// A policy selection alone does not keep a blank new property in creation.
    #[must_use]
    pub fn blank_creation(&self, id: &NodeId) -> bool {
        match self.node(id) {
            Some(NodeRef::Taxonomy(node)) => {
                node.saved_name.is_none()
                    && node.baseline.is_none()
                    && node.name.is_empty()
                    && node.description.is_empty()
                    && node.types.is_empty()
            }
            Some(NodeRef::Type {
                commit_type: node, ..
            }) => {
                node.baseline.is_none()
                    && node.name.is_empty()
                    && node.description.is_empty()
                    && node.properties.is_empty()
            }
            Some(NodeRef::Property { property: node, .. }) => {
                node.baseline.is_none()
                    && node.name.is_empty()
                    && node.description.is_empty()
                    && node.condition.is_empty()
                    && node.rationale.is_empty()
            }
            None => false,
        }
    }

    /// Field errors for an edited node, including collisions with siblings later in the outline.
    #[must_use]
    pub fn field_problems(&self, id: &NodeId) -> Vec<Problem> {
        use gitserious_core::CommitTypeId;
        let name = match self.node(id) {
            Some(NodeRef::Taxonomy(node)) => {
                TaxonomyId::from_label(&node.name).map(|id| id.to_string())
            }
            Some(NodeRef::Type { commit_type, .. }) => {
                CommitTypeId::from_label(&commit_type.name).map(|id| id.to_string())
            }
            Some(NodeRef::Property { property, .. }) => {
                PropertyKey::from_label(&property.name).map(|id| id.to_string())
            }
            None => return Vec::new(),
        }
        .ok();
        let mut errors = Vec::new();
        for mut problem in self.problems() {
            if problem.node() == Some(id) {
                errors.push(problem);
            } else if let ProblemKind::Taken(taken) = &problem.kind
                && name.as_ref() == Some(taken)
                && problem.node().and_then(|node| self.parent(node)) == self.parent(id)
            {
                problem.site = ProblemSite::Field(id.clone(), DraftField::Name);
                errors.insert(0, problem);
            }
        }
        errors
    }

    /// Why removing `id` would break the selection: for a taxonomy that is the default or
    /// available, and that no other taxonomy of the same name stands in for, the error saving
    /// would report once it is gone.
    #[must_use]
    pub fn removal_blocker(&self, id: &NodeId) -> Option<ConfigurationError> {
        let NodeRef::Taxonomy(taxonomy) = self.node(id)? else {
            return None;
        };
        let available = if self.is_project() {
            &[][..]
        } else {
            &self.available[..]
        };
        let selected = std::iter::once(&self.default)
            .chain(available)
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

    /// Keeps only the project taxonomies named in `order`, in that order, and makes `default`
    /// the project default. A name missing from the draft is restored from the applied file.
    pub fn select_project(
        &mut self,
        default: TaxonomyId,
        order: &[TaxonomyId],
    ) -> Result<(), String> {
        if !self.is_project() {
            return Err("only a project draft selects its own taxonomies".into());
        }
        let mut kept = Vec::with_capacity(order.len());
        for id in order {
            let node = self
                .taxonomies
                .iter()
                .find(|node| TaxonomyId::from_label(&node.name).as_ref() == Ok(id))
                .cloned()
                .or_else(|| {
                    self.records()
                        .iter()
                        .find(|record| record.id() == id)
                        .map(|record| TaxonomyNode::new(record, TaxonomyOrigin::Custom))
                })
                .ok_or_else(|| format!("taxonomy \"{id}\" is not part of this project"))?;
            kept.push(node);
        }
        self.taxonomies = kept;
        self.available = order.to_vec();
        self.default = default;
        Ok(())
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
            DraftEdit::Text(field, text) => self.edit_text(id, address, field, text),
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
                        schema_version: None,
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
                        copied_from: None,
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
            DraftEdit::CopyTo(targets) => self.copy(address, targets).map(|()| id.clone()),
        }
    }

    fn copy(&mut self, address: Address, targets: &[NodeId]) -> Result<(), EditRefusal> {
        let Address::Property(t, c, p) = address else {
            return Err(EditRefusal::NotApplicable);
        };
        let key = |name: &str| {
            PropertyKey::from_label(name).map_or_else(|_| name.to_owned(), |key| key.to_string())
        };
        let source = self.taxonomies[t].types[c].properties[p].clone();
        let mut into = Vec::with_capacity(targets.len());
        for (index, target) in targets.iter().enumerate() {
            let Some(Address::Type(target_t, target_c)) = self.address(target) else {
                return Err(EditRefusal::NotApplicable);
            };
            if target_t != t {
                return Err(EditRefusal::NotApplicable);
            }
            let taken = into.contains(&target_c)
                || self.taxonomies[t].types[target_c]
                    .properties
                    .iter()
                    .any(|property| key(&property.name) == key(&source.name));
            if taken {
                return Err(EditRefusal::Taken { target: index });
            }
            into.push(target_c);
        }
        let copied_from = Some(self.taxonomies[t].types[c].id.clone());
        for target_c in into {
            let copy = PropertyNode {
                id: self.mint(),
                baseline: None,
                copied_from: copied_from.clone(),
                ..source.clone()
            };
            self.taxonomies[t].types[target_c].properties.push(copy);
        }
        Ok(())
    }

    fn edit_text(
        &mut self,
        id: &NodeId,
        address: Address,
        field: DraftField,
        text: &str,
    ) -> Result<NodeId, EditRefusal> {
        if field == DraftField::Name
            && matches!(address, Address::Taxonomy(t) if self.taxonomies[t].name_locked)
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
        if current == text {
            return Ok(id.clone());
        }
        text.clone_into(self.text_mut(address, field));
        Ok(id.clone())
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
            version: None,
            name_locked: false,
            saved_name: None,
            created_on: None,
            modified_on: None,
            derived_from: None,
            types: Vec::new(),
        });
        id
    }

    /// Appends a copy of the taxonomy that owns `id`, named `<source>-fork` with the source part
    /// cut to fit the Name limit, with lineage to the source. Each copied node keeps
    /// the baseline of the node it copies, but no property names a type it was copied from.
    pub fn fork(&mut self, id: &NodeId) -> Result<NodeId, EditRefusal> {
        let source = self.node(id).ok_or(EditRefusal::Missing)?.taxonomy();
        let lineage = TaxonomyId::from_label(&source.name)
            .ok()
            .map(|id| TaxonomyLineage::new(id, self.version(&source.id)))
            .ok_or(EditRefusal::NotApplicable)?;
        let mut fork = source.clone();
        fork.origin = TaxonomyOrigin::Custom;
        fork.name_locked = false;
        fork.saved_name = None;
        fork.created_on = None;
        fork.modified_on = None;
        let room = NAME_LIMIT - FORK_SUFFIX.len();
        fork.name = source.name.graphemes(true).take(room).collect::<String>() + FORK_SUFFIX;
        fork.version = None;
        fork.derived_from = Some(lineage);
        fork.id = self.mint();
        for commit_type in &mut fork.types {
            commit_type.schema_version = None;
            commit_type.id = self.mint();
            for property in &mut commit_type.properties {
                property.id = self.mint();
                property.copied_from = None;
            }
        }
        let id = fork.id.clone();
        self.taxonomies.push(fork);
        Ok(id)
    }
}

/// How a taxonomy copied in from another owner meets one already here with its identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Collision {
    /// Nothing here has the identity, so the copy can go in as is.
    Absent,
    /// A taxonomy here has the identity and the same description and types.
    Identical,
    /// A taxonomy here has the identity and different content; only an explicit replace or a
    /// copy under a new identity may proceed.
    Differs,
    /// A read-only built-in has the identity; only a copy under a new identity may proceed.
    Reserved,
}

/// What to do with a taxonomy copied in from another owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CopyIn {
    /// Add it under its own identity, which nothing here may already use.
    New,
    /// Replace the content of the taxonomy here that has its identity, keeping that
    /// taxonomy's place and creation history.
    Replace,
    /// Add it under `<identity>-copy`, with lineage to the source, ready to rename.
    Renamed,
}

impl LibraryDraft {
    /// How `taxonomy` would meet the taxonomy here that has its identity.
    #[must_use]
    pub fn collision(&self, taxonomy: &Taxonomy) -> Collision {
        let Some(node) = self.named(taxonomy.id()) else {
            return Collision::Absent;
        };
        let same = problems::content(self, node).is_ok_and(|(description, types)| {
            &description == taxonomy.description() && types == taxonomy.commit_types()
        });
        match (same, node.origin) {
            (true, _) => Collision::Identical,
            (false, TaxonomyOrigin::BuiltIn) => Collision::Reserved,
            (false, _) => Collision::Differs,
        }
    }

    /// Copies `taxonomy` in from another owner and returns the node it now occupies. The copy
    /// is independent: nothing links it back to its source.
    pub fn copy_in(&mut self, taxonomy: &Taxonomy, how: CopyIn) -> Result<NodeId, String> {
        let collision = self.collision(taxonomy);
        match (how, collision) {
            (CopyIn::New, Collision::Absent) => {
                let node = self.imported(taxonomy, taxonomy.id().to_string(), true, None);
                let id = node.id.clone();
                self.taxonomies.push(node);
                self.adopt(taxonomy.id());
                Ok(id)
            }
            (CopyIn::New, Collision::Identical) => Ok(self
                .named(taxonomy.id())
                .map(|node| node.id.clone())
                .ok_or("the identical taxonomy is missing")?),
            (CopyIn::Replace, Collision::Differs) => {
                let index = self
                    .taxonomies
                    .iter()
                    .position(|node| {
                        TaxonomyId::from_label(&node.name).as_ref() == Ok(taxonomy.id())
                    })
                    .ok_or("the taxonomy to replace is missing")?;
                let mut replacement =
                    self.imported(taxonomy, taxonomy.id().to_string(), true, None);
                let current = &self.taxonomies[index];
                replacement.id.clone_from(&current.id);
                replacement.baseline.clone_from(&current.baseline);
                replacement.saved_name.clone_from(&current.saved_name);
                replacement.created_on = current.created_on;
                replacement.modified_on = current.modified_on;
                replacement.derived_from.clone_from(&current.derived_from);
                self.taxonomies[index] = replacement;
                Ok(self.taxonomies[index].id.clone())
            }
            (CopyIn::Renamed, _) => {
                let room = NAME_LIMIT - COPY_SUFFIX.len();
                let name = taxonomy
                    .id()
                    .as_str()
                    .graphemes(true)
                    .take(room)
                    .collect::<String>()
                    + COPY_SUFFIX;
                let lineage = TaxonomyLineage::new(taxonomy.id().clone(), taxonomy.version());
                let node = self.imported(taxonomy, name, false, Some(lineage));
                let id = node.id.clone();
                if let Ok(name) = TaxonomyId::from_label(&node.name) {
                    self.adopt(&name);
                }
                self.taxonomies.push(node);
                Ok(id)
            }
            (CopyIn::New, Collision::Differs | Collision::Reserved) => Err(format!(
                "a different taxonomy \"{}\" is already here; replace it or copy under a new name",
                taxonomy.id()
            )),
            (CopyIn::Replace, Collision::Reserved) => Err(format!(
                "\"{}\" is a built-in; copy it under a new name instead",
                taxonomy.id()
            )),
            (CopyIn::Replace, Collision::Absent | Collision::Identical) => Err(format!(
                "there is no different \"{}\" to replace",
                taxonomy.id()
            )),
        }
    }

    fn named(&self, id: &TaxonomyId) -> Option<&TaxonomyNode> {
        self.taxonomies
            .iter()
            .find(|node| TaxonomyId::from_label(&node.name).as_ref() == Ok(id))
    }

    /// A project offers every taxonomy it holds, so a new one joins its available list.
    fn adopt(&mut self, id: &TaxonomyId) {
        if self.is_project() && !self.available.contains(id) {
            self.available.push(id.clone());
        }
    }

    /// A fresh custom node holding `taxonomy`'s content, with new local ids and no baseline.
    fn imported(
        &mut self,
        taxonomy: &Taxonomy,
        name: String,
        name_locked: bool,
        derived_from: Option<TaxonomyLineage>,
    ) -> TaxonomyNode {
        let record: LibraryTaxonomy = if self.is_project() && derived_from.is_none() {
            taxonomy.clone().into()
        } else {
            taxonomy.clone().into_custom().into()
        };
        let mut node = TaxonomyNode::new(&record, TaxonomyOrigin::Custom);
        node.id = self.mint();
        node.baseline = None;
        node.saved_name = None;
        node.name = name;
        node.name_locked = name_locked;
        if derived_from.is_some() {
            node.version = None;
            node.derived_from = derived_from;
        }
        for commit_type in &mut node.types {
            commit_type.id = self.mint();
            commit_type.baseline = None;
            for property in &mut commit_type.properties {
                property.id = self.mint();
                property.baseline = None;
                property.copied_from = None;
            }
        }
        node
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
