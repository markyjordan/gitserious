//! What keeps a draft from building, found while building it.

use std::fmt::{self, Display, Formatter};

use gitserious_core::{
    CommitTypeDefinition, CommitTypeId, ConditionId, Description, LabelError, PropertyCondition,
    PropertyDefinition, PropertyKey, PropertyRequirement, TaxonomyId, built_in_taxonomies,
};

use super::Owner;
use super::{
    DraftField, LibraryDraft, LoadedPath, NodeId, Policy, PropertyNode, TaxonomyNode, TypeNode,
};
use crate::taxonomy_policy::validate_selection;
use crate::{
    ConfigurationError, GlobalConfiguration, LibraryTaxonomy, ProjectConfiguration, TaxonomyOrigin,
};

/// Where a problem sits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProblemSite {
    Field(NodeId, DraftField),
    Selection,
}

/// Why a draft does not build. Data only; a view words it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProblemKind {
    /// A name or condition has no identifier form.
    Label(LabelError),
    /// Another taxonomy, or a sibling, already has this identifier.
    Taken(String),
    /// A description or rationale holds no text.
    Blank,
    /// The default or available taxonomies do not resolve.
    Selection(ConfigurationError),
    /// A project taxonomy has no commit types, so commits could not use it.
    NoTypes,
    /// The draft was asked to build into the other owner's configuration.
    WrongOwner,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Problem {
    pub site: ProblemSite,
    pub kind: ProblemKind,
}

impl Problem {
    /// The node the problem sits on, if any.
    #[must_use]
    pub const fn node(&self) -> Option<&NodeId> {
        match &self.site {
            ProblemSite::Field(node, _) => Some(node),
            ProblemSite::Selection => None,
        }
    }

    fn field(node: &NodeId, field: DraftField, kind: ProblemKind) -> Self {
        Self {
            site: ProblemSite::Field(node.clone(), field),
            kind,
        }
    }
}

impl Display for Problem {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let field = match &self.site {
            ProblemSite::Field(_, field) => field.label().to_lowercase(),
            ProblemSite::Selection => String::new(),
        };
        match &self.kind {
            ProblemKind::Label(error) => write!(formatter, "{field}: {error}"),
            ProblemKind::Taken(id) => write!(formatter, "{field} \"{id}\" is already taken"),
            ProblemKind::Blank => write!(formatter, "{field} is blank"),
            ProblemKind::Selection(error) => error.fmt(formatter),
            ProblemKind::NoTypes => {
                formatter.write_str("a project taxonomy needs at least one commit type")
            }
            ProblemKind::WrongOwner => {
                formatter.write_str("this draft belongs to a different configuration")
            }
        }
    }
}

pub(super) const fn wrong_owner() -> Problem {
    Problem {
        site: ProblemSite::Selection,
        kind: ProblemKind::WrongOwner,
    }
}

pub(super) fn build_owner(draft: &LibraryDraft) -> Result<Owner, Vec<Problem>> {
    if draft.is_project() {
        build_project(draft).map(|(configuration, records)| Owner::Project {
            configuration,
            records,
        })
    } else {
        build(draft).map(Owner::Library)
    }
}

