#![allow(clippy::expect_used)]

use std::error::Error;

use gitserious_app::{
    ConfigurationSession, DraftEdit, DraftField, GlobalConfiguration, NodeId, ProjectConfig,
    ProjectLock, resolve_effective_configuration,
};
use gitserious_core::{TaxonomyId, built_in_taxonomies};

#[test]
fn project_fields_fall_back_independently() -> Result<(), Box<dyn Error>> {
    let global = GlobalConfiguration::default();
    let project = ProjectConfig::new(1, Some(TaxonomyId::new("research")?), None)?;
    let effective = resolve_effective_configuration(&global, &project)?;
    assert_eq!(effective.default_taxonomy().as_str(), "research");
    assert_eq!(
        effective
            .taxonomies()
            .iter()
            .map(|item| item.id().as_str())
            .collect::<Vec<_>>(),
        ["conventional", "research", "infra-ops"]
    );
    Ok(())
}

#[test]
fn portable_lock_keeps_complete_taxonomies() -> Result<(), Box<dyn Error>> {
    let global = GlobalConfiguration::default();
    let project = ProjectConfig::inherited();
    let effective = resolve_effective_configuration(&global, &project)?;
    let lock = ProjectLock::capture(&project, &effective)?;
    assert!(lock.matches(&project));
    assert_eq!(lock.taxonomies(), built_in_taxonomies());
    assert_eq!(lock.default_taxonomy().as_str(), "conventional");
    Ok(())
}

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
    assert!(session.catalog()?.find(&TaxonomyId::new("team")?).is_some());
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
    let error = session
        .review()
        .expect_err("selected taxonomy must be protected");
    assert!(error.contains("not installed") || error.contains("available"));
    Ok(())
}
