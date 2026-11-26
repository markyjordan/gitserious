//! The start states the search explores from, their key alphabets, and their workspaces.

use gitserious_app::{
    DraftField, GlobalConfiguration, LibraryDraft, NodeRef, TaxonomyNode, TypeNode,
};
use gitserious_core::{
    CommitTypeDefinition, CommitTypeId, Description, PropertyDefinition, PropertyKey,
    PropertyRequirement, SchemaVersion, Taxonomy, TaxonomyId, TaxonomyVersion,
};

use super::snapshot::{Snapshot, at_outline, editing, selects};
use crate::config::library_lookup::LibraryLookup;
use crate::config::taxonomies::ObservedMode as Mode;
use crate::config::test_support::Workspace;

const COPY_KEYS: &[&str] = &[
    "esc",
    "enter",
    "up",
    "down",
    "left",
    "right",
    "e",
    "a",
    "backspace",
    "space",
    "c",
    "ctrl+s",
    "ctrl+w",
];

pub(super) struct Seed {
    pub(super) name: &'static str,
    pub(super) workspace: fn() -> Result<Workspace, String>,
    pub(super) start: Start,
    pub(super) depth: usize,
    pub(super) alphabet: Alphabet,
    pub(super) reaches: &'static [Reach],
}

/// Where a seed's exploration starts.
pub(super) enum Start {
    /// The Taxonomies outline with the row these names lead to selected, in `mode`.
    Outline {
        path: &'static [&'static str],
        mode: StartMode,
    },
    /// The state these keys reach, which `ready` must accept.
    Keys {
        keys: &'static [&'static str],
        ready: fn(&Snapshot) -> bool,
    },
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum StartMode {
    Nav,
    Edit(DraftField),
}

/// The keys a seed explores after its start.
pub(super) enum Alphabet {
    Every,
    Only(&'static [&'static str]),
}

/// A property of what a seed can reach: every value in `expected` is what `observe` returns for
/// at least one state explored from the seed.
pub(super) struct Reach {
    pub(super) name: &'static str,
    pub(super) observe: fn(&Snapshot) -> Option<String>,
    pub(super) expected: &'static [&'static str],
}

const SUMMARY: &[&str] = &["team", "release", "summary"];

