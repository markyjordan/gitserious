#![allow(clippy::expect_used)]

use std::cell::{Cell, RefCell};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

use gitserious_app::{
    ConfigurationDestination, ConfigurationEditor, ConfigurationError, ConfigurationSession,
    ConfigurationWorkspace, GlobalConfiguration, GlobalConfigurationStore, ProjectConfig,
    ProjectLock, ProjectState, ProjectStateStore, RepositoryLocator, RepositoryRoot,
    TaxonomyCatalog, TaxonomyOrigin, edit_configuration, revised_taxonomy,
};
use gitserious_core::{
    Description, Taxonomy, TaxonomyId, TaxonomyLineage, TaxonomyVersion, built_in_taxonomies,
};

fn id(value: &str) -> TaxonomyId {
    TaxonomyId::new(value).expect("valid test identity")
}

fn built_in_ids() -> Vec<TaxonomyId> {
    built_in_taxonomies()
        .iter()
        .map(|taxonomy| taxonomy.id().clone())
        .collect()
}

fn custom(name: &str) -> Taxonomy {
    built_in_taxonomies()[0].fork(id(name))
}

fn library(custom: Vec<Taxonomy>) -> Result<GlobalConfiguration, ConfigurationError> {
    GlobalConfiguration::new(1, id("conventional"), built_in_ids(), custom)
}

#[test]
fn built_in_shadowing_and_duplicate_custom_taxonomies_are_rejected() {
    for reserved in ["conventional", "research", "infra-ops"] {
        assert_eq!(
            TaxonomyCatalog::new(&[custom(reserved)]).err(),
            Some(ConfigurationError::DuplicateOrReservedTaxonomy(id(
                reserved
            ))),
            "{reserved} shadowed a built-in"
        );
    }
    assert_eq!(
        TaxonomyCatalog::new(&[custom("team"), custom("team")]).err(),
        Some(ConfigurationError::DuplicateOrReservedTaxonomy(id("team")))
    );
    assert!(library(vec![custom("team"), custom("team")]).is_err());
}

#[test]
fn global_selection_rejects_dangling_and_duplicate_references() {
    let cases = [
        (id("missing"), built_in_ids()),
        (id("conventional"), vec![id("conventional"), id("missing")]),
        (id("conventional"), vec![id("research")]),
        (
            id("conventional"),
            vec![id("conventional"), id("conventional")],
        ),
        (id("conventional"), Vec::new()),
    ];
    for (index, (default, available)) in cases.into_iter().enumerate() {
        assert!(
            GlobalConfiguration::new(1, default, available, Vec::new()).is_err(),
            "case {index} was accepted"
        );
    }
    assert!(GlobalConfiguration::new(2, id("conventional"), built_in_ids(), Vec::new()).is_err());
}

#[test]
fn catalog_lists_built_ins_then_custom_taxonomies_in_storage_order() -> Result<(), Box<dyn Error>> {
    let catalog = library(vec![custom("zeta"), custom("alpha")])?.catalog()?;

    assert_eq!(
        catalog
            .taxonomies()
            .iter()
            .map(|taxonomy| taxonomy.id().as_str())
            .collect::<Vec<_>>(),
        ["conventional", "research", "infra-ops", "zeta", "alpha"]
    );
    assert_eq!(
        catalog.origin(&id("research")),
        Some(TaxonomyOrigin::BuiltIn)
    );
    assert_eq!(catalog.origin(&id("alpha")), Some(TaxonomyOrigin::Custom));
    assert_eq!(catalog.origin(&id("missing")), None);
    Ok(())
}

#[test]
fn created_taxonomies_are_queryable_as_custom_library_entries() -> Result<(), Box<dyn Error>> {
    let mut session = ConfigurationSession::open_global(GlobalConfiguration::default());
    let team = Taxonomy::new(
        id("team"),
        TaxonomyVersion::V1,
        Description::new("Team policy.")?,
        None,
        built_in_taxonomies()[1].commit_types().to_vec(),
    )?;

    session.create_taxonomy(team.clone())?;

    assert_eq!(
        session.global().custom_taxonomies(),
        std::slice::from_ref(&team)
    );
    assert_eq!(session.catalog()?.find(&id("team")), Some(&team));
    assert_eq!(
        session.catalog()?.origin(&id("team")),
        Some(TaxonomyOrigin::Custom)
    );
    assert!(session.is_dirty());
    assert!(session.review()?.contains("Custom taxonomies\n  0 -> 1"));
    Ok(())
}

