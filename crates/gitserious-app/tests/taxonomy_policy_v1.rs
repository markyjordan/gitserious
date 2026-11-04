#![allow(clippy::expect_used)]

#[path = "support/library_lookup.rs"]
mod library_lookup;
use library_lookup::LibraryLookup;

use std::error::Error;

use gitserious_core::TaxonomyId;

use gitserious_app::{
    ConfigurationSession, DraftEdit, DraftField, GlobalConfiguration, NodeId, Problem, ProblemSite,
};

/// Forks conventional under the name `team`, as forking and typing a name does.
fn fork_team(session: &mut ConfigurationSession) -> Result<NodeId, Box<dyn Error>> {
    let library = session.library_mut().ok_or("a Global session")?;
    let conventional = library.find_path(&["conventional"]).ok_or("conventional")?;
    let fork = library.fork(&conventional)?;
    library.edit(&fork, DraftEdit::Text(DraftField::Name, "team"))?;
    Ok(fork)
}

#[test]
fn fork_creation_is_separate_from_availability() -> Result<(), Box<dyn Error>> {
    let mut session = ConfigurationSession::open_global(GlobalConfiguration::default());
    fork_team(&mut session)?;
    assert!(
        session
            .library()
            .ok_or("Library")?
            .build()
            .map_err(|errors| format!("{errors:?}"))?
            .catalog()?
            .find(&TaxonomyId::new("team")?)
            .is_some()
    );
    assert!(
        !session
            .library()
            .ok_or("a Global session")?
            .available_taxonomies()
            .contains(&TaxonomyId::new("team")?)
    );
    assert!(session.is_dirty());
    Ok(())
}

#[test]
fn a_selected_custom_taxonomy_cannot_be_removed_without_a_problem() -> Result<(), Box<dyn Error>> {
    let mut session = ConfigurationSession::open_global(GlobalConfiguration::default());
    let team = TaxonomyId::new("team")?;
    let fork = fork_team(&mut session)?;
    let mut available = session
        .library()
        .ok_or("a Global session")?
        .available_taxonomies()
        .to_vec();
    available.push(team.clone());
    session.configure_global(team, available)?;
    let library = session.library_mut().ok_or("a Global session")?;
    library.edit(&fork, DraftEdit::Remove)?;
    let problems = session.library().ok_or("Library")?.problems();
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
