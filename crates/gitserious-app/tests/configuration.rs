#![allow(clippy::expect_used)]

use std::cell::{Cell, RefCell};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

use gitserious_app::{
    ConfigurationDestination, ConfigurationEditor, ConfigurationError, ConfigurationSession,
    ConfigurationWorkspace, DraftEdit, DraftField, EditRefusal, GlobalConfiguration,
    GlobalConfigurationStore, LibraryDraft, NodeId, ProblemKind, ProjectConfig, ProjectLock,
    ProjectState, ProjectStateStore, RepositoryLocator, RepositoryRoot, TaxonomyCatalog,
    TaxonomyOrigin, edit_configuration,
};
use gitserious_core::{
    CommitTypeDefinition, CommitTypeId, Description, PropertyDefinition, PropertyKey,
    PropertyRequirement, SchemaVersion, Taxonomy, TaxonomyId, TaxonomyLineage, TaxonomyVersion,
    built_in_taxonomies,
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
    let source = &built_in_taxonomies()[0];
    Taxonomy::new(
        id(name),
        TaxonomyVersion::V1,
        source.description().clone(),
        Some(TaxonomyLineage::new(source.id().clone(), source.version())),
        source.commit_types().to_vec(),
    )
    .expect("valid test taxonomy")
}

fn at_version(original: &Taxonomy, version: u16, description: &str) -> Taxonomy {
    Taxonomy::new(
        original.id().clone(),
        TaxonomyVersion::new(version).expect("valid test version"),
        Description::new(description).expect("valid test description"),
        original.derived_from().cloned(),
        original.commit_types().to_vec(),
    )
    .expect("valid test taxonomy")
}

fn library(custom: Vec<Taxonomy>) -> Result<GlobalConfiguration, ConfigurationError> {
    GlobalConfiguration::new(1, id("conventional"), built_in_ids(), custom)
}

fn draft(session: &mut ConfigurationSession) -> &mut LibraryDraft {
    session
        .library_mut()
        .expect("a Global session holds the Library")
}

fn node(session: &mut ConfigurationSession, path: &[&str]) -> NodeId {
    draft(session).find_path(path).expect("the node exists")
}

/// Forks `source` under the name `target`, as forking and typing a name does.
fn fork_as(session: &mut ConfigurationSession, source: &str, target: &str) -> NodeId {
    let source = node(session, &[source]);
    let library = draft(session);
    let fork = library.fork(&source).expect("the source forks");
    library
        .edit(&fork, DraftEdit::Text(DraftField::Name, target))
        .expect("a fork takes a name");
    fork
}

fn describe(session: &mut ConfigurationSession, taxonomy: &str, description: &str) {
    let taxonomy = node(session, &[taxonomy]);
    draft(session)
        .edit(
            &taxonomy,
            DraftEdit::Text(DraftField::Description, description),
        )
        .expect("a custom taxonomy takes a description");
}

fn problems(session: &ConfigurationSession) -> Vec<ProblemKind> {
    session
        .library()
        .expect("a Global session holds the Library")
        .problems()
        .into_iter()
        .map(|problem| problem.kind)
        .collect()
}

fn built(session: &ConfigurationSession) -> GlobalConfiguration {
    session
        .library()
        .and_then(|library| library.build().ok())
        .expect("the Library builds")
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
    assert_eq!(
        library(vec![custom("team"), custom("team")]).map_err(|error| error.to_string()),
        Err("taxonomy \"team\" repeats or shadows an existing identity".to_owned())
    );
}

