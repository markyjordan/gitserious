use gitserious_core::{
    CommitTypeId, ConditionId, Description, LabelError, PropertyCondition, PropertyKey,
    SchemaVersion, TaxonomyId,
};

use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

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
    let source = &built_in_taxonomies()[0];
    let zeta = Taxonomy::new(
        TaxonomyId::new("zeta")?,
        TaxonomyVersion::V1,
        source.description().clone(),
        Some(TaxonomyLineage::new(source.id().clone(), source.version())),
        source.commit_types().to_vec(),
    )?;
    let conventional = TaxonomyId::new("conventional")?;
    Ok(GlobalConfiguration::new(
        1,
        conventional.clone(),
        vec![conventional],
        vec![zeta, team],
    )?)
}

fn find(draft: &LibraryDraft, path: &[&str]) -> Result<NodeId, Box<dyn std::error::Error>> {
    draft
        .find_path(path)
        .ok_or_else(|| format!("{path:?} is missing").into())
}

#[test]
fn built_ins_come_first_then_custom_taxonomies_in_storage_order() -> TestResult {
    let draft = LibraryDraft::from_configuration(team()?);
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
fn rebuilding_from_the_same_configuration_keeps_every_node_id() -> TestResult {
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
    let first = ids(&LibraryDraft::from_configuration(configuration.clone()));
    assert_eq!(
        first,
        ids(&LibraryDraft::from_configuration(configuration.clone()))
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
fn reads_walk_from_a_node_to_its_ancestors() -> TestResult {
    let draft = LibraryDraft::from_configuration(team()?);
    let rollback = find(&draft, &["team", "release", "rollback"])?;
    let Some(NodeRef::Property { property, .. }) = draft.node(&rollback) else {
        return Err("rollback is a property".into());
    };
    assert_eq!(property.policy(), Policy::Conditional);
    assert_eq!(draft.text(&rollback, DraftField::Condition), Some("risky"));
    assert_eq!(
        draft.text(&rollback, DraftField::Rationale),
        Some("Risky releases need a way back.")
    );
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

#[test]
fn a_loaded_draft_builds_back_to_its_configuration_unchanged() -> TestResult {
    let configuration = team()?;
    let draft = LibraryDraft::from_configuration(configuration.clone());
    assert_eq!(draft.build(), Ok(configuration));
    assert!(!draft.is_changed());
    assert!(draft.problems().is_empty());
    Ok(())
}

#[test]
fn edits_refuse_built_ins_stored_names_missing_fields_and_growth_past_the_limit() -> TestResult {
    let mut draft = LibraryDraft::from_configuration(team()?);
    let feat = find(&draft, &["conventional", "feat"])?;
    let team = find(&draft, &["team"])?;
    let summary = find(&draft, &["team", "release", "summary"])?;
    let release = find(&draft, &["team", "release"])?;
    assert_eq!(
        draft.edit(&feat, DraftEdit::Text(DraftField::Name, "x")),
        Err(EditRefusal::ReadOnly)
    );
    assert_eq!(draft.fields(&feat), &[] as &[DraftField]);
    assert_eq!(
        draft.edit(&team, DraftEdit::Text(DraftField::Name, "crew")),
        Err(EditRefusal::NameLocked)
    );
    assert_eq!(draft.fields(&team), &[DraftField::Description]);
    for (node, edit) in [
        (&release, DraftEdit::Text(DraftField::Policy, "x")),
        (&release, DraftEdit::Text(DraftField::Condition, "x")),
        (&release, DraftEdit::Policy(Policy::Optional)),
        (&summary, DraftEdit::AddProperty),
        (&release, DraftEdit::AddType),
    ] {
        assert_eq!(draft.edit(node, edit), Err(EditRefusal::NotApplicable));
    }
    let at_limit = "n".repeat(80);
    draft.edit(&summary, DraftEdit::Text(DraftField::Name, &at_limit))?;
    assert_eq!(
        draft.edit(&summary, DraftEdit::Text(DraftField::Name, &"n".repeat(81))),
        Err(EditRefusal::TooLong)
    );
    assert_eq!(
        draft.text(&summary, DraftField::Name),
        Some(at_limit.as_str())
    );
    Ok(())
}

#[test]
fn an_over_limit_loaded_field_may_shrink_but_not_grow() -> TestResult {
    let mut draft = LibraryDraft::from_configuration(team()?);
    let summary = find(&draft, &["team", "release", "summary"])?;
    let long = "d".repeat(130);
    let address = draft.address(&summary).ok_or("summary")?;
    *draft.text_mut(address, DraftField::Description) = long.clone();
    assert_eq!(
        draft.edit(
            &summary,
            DraftEdit::Text(DraftField::Description, &format!("{long}d"))
        ),
        Err(EditRefusal::TooLong)
    );
    draft.edit(
        &summary,
        DraftEdit::Text(DraftField::Description, &"d".repeat(129)),
    )?;
    Ok(())
}

#[test]
fn condition_and_rationale_survive_cycling_the_policy_away_and_back() -> TestResult {
    let mut draft = LibraryDraft::from_configuration(team()?);
    let rollback = find(&draft, &["team", "release", "rollback"])?;
    draft.edit(
        &rollback,
        DraftEdit::Policy(Policy::Conditional.cycle(true)),
    )?;
    assert_eq!(
        draft.fields(&rollback),
        &[
            DraftField::Name,
            DraftField::Description,
            DraftField::Policy
        ]
    );
    assert!(draft.is_changed());
    draft.edit(&rollback, DraftEdit::Policy(Policy::Optional.cycle(false)))?;
    assert_eq!(draft.text(&rollback, DraftField::Condition), Some("risky"));
    assert!(!draft.is_changed());
    assert_eq!(Policy::Optional.cycle(true), Policy::Required);
    assert_eq!(Policy::Required.cycle(false), Policy::Optional);
    Ok(())
}

#[test]
fn added_nodes_start_blank_and_removal_lands_on_a_neighbor() -> TestResult {
    let mut draft = LibraryDraft::from_configuration(team()?);
    let team = find(&draft, &["team"])?;
    let release = find(&draft, &["team", "release"])?;
    let summary = find(&draft, &["team", "release", "summary"])?;
    let rollback = find(&draft, &["team", "release", "rollback"])?;
    let added = draft.edit(&release, DraftEdit::AddProperty)?;
    assert!(matches!(added, NodeId::Local(_)));
    assert_eq!(draft.parent(&added), Some(release.clone()));
    assert_eq!(draft.text(&added, DraftField::Name), Some(""));
    assert!(draft.problems().iter().any(|problem| problem
        == &Problem {
            site: ProblemSite::Field(added.clone(), DraftField::Name),
            kind: ProblemKind::Label(LabelError::Empty),
        }));
    assert_eq!(draft.edit(&rollback, DraftEdit::Remove)?, added);
    assert_eq!(draft.edit(&added, DraftEdit::Remove)?, summary);
    assert_eq!(draft.edit(&summary, DraftEdit::Remove)?, release);
    let other = draft.edit(&team, DraftEdit::AddType)?;
    assert_eq!(draft.edit(&release, DraftEdit::Remove)?, other);
    assert_eq!(
        draft.edit(&team, DraftEdit::Remove)?,
        find(&draft, &["zeta"])?
    );
    Ok(())
}

#[test]
fn edits_advance_a_stored_version_once_and_restoring_the_content_restores_it() -> TestResult {
    let mut draft = LibraryDraft::from_configuration(team()?);
    let team = find(&draft, &["team"])?;
    for description in ["First revision.", "Second revision."] {
        draft.edit(&team, DraftEdit::Text(DraftField::Description, description))?;
        assert_eq!(draft.version(&team), TaxonomyVersion::new(4).ok());
    }
    draft.edit(
        &team,
        DraftEdit::Text(DraftField::Description, "Team taxonomy"),
    )?;
    assert_eq!(draft.version(&team), TaxonomyVersion::new(3).ok());
    assert!(!draft.is_changed());
    Ok(())
}

#[test]
fn a_fork_copies_its_source_under_a_new_name_with_lineage_and_source_baselines() -> TestResult {
    let mut draft = LibraryDraft::from_configuration(team()?);
    let feat = find(&draft, &["conventional", "feat"])?;
    let fork = draft.fork(&feat)?;
    let Some(NodeRef::Taxonomy(copy)) = draft.node(&fork) else {
        return Err("the fork is a taxonomy".into());
    };
    assert_eq!(copy.name(), "conventional-fork");
    assert_eq!(copy.origin(), TaxonomyOrigin::Custom);
    assert_eq!(
        copy.derived_from()
            .map(|source| format!("{}@{}", source.id(), source.version())),
        Some("conventional@2".into())
    );
    let source = &draft.taxonomies()[0];
    assert_eq!(copy.types().len(), source.types().len());
    assert!(
        copy.types()
            .iter()
            .zip(source.types())
            .all(|(copy, source)| {
                matches!(copy.id(), NodeId::Local(_)) && copy.baseline() == source.baseline()
            })
    );
    assert_eq!(draft.version(&fork), Some(TaxonomyVersion::V1));
    assert_eq!(
        draft.fields(&fork),
        &[DraftField::Name, DraftField::Description]
    );
    let built = draft.build().map_err(|problems| format!("{problems:?}"))?;
    assert_eq!(
        built.custom_taxonomies().last().map(Taxonomy::commit_types),
        Some(built_in_taxonomies()[0].commit_types())
    );
    Ok(())
}

#[test]
fn problems_name_every_field_that_does_not_build() -> TestResult {
    let mut draft = LibraryDraft::from_configuration(team()?);
    let rollback = find(&draft, &["team", "release", "rollback"])?;
    draft.edit(&rollback, DraftEdit::Text(DraftField::Condition, ""))?;
    draft.edit(&rollback, DraftEdit::Text(DraftField::Rationale, " "))?;
    let created = draft.create_taxonomy();
    draft.edit(&created, DraftEdit::Text(DraftField::Name, "Team"))?;
    assert_eq!(
        draft.problems(),
        [
            Problem {
                site: ProblemSite::Field(rollback.clone(), DraftField::Condition),
                kind: ProblemKind::Label(LabelError::Empty),
            },
            Problem {
                site: ProblemSite::Field(rollback, DraftField::Rationale),
                kind: ProblemKind::Blank,
            },
            Problem {
                site: ProblemSite::Field(created.clone(), DraftField::Name),
                kind: ProblemKind::Taken("team".into()),
            },
            Problem {
                site: ProblemSite::Field(created.clone(), DraftField::Description),
                kind: ProblemKind::Blank,
            },
            Problem {
                site: ProblemSite::Node(created),
                kind: ProblemKind::NoTypes,
            },
        ]
    );
    assert!(draft.build().is_err());
    assert!(draft.is_changed());
    Ok(())
}

#[test]
fn a_stored_name_stays_reserved_while_its_deletion_is_unapplied() -> TestResult {
    let mut draft = LibraryDraft::from_configuration(team()?);
    let team = find(&draft, &["team"])?;
    draft.edit(&team, DraftEdit::Remove)?;
    let fork = draft.fork(&find(&draft, &["research"])?)?;
    draft.edit(&fork, DraftEdit::Text(DraftField::Name, "team"))?;
    assert_eq!(
        draft.problems(),
        [Problem {
            site: ProblemSite::Field(fork, DraftField::Name),
            kind: ProblemKind::StagedForDeletion("team".into()),
        }]
    );
    Ok(())
}

#[test]
fn removing_a_selected_taxonomy_is_a_selection_problem() -> TestResult {
    let configuration = team()?;
    let mut available = configuration.available_taxonomies().to_vec();
    available.push(TaxonomyId::new("team")?);
    let mut draft = LibraryDraft::from_configuration(configuration.clone());
    draft.select(TaxonomyId::new("team")?, available);
    assert!(draft.problems().is_empty());
    draft.edit(&find(&draft, &["team"])?, DraftEdit::Remove)?;
    assert!(matches!(
        draft.problems().as_slice(),
        [Problem {
            site: ProblemSite::Selection,
            kind: ProblemKind::Selection(_),
        }]
    ));
    Ok(())
}

#[test]
fn a_move_swaps_a_custom_type_or_property_with_its_neighbor_and_stays_at_either_end() -> TestResult
{
    let mut draft = LibraryDraft::from_configuration(team()?);
    let team = find(&draft, &["team"])?;
    let release = find(&draft, &["team", "release"])?;
    let summary = find(&draft, &["team", "release", "summary"])?;
    let rollback = find(&draft, &["team", "release", "rollback"])?;
    let keys = |draft: &LibraryDraft| -> Vec<String> {
        draft
            .node(&release)
            .map(|node| match node {
                NodeRef::Type { commit_type, .. } => commit_type
                    .properties()
                    .iter()
                    .map(|property| property.name().to_owned())
                    .collect(),
                _ => Vec::new(),
            })
            .unwrap_or_default()
    };
    assert_eq!(
        draft.edit(&summary, DraftEdit::Move { down: true })?,
        summary
    );
    assert_eq!(keys(&draft), ["rollback", "summary"]);
    assert!(draft.is_changed());
    assert_eq!(
        draft.edit(&summary, DraftEdit::Move { down: true })?,
        summary
    );
    assert_eq!(keys(&draft), ["rollback", "summary"]);
    draft.edit(&rollback, DraftEdit::Move { down: true })?;
    assert_eq!(keys(&draft), ["summary", "rollback"]);
    assert!(!draft.is_changed());
    let added = draft.edit(&team, DraftEdit::AddType)?;
    draft.edit(&added, DraftEdit::Move { down: false })?;
    assert_eq!(
        draft.taxonomies()[4]
            .types()
            .iter()
            .map(|commit_type| commit_type.id().clone())
            .collect::<Vec<_>>(),
        [added.clone(), release]
    );
    assert_eq!(draft.edit(&added, DraftEdit::Move { down: false })?, added);
    assert_eq!(
        draft.edit(&team, DraftEdit::Move { down: true }),
        Err(EditRefusal::NotApplicable)
    );
    assert_eq!(
        draft.edit(
            &find(&draft, &["conventional", "feat"])?,
            DraftEdit::Move { down: true }
        ),
        Err(EditRefusal::ReadOnly)
    );
    Ok(())
}

#[test]
fn edited_prose_builds_trimmed_and_loaded_prose_stays_byte_for_byte() -> TestResult {
    let mut draft = LibraryDraft::from_configuration(team()?);
    let summary = find(&draft, &["team", "release", "summary"])?;
    let rollback = find(&draft, &["team", "release", "rollback"])?;
    draft.edit(
        &summary,
        DraftEdit::Text(DraftField::Description, "What shipped  "),
    )?;
    draft.edit(
        &rollback,
        DraftEdit::Text(DraftField::Rationale, " Risky releases need a way back.\t"),
    )?;
    assert!(!draft.is_changed());
    assert_eq!(
        draft.version(&summary),
        TaxonomyVersion::new(3).ok(),
        "trailing spaces alone do not advance the version"
    );
    assert_eq!(
        draft.text(&summary, DraftField::Description),
        Some("What shipped  ")
    );
    draft.edit(
        &summary,
        DraftEdit::Text(DraftField::Description, "  What went out "),
    )?;
    let built = draft.build().map_err(|problems| format!("{problems:?}"))?;
    let release = &built.custom_taxonomies()[1].commit_types()[0];
    assert_eq!(release.properties()[0].description(), "What went out");

    let padded = Taxonomy::new(
        TaxonomyId::new("padded")?,
        TaxonomyVersion::V1,
        Description::new(" Padded taxonomy \n")?,
        None,
        built_in_taxonomies()[0].commit_types().to_vec(),
    )?;
    let conventional = TaxonomyId::new("conventional")?;
    let configuration =
        GlobalConfiguration::new(1, conventional.clone(), vec![conventional], vec![padded])?;
    let draft = LibraryDraft::from_configuration(configuration.clone());
    assert_eq!(draft.build(), Ok(configuration));
    Ok(())
}

#[test]
fn removing_the_default_or_an_available_taxonomy_names_the_selection_error() -> TestResult {
    let mut draft = LibraryDraft::from_configuration(team()?);
    draft.select(TaxonomyId::new("team")?, vec![TaxonomyId::new("team")?]);
    let team = find(&draft, &["team"])?;
    assert_eq!(
        draft.removal_blocker(&team).map(|error| error.to_string()),
        Some("taxonomy \"team\" is not installed".into())
    );
    draft.select(
        TaxonomyId::new("conventional")?,
        vec![TaxonomyId::new("conventional")?, TaxonomyId::new("zeta")?],
    );
    assert_eq!(draft.removal_blocker(&team), None);
    assert_eq!(
        draft
            .removal_blocker(&find(&draft, &["zeta"])?)
            .map(|error| error.to_string()),
        Some("taxonomy \"zeta\" is not installed".into())
    );
    assert_eq!(
        draft.removal_blocker(&find(&draft, &["zeta", "feat"])?),
        None
    );
    Ok(())
}

#[test]
fn a_fork_name_fits_the_name_limit() -> TestResult {
    let mut draft = LibraryDraft::from_configuration(team()?);
    let long = draft.create_taxonomy();
    let name = "t".repeat(80);
    draft.edit(&long, DraftEdit::Text(DraftField::Name, &name))?;
    let fork = draft.fork(&long)?;
    assert_eq!(
        draft.text(&fork, DraftField::Name),
        Some(format!("{}-fork", "t".repeat(75)).as_str())
    );
    let fork = draft.fork(&find(&draft, &["team"])?)?;
    assert_eq!(draft.text(&fork, DraftField::Name), Some("team-fork"));
    Ok(())
}