#[test]
fn failed_create_and_update_leave_the_snapshot_unchanged() -> Result<(), Box<dyn Error>> {
    let mut session = ConfigurationSession::open_global(library(vec![custom("team")])?);
    let before = session.global().clone();

    assert!(session.create_taxonomy(custom("team")).is_err());
    assert!(session.create_taxonomy(custom("research")).is_err());
    assert!(session.update_taxonomy(custom("team")).is_err());
    assert!(session.update_taxonomy(custom("missing")).is_err());

    assert_eq!(session.global(), &before);
    assert!(!session.is_dirty());
    Ok(())
}

#[test]
fn updates_must_advance_the_taxonomy_version() -> Result<(), Box<dyn Error>> {
    let original = custom("team");
    let mut session = ConfigurationSession::open_global(library(vec![original.clone()])?);
    let revised = revised_taxonomy(
        &original,
        Description::new("Revised team policy.")?,
        original.commit_types().to_vec(),
    )?;
    let skipped = Taxonomy::new(
        original.id().clone(),
        TaxonomyVersion::new(3)?,
        original.description().clone(),
        original.derived_from().cloned(),
        original.commit_types().to_vec(),
    )?;

    let error = session
        .update_taxonomy(skipped)
        .expect_err("skipping a version was accepted");
    assert!(error.contains("version 2"), "{error}");
    session.update_taxonomy(revised.clone())?;

    assert_eq!(revised.version(), TaxonomyVersion::new(2)?);
    assert_eq!(revised.derived_from(), original.derived_from());
    assert_eq!(session.catalog()?.find(&id("team")), Some(&revised));
    Ok(())
}

#[test]
fn deletes_protect_selected_and_built_in_taxonomies_then_succeed() -> Result<(), Box<dyn Error>> {
    let mut available = built_in_ids();
    available.push(id("team"));
    let global = GlobalConfiguration::new(
        1,
        id("conventional"),
        available,
        vec![custom("team"), custom("spare")],
    )?;
    let mut session = ConfigurationSession::open_global(global.clone());

    assert!(session.delete_taxonomy(&id("team")).is_err());
    assert!(session.delete_taxonomy(&id("conventional")).is_err());
    assert!(session.delete_taxonomy(&id("missing")).is_err());
    assert_eq!(session.global(), &global);

    session.delete_taxonomy(&id("spare"))?;
    session.configure_global(id("conventional"), built_in_ids())?;
    session.delete_taxonomy(&id("team"))?;

    assert!(session.global().custom_taxonomies().is_empty());
    Ok(())
}

#[test]
fn library_edits_belong_to_the_global_destination() -> Result<(), Box<dyn Error>> {
    let root = RepositoryRoot::new(std::env::temp_dir())?;
    let mut session = ConfigurationSession::open_project(
        root,
        GlobalConfiguration::default(),
        ProjectState::Absent,
    )?;

    for result in [
        session.create_taxonomy(custom("team")),
        session.fork_taxonomy(&id("conventional"), id("team")),
        session.update_taxonomy(custom("team")),
        session.delete_taxonomy(&id("team")),
        session.configure_global(id("research"), built_in_ids()),
    ] {
        let error = result.expect_err("a project session edited the global library");
        assert!(error.contains("Global configuration"), "{error}");
    }
    assert_eq!(session.global(), &GlobalConfiguration::default());
    Ok(())
}

#[test]
fn fork_copies_a_built_in_under_a_new_identity_with_lineage() -> Result<(), Box<dyn Error>> {
    let mut session = ConfigurationSession::open_global(GlobalConfiguration::default());

    session.fork_taxonomy(&id("research"), id("lab"))?;

    let source = &built_in_taxonomies()[1];
    let fork = session
        .catalog()?
        .find(&id("lab"))
        .cloned()
        .ok_or("fork is missing")?;
    assert_eq!(fork.version(), TaxonomyVersion::V1);
    assert_eq!(fork.description(), source.description());
    assert_eq!(fork.commit_types(), source.commit_types());
    assert_eq!(
        fork.derived_from(),
        Some(&TaxonomyLineage::new(id("research"), source.version()))
    );
    assert_eq!(session.catalog()?.find(&id("research")), Some(source));
    Ok(())
}