/// A project's taxonomies in outline order; each needs a type, and the default must be one of
/// them. Every taxonomy is available, so the stored available list is not consulted.
fn build_project(
    draft: &LibraryDraft,
) -> Result<(ProjectConfiguration, Vec<LibraryTaxonomy>), Vec<Problem>> {
    let (custom, mut problems) = custom(draft);
    let mut taxonomies = Vec::new();
    for (index, record) in &custom {
        match record.usable() {
            Some(taxonomy) => taxonomies.push(taxonomy),
            None => problems.push(Problem::field(
                &draft.taxonomies[*index].id,
                DraftField::Name,
                ProblemKind::NoTypes,
            )),
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    let configuration = ProjectConfiguration::new(1, draft.default.clone(), taxonomies)
        .map_err(|error| vec![selection(error)])?;
    Ok((
        configuration,
        custom.into_iter().map(|(_, record)| record).collect(),
    ))
}

fn build(draft: &LibraryDraft) -> Result<GlobalConfiguration, Vec<Problem>> {
    let (custom, mut problems) = custom(draft);
    let installed = draft
        .taxonomies
        .iter()
        .filter_map(|taxonomy| TaxonomyId::from_label(&taxonomy.name).ok())
        .collect::<Vec<_>>();
    if let Err(error) = validate_selection(
        |id| installed.contains(id),
        &draft.default,
        &draft.available,
    ) {
        problems.push(selection(error));
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    let custom = custom.into_iter().map(|(_, taxonomy)| taxonomy).collect();
    GlobalConfiguration::new(1, draft.default.clone(), draft.available.clone(), custom)
        .map_err(|error| vec![selection(error)])
}

/// Every custom taxonomy that builds, with its index in the draft, and the problems of the
/// ones that do not.
pub(super) fn custom(draft: &LibraryDraft) -> (Vec<(usize, LibraryTaxonomy)>, Vec<Problem>) {
    let mut problems = Vec::new();
    let mut taken = draft
        .taxonomies
        .iter()
        .filter(|taxonomy| taxonomy.origin == TaxonomyOrigin::BuiltIn)
        .map(|taxonomy| taxonomy.name.clone())
        .collect::<Vec<_>>();
    let mut custom = Vec::new();
    for (index, taxonomy) in draft
        .taxonomies
        .iter()
        .enumerate()
        .filter(|(_, taxonomy)| taxonomy.origin == TaxonomyOrigin::Custom)
    {
        let before = problems.len();
        let id = identifier(
            TaxonomyId::from_label,
            &taxonomy.id,
            &taxonomy.name,
            &mut taken,
            &mut problems,
        );
        match (id, content(draft, taxonomy)) {
            (Some(id), Ok((description, types))) if problems.len() == before => {
                custom.extend(
                    LibraryTaxonomy::new(
                        id,
                        taxonomy.version,
                        description,
                        taxonomy.derived_from.clone(),
                        types,
                        taxonomy.name_locked,
                    )
                    .ok()
                    .map(|built| {
                        (
                            index,
                            built.with_dates(taxonomy.created_on, taxonomy.modified_on),
                        )
                    }),
                );
            }
            (_, Err(more)) => problems.extend(more),
            _ => {}
        }
    }
    (custom, problems)
}

const fn selection(error: ConfigurationError) -> Problem {
    Problem {
        site: ProblemSite::Selection,
        kind: ProblemKind::Selection(error),
    }
}

/// A taxonomy's description and types as apply would write them.
pub(super) fn content(
    draft: &LibraryDraft,
    taxonomy: &TaxonomyNode,
) -> Result<(Description, Vec<CommitTypeDefinition>), Vec<Problem>> {
    let mut problems = Vec::new();
    let description = text(
        &taxonomy.id,
        DraftField::Description,
        &taxonomy.description,
        loaded(draft, taxonomy.baseline(), DraftField::Description),
        &mut problems,
    )
    .and_then(|description| Description::new(description).ok());
    let mut taken = Vec::new();
    let types = taxonomy
        .types
        .iter()
        .map(|commit_type| definition(draft, commit_type, &mut taken, &mut problems))
        .collect::<Vec<_>>();
    match (description, types.into_iter().collect::<Option<Vec<_>>>()) {
        (Some(description), Some(types)) if problems.is_empty() => Ok((description, types)),
        _ => Err(problems),
    }
}

fn definition(
    draft: &LibraryDraft,
    commit_type: &TypeNode,
    taken: &mut Vec<String>,
    problems: &mut Vec<Problem>,
) -> Option<CommitTypeDefinition> {
    let id = identifier(
        CommitTypeId::from_label,
        &commit_type.id,
        &commit_type.name,
        taken,
        problems,
    );
    let description = text(
        &commit_type.id,
        DraftField::Description,
        &commit_type.description,
        loaded(draft, commit_type.baseline(), DraftField::Description),
        problems,
    );
    let mut keys = Vec::new();
    let properties = commit_type
        .properties
        .iter()
        .map(|property| property_definition(draft, property, &mut keys, problems))
        .collect::<Vec<_>>();
    let properties = properties.into_iter().collect::<Option<Vec<_>>>()?;
    CommitTypeDefinition::new(commit_type.schema_version, id?, description?, properties).ok()
}

fn property_definition(
    draft: &LibraryDraft,
    property: &PropertyNode,
    taken: &mut Vec<String>,
    problems: &mut Vec<Problem>,
) -> Option<PropertyDefinition> {
    let key = identifier(
        PropertyKey::from_label,
        &property.id,
        &property.name,
        taken,
        problems,
    );
    let description = text(
        &property.id,
        DraftField::Description,
        &property.description,
        loaded(draft, property.baseline(), DraftField::Description),
        problems,
    );
    let requirement = match property.policy {
        Policy::Required => Some(PropertyRequirement::Required),
        Policy::Recommended => Some(PropertyRequirement::Recommended),
        Policy::Optional => Some(PropertyRequirement::Optional),
        Policy::Conditional => {
            let condition = ConditionId::from_label(&property.condition)
                .map_err(|error| {
                    problems.push(Problem::field(
                        &property.id,
                        DraftField::Condition,
                        ProblemKind::Label(error),
                    ));
                })
                .ok();
            let rationale = text(
                &property.id,
                DraftField::Rationale,
                &property.rationale,
                loaded(draft, property.baseline(), DraftField::Rationale),
                problems,
            );
            PropertyCondition::new(condition?, rationale?)
                .ok()
                .map(PropertyRequirement::Conditional)
        }
    };
    PropertyDefinition::new(key?, description?, requirement?).ok()
}

/// Parses `name` into an identifier no sibling in `taken` has yet, and records it.
fn identifier<T: AsRef<str>>(
    parse: fn(&str) -> Result<T, LabelError>,
    node: &NodeId,
    name: &str,
    taken: &mut Vec<String>,
    problems: &mut Vec<Problem>,
) -> Option<T> {
    match parse(name) {
        Err(error) => {
            problems.push(Problem::field(
                node,
                DraftField::Name,
                ProblemKind::Label(error),
            ));
            None
        }
        Ok(id) if taken.iter().any(|other| other == id.as_ref()) => {
            problems.push(Problem::field(
                node,
                DraftField::Name,
                ProblemKind::Taken(id.as_ref().to_owned()),
            ));
            None
        }
        Ok(id) => {
            taken.push(id.as_ref().to_owned());
            Some(id)
        }
    }
}

/// Prose as apply would write it: as loaded, byte for byte, or once edited, trimmed at its
/// edges. `None` when it holds no text.
fn text<'a>(
    node: &NodeId,
    field: DraftField,
    value: &'a str,
    loaded: Option<&str>,
    problems: &mut Vec<Problem>,
) -> Option<&'a str> {
    if value.trim().is_empty() {
        problems.push(Problem::field(node, field, ProblemKind::Blank));
        None
    } else if loaded == Some(value) {
        Some(value)
    } else {
        Some(value.trim())
    }
}

/// The prose `field` held in the stored node at `baseline` when the draft was loaded.
fn loaded<'a>(
    draft: &'a LibraryDraft,
    baseline: Option<&LoadedPath>,
    field: DraftField,
) -> Option<&'a str> {
    let (taxonomy, rest) = baseline?.names().split_first()?;
    let (description, types) = if let Some(stored) = draft
        .records()
        .iter()
        .find(|stored| stored.id().as_str() == taxonomy)
    {
        (stored.description(), stored.commit_types())
    } else {
        let stored = built_in_taxonomies()
            .iter()
            .find(|stored| stored.id().as_str() == taxonomy)?;
        (stored.description(), stored.commit_types())
    };
    let Some((commit_type, rest)) = rest.split_first() else {
        return Some(description.as_str());
    };
    let commit_type = types
        .iter()
        .find(|stored| stored.id().as_str() == commit_type)?;
    let Some(key) = rest.first() else {
        return Some(commit_type.description());
    };
    let property = commit_type
        .properties()
        .iter()
        .find(|stored| stored.key().as_str() == key)?;
    match (field, property.requirement()) {
        (DraftField::Rationale, PropertyRequirement::Conditional(condition)) => {
            Some(condition.rationale())
        }
        (DraftField::Rationale, _) => None,
        _ => Some(property.description()),
    }
}