#[test]
fn global_selection_rejects_dangling_and_duplicate_references() {
    let cases = [
        (
            id("missing"),
            built_in_ids(),
            "default taxonomy \"missing\" must also be available",
        ),
        (
            id("conventional"),
            vec![id("conventional"), id("missing")],
            "taxonomy \"missing\" is not installed",
        ),
        (
            id("conventional"),
            vec![id("research")],
            "default taxonomy \"conventional\" must also be available",
        ),
        (
            id("conventional"),
            vec![id("conventional"), id("conventional")],
            "available taxonomies repeat \"conventional\"",
        ),
        (
            id("conventional"),
            Vec::new(),
            "at least one taxonomy must be available",
        ),
    ];
    for (default, available, expected) in cases {
        assert_eq!(
            GlobalConfiguration::new(1, default, available, Vec::new())
                .map_err(|error| error.to_string()),
            Err(expected.to_owned())
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
    let library = draft(&mut session);
    let team = library.create_taxonomy();
    let release = library.edit(&team, DraftEdit::AddType)?;
    let summary = library.edit(&release, DraftEdit::AddProperty)?;
    for (node, field, text) in [
        (&team, DraftField::Name, "team"),
        (&team, DraftField::Description, "Team policy."),
        (&release, DraftField::Name, "Release"),
        (&release, DraftField::Description, "Release notes"),
        (&summary, DraftField::Name, "summary"),
        (&summary, DraftField::Description, "What shipped"),
    ] {
        library.edit(node, DraftEdit::Text(field, text))?;
    }
    let expected = Taxonomy::new(
        id("team"),
        TaxonomyVersion::V1,
        Description::new("Team policy.")?,
        None,
        vec![CommitTypeDefinition::new(
            SchemaVersion::new(1)?,
            CommitTypeId::new("release")?,
            "Release notes",
            vec![PropertyDefinition::new(
                PropertyKey::new("summary")?,
                "What shipped",
                PropertyRequirement::Required,
            )?],
        )?],
    )?;

    assert_eq!(
        built(&session).custom_taxonomies(),
        std::slice::from_ref(&expected)
    );
    assert_eq!(session.catalog()?.find(&id("team")), Some(&expected));
    assert_eq!(
        session.catalog()?.origin(&id("team")),
        Some(TaxonomyOrigin::Custom)
    );
    assert!(session.is_dirty());
    assert!(
        session
            .review()?
            .changes
            .contains("Taxonomies\n  created  team v1, ")
    );
    Ok(())
}

#[test]
fn names_already_in_the_library_are_problems_until_renamed() -> Result<(), Box<dyn Error>> {
    let mut session = ConfigurationSession::open_global(library(vec![custom("team")])?);

    for existing in ["team", "research"] {
        let created = draft(&mut session).create_taxonomy();
        draft(&mut session).edit(&created, DraftEdit::Text(DraftField::Name, existing))?;
        assert!(
            problems(&session).contains(&ProblemKind::Taken(existing.into())),
            "{existing} was not reported as taken"
        );
        draft(&mut session).edit(&created, DraftEdit::Remove)?;
    }
    let release = node(&mut session, &["team", "feat"]);
    draft(&mut session).edit(&release, DraftEdit::Text(DraftField::Description, " "))?;
    assert_eq!(problems(&session), [ProblemKind::Blank]);
    assert_eq!(session.review()?.problems.len(), 1);
    assert!(session.is_dirty());
    Ok(())
}

#[test]
fn updates_that_change_nothing_leave_the_library_unchanged() -> Result<(), Box<dyn Error>> {
    let original = custom("team");
    let mut session = ConfigurationSession::open_global(library(vec![original.clone()])?);

    describe(&mut session, "team", original.description().as_str());

    assert_eq!(built(&session), library(vec![original])?);
    assert!(!session.is_dirty());
    Ok(())
}

#[test]
fn edits_in_one_session_advance_the_version_once() -> Result<(), Box<dyn Error>> {
    let original = custom("team");
    let mut session = ConfigurationSession::open_global(library(vec![original.clone()])?);

    for description in ["First revision.", "Second revision."] {
        describe(&mut session, "team", description);
        assert_eq!(
            session.catalog()?.find(&id("team")),
            Some(&at_version(&original, 2, description))
        );
    }
    describe(&mut session, "team", original.description().as_str());

    assert_eq!(session.catalog()?.find(&id("team")), Some(&original));
    assert!(!session.is_dirty());
    Ok(())
}

#[test]
fn each_applied_edit_advances_the_version_by_one() -> Result<(), Box<dyn Error>> {
    let original = custom("team");
    let store = FakeGlobalStore::new(library(vec![original.clone()])?);

    edit_global(&store, |workspace| {
        let mut session = workspace.load(ConfigurationDestination::Global)?;
        describe(&mut session, "team", "First revision.");
        let mut session = workspace.save(&session)?;
        describe(&mut session, "team", "Second revision.");
        describe(&mut session, "team", "Third revision.");
        workspace.save(&session).map(|_| ())
    })?;

    assert_eq!(
        store.current.borrow().custom_taxonomies(),
        [at_version(&original, 3, "Third revision.")]
    );
    Ok(())
}

#[test]
fn edits_to_a_taxonomy_created_in_the_session_keep_its_first_version() -> Result<(), Box<dyn Error>>
{
    let mut session = ConfigurationSession::open_global(GlobalConfiguration::default());
    fork_as(&mut session, "conventional", "team");

    describe(&mut session, "team", "Team policy.");

    assert_eq!(
        session.catalog()?.find(&id("team")),
        Some(&at_version(&custom("team"), 1, "Team policy."))
    );
    Ok(())
}

#[test]
fn deletes_refuse_built_ins_and_a_selected_taxonomy_does_not_build_until_unselected()
-> Result<(), Box<dyn Error>> {
    let mut available = built_in_ids();
    available.push(id("team"));
    let global = GlobalConfiguration::new(
        1,
        id("conventional"),
        available,
        vec![custom("team"), custom("spare")],
    )?;
    let mut session = ConfigurationSession::open_global(global);

    let conventional = node(&mut session, &["conventional"]);
    assert_eq!(
        draft(&mut session).edit(&conventional, DraftEdit::Remove),
        Err(EditRefusal::ReadOnly)
    );
    let spare = node(&mut session, &["spare"]);
    draft(&mut session).edit(&spare, DraftEdit::Remove)?;
    let team = node(&mut session, &["team"]);
    draft(&mut session).edit(&team, DraftEdit::Remove)?;
    assert!(matches!(
        problems(&session).as_slice(),
        [ProblemKind::Selection(ConfigurationError::UnknownTaxonomy(
            _
        ))]
    ));
    assert!(
        session
            .configure_global(id("conventional"), built_in_ids())
            .is_err()
    );

    draft(&mut session).select(id("conventional"), built_in_ids());
    assert!(built(&session).custom_taxonomies().is_empty());
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

    assert!(session.library_mut().is_none());
    let error = session
        .configure_global(id("research"), built_in_ids())
        .expect_err("a project session edited the global library");
    assert!(error.contains("Global configuration"), "{error}");
    assert_eq!(session.global(), Some(&GlobalConfiguration::default()));
    Ok(())
}

#[test]
fn fork_copies_a_built_in_under_a_new_identity_with_lineage() -> Result<(), Box<dyn Error>> {
    let mut session = ConfigurationSession::open_global(GlobalConfiguration::default());

    fork_as(&mut session, "research", "lab");

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
fn a_fork_takes_its_authored_content_and_refuses_its_source_name() -> Result<(), Box<dyn Error>> {
    let mut session = ConfigurationSession::open_global(GlobalConfiguration::default());
    let source = &built_in_taxonomies()[0];

    let fork = fork_as(&mut session, "conventional", "conventional");
    assert_eq!(
        problems(&session),
        [ProblemKind::Taken("conventional".into())]
    );

    draft(&mut session).edit(&fork, DraftEdit::Text(DraftField::Name, "team"))?;
    describe(&mut session, "team", "A team policy.");
    for commit_type in &source.commit_types()[2..] {
        let commit_type = node(&mut session, &["team", commit_type.id().as_str()]);
        draft(&mut session).edit(&commit_type, DraftEdit::Remove)?;
    }
    let fork = session
        .catalog()?
        .find(&id("team"))
        .cloned()
        .ok_or("fork is missing")?;
    assert_eq!(fork.version(), TaxonomyVersion::V1);
    assert_eq!(fork.description().as_str(), "A team policy.");
    assert_eq!(fork.commit_types(), &source.commit_types()[..2]);
    assert_eq!(
        fork.derived_from(),
        Some(&TaxonomyLineage::new(source.id().clone(), source.version()))
    );
    Ok(())
}

#[test]
fn fork_names_that_reserved_or_existing_taxonomies_use_are_problems() -> Result<(), Box<dyn Error>>
{
    let mut session = ConfigurationSession::open_global(library(vec![custom("team")])?);

    for target in ["conventional", "infra-ops", "team"] {
        let fork = fork_as(&mut session, "research", target);
        assert_eq!(problems(&session), [ProblemKind::Taken(target.into())]);
        draft(&mut session).edit(&fork, DraftEdit::Remove)?;
    }

    assert!(!session.is_dirty());
    Ok(())
}

#[test]
fn forks_of_custom_taxonomies_preserve_content_and_reset_the_new_version()
-> Result<(), Box<dyn Error>> {
    let original = custom("team");
    let revised = Taxonomy::new(
        original.id().clone(),
        TaxonomyVersion::new(2)?,
        Description::new("Second team policy.")?,
        original.derived_from().cloned(),
        built_in_taxonomies()[2].commit_types().to_vec(),
    )?;
    let mut session = ConfigurationSession::open_global(library(vec![revised.clone()])?);

    fork_as(&mut session, "team", "team-copy");

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
    let missing =
        LibraryDraft::from_configuration(library(vec![custom("missing")]).expect("valid"))
            .find_path(&["missing"])
            .expect("the other library has it");

    assert_eq!(
        draft(&mut session).fork(&missing),
        Err(EditRefusal::Missing)
    );
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
        fork_as(&mut session, "conventional", "team");
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
        fork_as(&mut session, "conventional", "team");
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
fn apply_refuses_while_the_library_has_problems() -> Result<(), Box<dyn Error>> {
    let original = library(vec![custom("team")])?;
    let store = FakeGlobalStore::new(original.clone());

    let error = edit_global(&store, |workspace| {
        let mut session = workspace.load(ConfigurationDestination::Global)?;
        describe(&mut session, "team", " ");
        workspace.save(&session).map(|_| ())
    })
    .expect_err("a Library with a problem was applied");

    assert!(error.contains("description is blank"), "{error}");
    assert_eq!(store.swaps.get(), 0);
    assert_eq!(*store.current.borrow(), original);
    Ok(())
}

#[test]
fn global_store_failures_are_reported_and_leave_the_library_unchanged() {
    let mut store = FakeGlobalStore::new(GlobalConfiguration::default());
    store.fail = true;

    let error = edit_global(&store, |workspace| {
        let mut session = workspace.load(ConfigurationDestination::Global)?;
        fork_as(&mut session, "conventional", "team");
        workspace.save(&session).map(|_| ())
    })
    .expect_err("a failed save was reported as success");

    assert_eq!(error, "global store failed");
    assert_eq!(*store.current.borrow(), GlobalConfiguration::default());
}

#[test]
fn recreating_an_original_identity_cannot_reset_its_version() -> Result<(), Box<dyn Error>> {
    let original = custom("team");
    let revised = at_version(&original, 2, original.description().as_ref());
    let mut session = ConfigurationSession::open_global(library(vec![revised])?);
    let team = node(&mut session, &["team"]);
    draft(&mut session).edit(&team, DraftEdit::Remove)?;

    let fork = fork_as(&mut session, "research", "team");
    let created = draft(&mut session).create_taxonomy();
    draft(&mut session).edit(&created, DraftEdit::Text(DraftField::Name, "team"))?;

    let staged = ProblemKind::StagedForDeletion("team".into());
    assert_eq!(
        problems(&session)
            .iter()
            .filter(|kind| **kind == staged)
            .count(),
        1,
        "{:?}",
        problems(&session)
    );
    draft(&mut session).edit(&created, DraftEdit::Remove)?;
    assert_eq!(problems(&session), [staged]);
    draft(&mut session).edit(&fork, DraftEdit::Remove)?;
    assert_eq!(session.catalog()?.find(&id("team")), None);
    Ok(())
}

#[test]
fn review_names_an_edited_taxonomy_and_what_changed() -> Result<(), Box<dyn Error>> {
    let original = custom("team");
    let mut session = ConfigurationSession::open_global(library(vec![original.clone()])?);

    describe(&mut session, "team", "Team policy.");

    assert_eq!(
        session.review()?.changes,
        "GLOBAL CONFIGURATION\n\n\
         Taxonomies\n  edited   team v1 -> v2: description\n\n\
         Default\n  conventional\n\n\
         Available\n  conventional, research, infra-ops"
    );
    Ok(())
}

#[test]
fn review_names_a_created_and_a_deleted_taxonomy_in_one_session() -> Result<(), Box<dyn Error>> {
    let mut session = ConfigurationSession::open_global(library(vec![custom("team")])?);

    let team = node(&mut session, &["team"]);
    let other = draft(&mut session).fork(&team)?;
    draft(&mut session).edit(&other, DraftEdit::Text(DraftField::Name, "other"))?;
    draft(&mut session).edit(&team, DraftEdit::Remove)?;

    let types = custom("other").commit_types().len();
    assert!(session.review()?.changes.contains(&format!(
        "Taxonomies\n  created  other v1, {types} types\n  deleted  team v1\n\n"
    )));
    Ok(())
}

#[test]
fn review_omits_taxonomies_when_only_the_selection_changed() -> Result<(), Box<dyn Error>> {
    let original = custom("team");
    let mut session = ConfigurationSession::open_global(library(vec![original.clone()])?);

    describe(&mut session, "team", "Team policy.");
    describe(&mut session, "team", original.description().as_str());
    session.configure_global(id("research"), built_in_ids())?;

    assert_eq!(
        session.review()?.changes,
        "GLOBAL CONFIGURATION\n\n\
         Default\n  conventional -> research\n\n\
         Available\n  conventional, research, infra-ops"
    );
    Ok(())
}