#[test]
fn fork_rejects_reserved_and_existing_identities_atomically() -> Result<(), Box<dyn Error>> {
    let mut session = ConfigurationSession::open_global(library(vec![custom("team")])?);
    let before = session.global().clone();

    for target in ["conventional", "infra-ops", "team"] {
        let error = session
            .fork_taxonomy(&id("research"), id(target))
            .expect_err("fork overwrote an existing identity");
        assert!(error.contains("already exists"), "{error}");
    }

    assert_eq!(session.global(), &before);
    Ok(())
}

#[test]
fn forks_of_custom_taxonomies_preserve_content_and_reset_the_new_version()
-> Result<(), Box<dyn Error>> {
    let original = custom("team");
    let revised = revised_taxonomy(
        &original,
        Description::new("Second team policy.")?,
        built_in_taxonomies()[2].commit_types().to_vec(),
    )?;
    let mut session = ConfigurationSession::open_global(library(vec![revised.clone()])?);

    session.fork_taxonomy(&id("team"), id("team-copy"))?;

    let fork = session
        .catalog()?
        .find(&id("team-copy"))
        .cloned()
        .ok_or("fork is missing")?;
    assert_eq!(fork.version(), TaxonomyVersion::V1);
    assert_eq!(fork.description(), revised.description());
    assert_eq!(fork.commit_types(), revised.commit_types());
    assert_eq!(
        fork.derived_from(),
        Some(&TaxonomyLineage::new(id("team"), TaxonomyVersion::new(2)?))
    );
    Ok(())
}

#[test]
fn forks_reject_unavailable_sources() {
    let mut session = ConfigurationSession::open_global(GlobalConfiguration::default());

    let error = session
        .fork_taxonomy(&id("missing"), id("team"))
        .expect_err("fork of a missing source was accepted");

    assert!(error.contains("not installed"), "{error}");
    assert!(!session.is_dirty());
}

