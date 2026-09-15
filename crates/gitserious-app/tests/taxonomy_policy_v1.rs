#![allow(clippy::expect_used)]

use std::error::Error;

use gitserious_app::{
    ConfigurationSession, GlobalConfiguration, ProjectConfig, ProjectLock,
    resolve_effective_configuration,
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

#[test]
fn fork_creation_is_separate_from_availability() -> Result<(), Box<dyn Error>> {
    let mut session = ConfigurationSession::open_global(GlobalConfiguration::default());
    session.fork_taxonomy(&TaxonomyId::new("conventional")?, TaxonomyId::new("team")?)?;
    assert!(session.catalog()?.find(&TaxonomyId::new("team")?).is_some());
    assert!(
        !session
            .global()
            .available_taxonomies()
            .contains(&TaxonomyId::new("team")?)
    );
    assert!(session.is_dirty());
    Ok(())
}

#[test]
fn selected_custom_taxonomy_cannot_be_deleted() -> Result<(), Box<dyn Error>> {
    let mut session = ConfigurationSession::open_global(GlobalConfiguration::default());
    let team = TaxonomyId::new("team")?;
    session.fork_taxonomy(&TaxonomyId::new("conventional")?, team.clone())?;
    let mut available = session.global().available_taxonomies().to_vec();
    available.push(team.clone());
    session.configure_global(team.clone(), available)?;
    let error = session
        .delete_taxonomy(&team)
        .expect_err("selected taxonomy must be protected");
    assert!(error.contains("not installed") || error.contains("available"));
    Ok(())
}