pub(super) const SEEDS: &[Seed] = &[
    Seed {
        name: "an earlier root selected while a later root is expanded",
        workspace: built_ins,
        start: Start::Keys {
            keys: &["2", "down", "enter", "up"],
            ready: |snapshot| {
                at_outline(snapshot)
                    && selects(snapshot, &["conventional"])
                    && snapshot.taxonomies.expanded.len() == 1
            },
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "built-ins at Taxonomies",
        workspace: built_ins,
        start: Start::Outline {
            path: &["conventional"],
            mode: StartMode::Nav,
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "team at Taxonomies",
        workspace: team_library,
        start: Start::Outline {
            path: &["team"],
            mode: StartMode::Nav,
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "team release expanded at Taxonomies",
        workspace: team_library,
        start: Start::Outline {
            path: SUMMARY,
            mode: StartMode::Nav,
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "a saved empty taxonomy in NAV",
        workspace: empty_library,
        start: Start::Outline {
            path: &["unfinished"],
            mode: StartMode::Nav,
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "a saved empty taxonomy with an unlocked name in EDIT",
        workspace: empty_library,
        start: Start::Outline {
            path: &["unfinished"],
            mode: StartMode::Edit(DraftField::Name),
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "a new taxonomy in EDIT on its Name",
        workspace: built_ins,
        start: Start::Keys {
            keys: &["2", "down", "down", "down", "enter"],
            ready: |snapshot| editing(snapshot) == Some(DraftField::Name),
        },
        depth: 3,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "a fork of conventional in EDIT on its Name",
        workspace: built_ins,
        start: Start::Keys {
            keys: &["2", "f"],
            ready: |snapshot| editing(snapshot) == Some(DraftField::Name),
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "team in EDIT on its Description at the limit",
        workspace: team_library,
        start: Start::Outline {
            path: &["team"],
            mode: StartMode::Edit(DraftField::Description),
        },
        depth: 3,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "team summary in EDIT on its Name",
        workspace: team_library,
        start: Start::Outline {
            path: SUMMARY,
            mode: StartMode::Edit(DraftField::Name),
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "team edited at Taxonomies",
        workspace: team_library,
        start: Start::Keys {
            keys: &["2", "down", "down", "down", "e", "backspace", "esc"],
            ready: |snapshot| at_outline(snapshot) && !snapshot.unapplied,
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "a saved fork at Taxonomies",
        workspace: team_library,
        start: Start::Keys {
            keys: &["2", "f", "esc", "up", "d", "y"],
            ready: |snapshot| at_outline(snapshot) && !snapshot.unapplied,
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "a new property from its add row in EDIT on its Name",
        workspace: team_library,
        start: Start::Keys {
            keys: &[
                "2", "down", "down", "down", "enter", "down", "enter", "down", "down", "down",
                "enter",
            ],
            ready: |snapshot| editing(snapshot) == Some(DraftField::Name),
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "team summary conditional with an empty condition in EDIT on its Condition",
        workspace: team_library,
        start: Start::Keys {
            keys: &[
                "2", "down", "down", "down", "enter", "down", "enter", "down", "e", "enter",
                "enter", "right", "right", "enter",
            ],
            ready: |snapshot| editing(snapshot) == Some(DraftField::Condition),
        },
        depth: 2,
        alphabet: Alphabet::Every,
        reaches: &[],
    },
    Seed {
        name: "team summary's policy in NAV",
        workspace: team_library,
        start: Start::Outline {
            path: SUMMARY,
            mode: StartMode::Nav,
        },
        depth: 5,
        alphabet: Alphabet::Only(&["e", "up", "down", "left", "right", "enter"]),
        reaches: &[Reach {
            name: "P4, the Inspector's Policy reaches every policy",
            observe: summary_policy,
            expected: &["required", "recommended", "conditional", "optional"],
        }],
    },
    Seed {
        name: "summary copied to hotfix, on the source in NAV",
        workspace: team_pair_library,
        start: Start::Keys {
            keys: COPY_SUMMARY,
            ready: |snapshot| copied(snapshot) && selects(snapshot, SUMMARY),
        },
        depth: 3,
        alphabet: Alphabet::Only(COPY_KEYS),
        reaches: &[],
    },
    Seed {
        name: "summary copied to hotfix, on the copy in NAV",
        workspace: team_pair_library,
        start: Start::Keys {
            keys: &[
                "2", "down", "down", "down", "enter", "down", "enter", "down", "c", "space",
                "enter", "down", "down", "down", "down", "down",
            ],
            ready: |snapshot| copied(snapshot) && selects(snapshot, &["team", "hotfix", "summary"]),
        },
        depth: 3,
        alphabet: Alphabet::Only(COPY_KEYS),
        reaches: &[],
    },
];

const COPY_SUMMARY: &[&str] = &[
    "2", "down", "down", "down", "enter", "down", "enter", "down", "c", "space", "enter",
];

fn copied(snapshot: &Snapshot) -> bool {
    at_outline(snapshot)
        && matches!(snapshot.taxonomies.mode, Mode::Nav)
        && snapshot.library().is_some_and(|library| {
            library
                .taxonomies()
                .iter()
                .flat_map(TaxonomyNode::types)
                .flat_map(TypeNode::properties)
                .any(|property| property.copied_from().is_some())
        })
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "every seed builds its workspace through one signature"
)]
fn built_ins() -> Result<Workspace, String> {
    Ok(Workspace::new())
}

fn empty_library() -> Result<Workspace, String> {
    let workspace = Workspace::new();
    let record = gitserious_app::LibraryTaxonomy::new(
        TaxonomyId::new("unfinished").map_err(|error| error.to_string())?,
        Some(TaxonomyVersion::V1),
        Description::new("Awaiting its first type").map_err(|error| error.to_string())?,
        None,
        Vec::new(),
        false,
    )
    .map_err(|error| error.to_string())?;
    let default = GlobalConfiguration::default();
    *workspace.global.borrow_mut() = LibraryDraft::from_configuration(
        GlobalConfiguration::new(
            1,
            default.default_taxonomy().clone(),
            default.available_taxonomies().to_vec(),
            vec![record],
        )
        .map_err(|error| error.to_string())?,
    );
    Ok(workspace)
}

fn team_library() -> Result<Workspace, String> {
    workspace_of(team_configuration(Vec::new()))
}

fn team_pair_library() -> Result<Workspace, String> {
    let hotfix = || -> Result<CommitTypeDefinition, Box<dyn std::error::Error>> {
        Ok(CommitTypeDefinition::new(
            Some(SchemaVersion::new(1)?),
            CommitTypeId::new("hotfix")?,
            "Urgent fixes",
            vec![PropertyDefinition::new(
                PropertyKey::new("cause")?,
                "Why it broke",
                PropertyRequirement::Required,
            )?],
        )?)
    };
    workspace_of(hotfix().and_then(|hotfix| team_configuration(vec![hotfix])))
}

fn workspace_of(
    configuration: Result<GlobalConfiguration, Box<dyn std::error::Error>>,
) -> Result<Workspace, String> {
    let workspace = Workspace::new();
    *workspace.global.borrow_mut() =
        LibraryDraft::from_configuration(configuration.map_err(|error| error.to_string())?);
    Ok(workspace)
}

/// `team` with a Description at its limit and one type with two properties, so a property can
/// move, then the `more` types.
fn team_configuration(
    more: Vec<CommitTypeDefinition>,
) -> Result<GlobalConfiguration, Box<dyn std::error::Error>> {
    let description = format!("Team taxonomy{}", ".".repeat(87));
    let team = Taxonomy::new(
        TaxonomyId::new("team")?,
        Some(TaxonomyVersion::new(1)?),
        Description::new(description)?,
        None,
        std::iter::once(CommitTypeDefinition::new(
            Some(SchemaVersion::new(1)?),
            CommitTypeId::new("release")?,
            "Release notes",
            vec![
                PropertyDefinition::new(
                    PropertyKey::new("summary")?,
                    "What shipped",
                    PropertyRequirement::Required,
                )?,
                PropertyDefinition::new(
                    PropertyKey::new("notes")?,
                    "Anything else",
                    PropertyRequirement::Optional,
                )?,
            ],
        )?)
        .chain(more)
        .collect(),
    )?;
    let conventional = TaxonomyId::new("conventional")?;
    Ok(GlobalConfiguration::new(
        1,
        conventional.clone(),
        vec![conventional],
        vec![team.into()],
    )?)
}

/// The policy of `summary` while the outline selects it. `notes` starts at optional, so
/// observing it would meet P4 without any key reaching optional.
fn summary_policy(snapshot: &Snapshot) -> Option<String> {
    if !selects(snapshot, SUMMARY) {
        return None;
    }
    let library = snapshot.library()?;
    let node = library.find_path(&snapshot.taxonomies.cursor_path)?;
    match library.node(&node)? {
        NodeRef::Property { property, .. } => Some(property.policy().label().to_owned()),
        _ => None,
    }
}
