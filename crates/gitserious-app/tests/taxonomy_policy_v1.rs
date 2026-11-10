#![allow(clippy::expect_used)]

#[path = "support/library_lookup.rs"]
mod library_lookup;
use library_lookup::LibraryLookup;

use std::error::Error;

use gitserious_core::TaxonomyId;

use gitserious_app::{
    DraftEdit, DraftField, GlobalConfiguration, LibraryDraft, NodeId, Problem, ProblemSite,
};

/// Forks conventional under the name `team`, as forking and typing a name does.
fn fork_team(session: &mut LibraryDraft) -> Result<NodeId, Box<dyn Error>> {
    let library = session;
    let conventional = library.find_path(&["conventional"]).ok_or("conventional")?;
    let fork = library.fork(&conventional)?;
    library.edit(&fork, DraftEdit::Text(DraftField::Name, "team"))?;
    Ok(fork)
}

#[test]
fn fork_creation_is_separate_from_availability() -> Result<(), Box<dyn Error>> {
    let mut session = LibraryDraft::from_configuration(GlobalConfiguration::default());
    fork_team(&mut session)?;
    assert!(
        session
            .build()
            .map_err(|errors| format!("{errors:?}"))?
            .catalog()?
            .find(&TaxonomyId::new("team")?)
            .is_some()
    );
    assert!(
        !session
            .build()
            .map_err(|errors| format!("{errors:?}"))?
            .available_taxonomies()
            .contains(&TaxonomyId::new("team")?)
    );
    assert!(session.is_changed());
    Ok(())
}

#[test]
fn a_selected_custom_taxonomy_cannot_be_removed_without_a_problem() -> Result<(), Box<dyn Error>> {
    let mut session = LibraryDraft::from_configuration(GlobalConfiguration::default());
    let team = TaxonomyId::new("team")?;
    fork_team(&mut session)?;
    let built = session.build().map_err(|errors| format!("{errors:?}"))?;
    let mut available = built.available_taxonomies().to_vec();
    available.push(team.clone());
    let selected =
        GlobalConfiguration::new(1, team, available, built.custom_taxonomies().to_vec())?;
    let mut library = LibraryDraft::from_configuration(selected);
    let team = library.find_path(&["team"]).ok_or("team")?;
    library.edit(&team, DraftEdit::Remove)?;
    let problems = library.problems();
    assert!(
        matches!(
            problems.as_slice(),
            [Problem {
                site: ProblemSite::Selection,
                ..
            }]
        ),
        "{problems:?}"
    );
    Ok(())
}