#[derive(Debug)]
struct FakeError(&'static str);

impl Display for FakeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for FakeError {}

struct FakeGlobalStore {
    current: RefCell<GlobalConfiguration>,
    fail: bool,
    swaps: Cell<usize>,
}

impl FakeGlobalStore {
    fn new(current: GlobalConfiguration) -> Self {
        Self {
            current: RefCell::new(current),
            fail: false,
            swaps: Cell::new(0),
        }
    }
}

impl GlobalConfigurationStore for FakeGlobalStore {
    type Error = FakeError;

    fn load(&self) -> Result<GlobalConfiguration, Self::Error> {
        Ok(self.current.borrow().clone())
    }

    fn compare_and_swap(
        &self,
        expected: &GlobalConfiguration,
        replacement: &GlobalConfiguration,
    ) -> Result<(), Self::Error> {
        self.swaps.set(self.swaps.get() + 1);
        if self.fail {
            return Err(FakeError("global store failed"));
        }
        if *self.current.borrow() != *expected {
            return Err(FakeError("global configuration changed while editing"));
        }
        *self.current.borrow_mut() = replacement.clone();
        Ok(())
    }
}

struct NoRepository;

impl RepositoryLocator for NoRepository {
    type Error = FakeError;

    fn locate(&self, _start: &Path) -> Result<RepositoryRoot, Self::Error> {
        Err(FakeError("no repository"))
    }
}

struct NoProject;

impl ProjectStateStore for NoProject {
    type Error = FakeError;

    fn inspect(&self, _: &RepositoryRoot) -> Result<ProjectState, Self::Error> {
        Err(FakeError("unexpected project read"))
    }
    fn ensure_local_state(&self, _: &RepositoryRoot) -> Result<(), Self::Error> {
        Err(FakeError("unexpected project write"))
    }
    fn initialize(
        &self,
        _: &RepositoryRoot,
        _: &ProjectConfig,
        _: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Err(FakeError("unexpected project write"))
    }
    fn create_lock(&self, _: &RepositoryRoot, _: &ProjectLock) -> Result<(), Self::Error> {
        Err(FakeError("unexpected project write"))
    }
    fn replace_lock(
        &self,
        _: &RepositoryRoot,
        _: &ProjectLock,
        _: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Err(FakeError("unexpected project write"))
    }
    fn compare_and_swap(
        &self,
        _: &RepositoryRoot,
        _: &ProjectConfig,
        _: &ProjectLock,
        _: &ProjectConfig,
        _: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Err(FakeError("unexpected project write"))
    }
}

struct Script<F>(F);

impl<F> ConfigurationEditor for Script<F>
where
    F: Fn(&dyn ConfigurationWorkspace) -> Result<(), String>,
{
    fn edit(&self, workspace: &dyn ConfigurationWorkspace) -> Result<(), String> {
        (self.0)(workspace)
    }
}

fn edit_global<F>(store: &FakeGlobalStore, script: F) -> Result<(), String>
where
    F: Fn(&dyn ConfigurationWorkspace) -> Result<(), String>,
{
    edit_configuration(
        &NoRepository,
        &NoProject,
        store,
        Path::new("."),
        &Script(script),
    )
}

#[test]
fn reviewed_global_sessions_save_one_snapshot_and_reject_concurrent_changes()
-> Result<(), Box<dyn Error>> {
    let store = FakeGlobalStore::new(GlobalConfiguration::default());
    edit_global(&store, |workspace| {
        let mut session = workspace.load(ConfigurationDestination::Global)?;
        session.fork_taxonomy(&id("conventional"), id("team"))?;
        let saved = workspace.save(&session)?;
        assert!(!saved.is_dirty());
        workspace.save(&saved)?;
        Ok(())
    })?;
    assert_eq!(store.swaps.get(), 1);
    assert_eq!(store.current.borrow().custom_taxonomies(), [custom("team")]);

    let stale = FakeGlobalStore::new(GlobalConfiguration::default());
    let error = edit_global(&stale, |workspace| {
        let mut session = workspace.load(ConfigurationDestination::Global)?;
        *stale.current.borrow_mut() = library(vec![custom("other")]).map_err(|e| e.to_string())?;
        session.fork_taxonomy(&id("conventional"), id("team"))?;
        workspace.save(&session).map(|_| ())
    })
    .expect_err("a stale session overwrote a newer library");
    assert!(error.contains("changed while editing"), "{error}");
    assert_eq!(
        stale.current.borrow().custom_taxonomies(),
        [custom("other")]
    );
    Ok(())
}

#[test]
fn global_store_failures_are_reported_and_leave_the_library_unchanged() {
    let mut store = FakeGlobalStore::new(GlobalConfiguration::default());
    store.fail = true;

    let error = edit_global(&store, |workspace| {
        let mut session = workspace.load(ConfigurationDestination::Global)?;
        session.fork_taxonomy(&id("conventional"), id("team"))?;
        workspace.save(&session).map(|_| ())
    })
    .expect_err("a failed save was reported as success");

    assert_eq!(error, "global store failed");
    assert_eq!(*store.current.borrow(), GlobalConfiguration::default());
}

#[test]
fn recreating_an_original_identity_cannot_reset_its_version() -> Result<(), Box<dyn Error>> {
    let original = custom("team");
    let revised = revised_taxonomy(
        &original,
        original.description().clone(),
        original.commit_types().to_vec(),
    )?;
    let mut session = ConfigurationSession::open_global(library(vec![revised.clone()])?);
    session.delete_taxonomy(&id("team"))?;
    let deleted = session.global().clone();

    let recreated = session
        .create_taxonomy(original)
        .expect_err("recreating team reset it to version 1");
    let forked = session
        .fork_taxonomy(&id("research"), id("team"))
        .expect_err("forking into team reset it to version 1");

    assert!(recreated.contains("version 2"), "{recreated}");
    assert!(forked.contains("version 2"), "{forked}");
    assert_eq!(session.global(), &deleted);
    assert_eq!(session.catalog()?.find(&id("team")), None);
    Ok(())
}
