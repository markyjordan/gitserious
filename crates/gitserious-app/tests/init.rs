use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use gitserious_app::{
    Fingerprint, GlobalConfiguration, InitOutcome, InitStatus, InitializeProjectError,
    ProjectConfig, ProjectLock, ProjectState, ProjectStateStore, RepositoryLocator, RepositoryRoot,
    fingerprint_taxonomy, initialize_project, resolve_effective_configuration,
};
use gitserious_core::{
    CommitTypeDefinition, CommitTypeId, ConditionId, Description, PropertyCondition,
    PropertyDefinition, PropertyKey, PropertyRequirement, SchemaVersion, Taxonomy, TaxonomyId,
    TaxonomyLineage, TaxonomyVersion, built_in_taxonomies,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FakeError {
    Locate,
    Inspect,
    EnsureLocalState,
    Initialize,
    CreateLock,
    ReplaceLock,
}

impl Display for FakeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "fake {self:?} failure")
    }
}

impl Error for FakeError {}

struct FakeLocator {
    result: Result<RepositoryRoot, FakeError>,
    calls: Cell<usize>,
}

impl FakeLocator {
    fn available() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            result: Ok(RepositoryRoot::new(repository_path())?),
            calls: Cell::new(0),
        })
    }

    fn failing() -> Self {
        Self {
            result: Err(FakeError::Locate),
            calls: Cell::new(0),
        }
    }
}

impl RepositoryLocator for FakeLocator {
    type Error = FakeError;

    fn locate(&self, _start: &Path) -> Result<RepositoryRoot, Self::Error> {
        self.calls.set(self.calls.get() + 1);
        self.result.clone()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum StoreCall {
    Inspect,
    EnsureLocalState,
    Initialize(ProjectConfig, ProjectLock),
    CreateLock(ProjectLock),
    ReplaceLock(ProjectLock, ProjectLock),
}

struct FakeStore {
    state: ProjectState,
    failure: Option<FakeError>,
    calls: RefCell<Vec<StoreCall>>,
}

impl FakeStore {
    fn new(state: ProjectState) -> Self {
        Self {
            state,
            failure: None,
            calls: RefCell::new(Vec::new()),
        }
    }

    fn failing(state: ProjectState, failure: FakeError) -> Self {
        Self {
            state,
            failure: Some(failure),
            calls: RefCell::new(Vec::new()),
        }
    }

    fn maybe_fail(&self, operation: FakeError) -> Result<(), FakeError> {
        if self.failure == Some(operation) {
            Err(operation)
        } else {
            Ok(())
        }
    }
}

impl ProjectStateStore for FakeStore {
    type Error = FakeError;

    fn inspect(&self, _root: &RepositoryRoot) -> Result<ProjectState, Self::Error> {
        self.calls.borrow_mut().push(StoreCall::Inspect);
        self.maybe_fail(FakeError::Inspect)?;
        Ok(self.state.clone())
    }

    fn ensure_local_state(&self, _root: &RepositoryRoot) -> Result<(), Self::Error> {
        self.calls.borrow_mut().push(StoreCall::EnsureLocalState);
        self.maybe_fail(FakeError::EnsureLocalState)
    }

    fn initialize(
        &self,
        _root: &RepositoryRoot,
        config: &ProjectConfig,
        lock: &ProjectLock,
    ) -> Result<(), Self::Error> {
        self.calls
            .borrow_mut()
            .push(StoreCall::Initialize(config.clone(), lock.clone()));
        self.maybe_fail(FakeError::Initialize)
    }

    fn create_lock(&self, _root: &RepositoryRoot, lock: &ProjectLock) -> Result<(), Self::Error> {
        self.calls
            .borrow_mut()
            .push(StoreCall::CreateLock(lock.clone()));
        self.maybe_fail(FakeError::CreateLock)
    }

    fn replace_lock(
        &self,
        _root: &RepositoryRoot,
        current: &ProjectLock,
        replacement: &ProjectLock,
    ) -> Result<(), Self::Error> {
        self.calls
            .borrow_mut()
            .push(StoreCall::ReplaceLock(current.clone(), replacement.clone()));
        self.maybe_fail(FakeError::ReplaceLock)
    }

    fn compare_and_swap(
        &self,
        _root: &RepositoryRoot,
        _current_config: &ProjectConfig,
        _current_lock: &ProjectLock,
        _replacement_config: &ProjectConfig,
        _replacement_lock: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Err(FakeError::ReplaceLock)
    }
}

fn lock_for(
    global: &GlobalConfiguration,
    config: &ProjectConfig,
) -> Result<ProjectLock, Box<dyn Error>> {
    let effective = resolve_effective_configuration(global, config)?;
    Ok(ProjectLock::capture(config, &effective)?)
}

fn default_config_and_lock() -> Result<(ProjectConfig, ProjectLock), Box<dyn Error>> {
    let config = ProjectConfig::inherited();
    let lock = lock_for(&GlobalConfiguration::default(), &config)?;
    Ok((config, lock))
}

fn init<S>(
    locator: &FakeLocator,
    store: &S,
    start: &Path,
) -> Result<InitOutcome, InitializeProjectError<FakeError, FakeError>>
where
    S: ProjectStateStore<Error = FakeError>,
{
    initialize_project(locator, store, &GlobalConfiguration::default(), None, start)
}

fn repository_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fake-repository")
}

fn stale_lock(current: &ProjectLock) -> Result<ProjectLock, Box<dyn Error>> {
    let fingerprint = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        .parse::<Fingerprint>()?;
    Ok(ProjectLock::new(
        current.version(),
        fingerprint,
        current.default_taxonomy().clone(),
        current.taxonomies().to_vec(),
    )?)
}

fn assert_resolution(status: InitStatus, outcome: &InitOutcome) {
    assert_eq!(outcome.status(), status);
    assert_eq!(outcome.root().as_path(), repository_path());
    assert_eq!(outcome.taxonomy().as_str(), "conventional");
    assert_eq!(Ok(outcome.version()), TaxonomyVersion::new(2));
}

#[test]
fn absent_state_creates_config_and_lock_once() -> Result<(), Box<dyn Error>> {
    let locator = FakeLocator::available()?;
    let store = FakeStore::new(ProjectState::Absent);
    let (expected_config, expected_lock) = default_config_and_lock()?;

    let outcome = init(&locator, &store, &repository_path().join("subdir"))?;

    assert_resolution(InitStatus::Initialized, &outcome);
    assert_eq!(locator.calls.get(), 1);
    assert_eq!(
        store.calls.borrow().as_slice(),
        [
            StoreCall::Inspect,
            StoreCall::EnsureLocalState,
            StoreCall::Initialize(expected_config, expected_lock),
        ]
    );
    Ok(())
}

#[test]
fn config_only_state_creates_only_the_missing_lock() -> Result<(), Box<dyn Error>> {
    let locator = FakeLocator::available()?;
    let (config, expected_lock) = default_config_and_lock()?;
    let store = FakeStore::new(ProjectState::ConfigOnly(config));

    let outcome = init(&locator, &store, &repository_path())?;

    assert_resolution(InitStatus::LockCreated, &outcome);
    assert_eq!(
        store.calls.borrow().as_slice(),
        [
            StoreCall::Inspect,
            StoreCall::EnsureLocalState,
            StoreCall::CreateLock(expected_lock)
        ]
    );
    Ok(())
}

#[test]
fn matching_initialized_state_ensures_only_local_state() -> Result<(), Box<dyn Error>> {
    let locator = FakeLocator::available()?;
    let (config, lock) = default_config_and_lock()?;
    let store = FakeStore::new(ProjectState::Initialized { config, lock });

    let outcome = init(&locator, &store, &repository_path())?;

    assert_resolution(InitStatus::AlreadyInitialized, &outcome);
    assert_eq!(
        store.calls.borrow().as_slice(),
        [StoreCall::Inspect, StoreCall::EnsureLocalState]
    );
    Ok(())
}

#[test]
fn stale_initialized_state_replaces_only_the_observed_lock() -> Result<(), Box<dyn Error>> {
    let locator = FakeLocator::available()?;
    let (config, expected) = default_config_and_lock()?;
    let stale = stale_lock(&expected)?;
    let store = FakeStore::new(ProjectState::Initialized {
        config,
        lock: stale.clone(),
    });

    let outcome = init(&locator, &store, &repository_path())?;

    assert_resolution(InitStatus::LockRefreshed, &outcome);
    assert_eq!(
        store.calls.borrow().as_slice(),
        [
            StoreCall::Inspect,
            StoreCall::EnsureLocalState,
            StoreCall::ReplaceLock(stale, expected),
        ]
    );
    Ok(())
}

#[test]
fn orphan_lock_is_refused_without_a_write() -> Result<(), Box<dyn Error>> {
    let locator = FakeLocator::available()?;
    let store = FakeStore::new(ProjectState::LockOnly);

    let error = init(&locator, &store, &repository_path()).err();

    assert!(matches!(error, Some(InitializeProjectError::OrphanLock)));
    assert_eq!(store.calls.borrow().as_slice(), [StoreCall::Inspect]);
    Ok(())
}

#[test]
fn unknown_authored_taxonomy_is_refused_without_a_write() -> Result<(), Box<dyn Error>> {
    let locator = FakeLocator::available()?;
    let config = ProjectConfig::new(1, Some(TaxonomyId::new("custom")?), None)?;
    let store = FakeStore::new(ProjectState::ConfigOnly(config));

    let error = init(&locator, &store, &repository_path()).err();

    assert!(matches!(
        error,
        Some(InitializeProjectError::Configuration(_))
    ));
    assert_eq!(store.calls.borrow().as_slice(), [StoreCall::Inspect]);
    Ok(())
}

#[test]
fn locator_and_each_store_failure_remain_distinguishable() -> Result<(), Box<dyn Error>> {
    let store = FakeStore::new(ProjectState::Absent);
    let locator_error = init(&FakeLocator::failing(), &store, &repository_path());
    assert!(matches!(
        locator_error,
        Err(InitializeProjectError::Repository(FakeError::Locate))
    ));
    assert!(store.calls.borrow().is_empty());

    let locator = FakeLocator::available()?;
    let (config, lock) = default_config_and_lock()?;
    for (state, failure) in [
        (ProjectState::Absent, FakeError::Inspect),
        (ProjectState::Absent, FakeError::EnsureLocalState),
        (ProjectState::Absent, FakeError::Initialize),
        (
            ProjectState::ConfigOnly(config.clone()),
            FakeError::CreateLock,
        ),
        (
            ProjectState::Initialized {
                config: config.clone(),
                lock: stale_lock(&lock)?,
            },
            FakeError::ReplaceLock,
        ),
    ] {
        let store = FakeStore::failing(state, failure);
        let error = init(&locator, &store, &repository_path());
        assert!(matches!(
            error,
            Err(InitializeProjectError::Store(actual)) if actual == failure
        ));
    }
    Ok(())
}

fn property(
    key: &str,
    description: &str,
    requirement: PropertyRequirement,
) -> Result<PropertyDefinition, Box<dyn Error>> {
    Ok(PropertyDefinition::new(
        PropertyKey::new(key)?,
        description,
        requirement,
    )?)
}

fn definition(
    version: u16,
    id: &str,
    description: &str,
    properties: Vec<PropertyDefinition>,
) -> Result<CommitTypeDefinition, Box<dyn Error>> {
    Ok(CommitTypeDefinition::new(
        SchemaVersion::new(version)?,
        CommitTypeId::new(id)?,
        description,
        properties,
    )?)
}

fn taxonomy(commit_types: Vec<CommitTypeDefinition>) -> Result<Taxonomy, Box<dyn Error>> {
    Ok(Taxonomy::new(
        TaxonomyId::new("team")?,
        TaxonomyVersion::V1,
        Description::new("Team policy.")?,
        None,
        commit_types,
    )?)
}

fn commit_type_variants() -> Result<Vec<CommitTypeDefinition>, Box<dyn Error>> {
    let required = property("intent", "Why.", PropertyRequirement::Required)?;
    let second = property("behavior", "What.", PropertyRequirement::Recommended)?;
    let mut variants = vec![
        definition(
            1,
            "feat",
            "Feature.",
            vec![required.clone(), second.clone()],
        )?,
        definition(
            2,
            "feat",
            "Feature.",
            vec![required.clone(), second.clone()],
        )?,
        definition(1, "fix", "Feature.", vec![required.clone(), second.clone()])?,
        definition(
            1,
            "feat",
            "Changed.",
            vec![required.clone(), second.clone()],
        )?,
        definition(1, "feat", "Feature.", vec![second, required])?,
    ];
    for (key, description, requirement) in [
        ("intent", "Why.", PropertyRequirement::Required),
        ("purpose", "Why.", PropertyRequirement::Required),
        ("intent", "Changed.", PropertyRequirement::Required),
        ("intent", "Why.", PropertyRequirement::Optional),
    ] {
        variants.push(definition(
            1,
            "feat",
            "Feature.",
            vec![property(key, description, requirement)?],
        )?);
    }
    for (id, rationale) in [
        ("known-cost", "When known."),
        ("different-condition", "When known."),
        ("known-cost", "A different rationale."),
    ] {
        let condition = PropertyCondition::new(ConditionId::new(id)?, rationale)?;
        variants.push(definition(
            1,
            "feat",
            "Feature.",
            vec![property(
                "intent",
                "Why.",
                PropertyRequirement::Conditional(condition),
            )?],
        )?);
    }
    Ok(variants)
}

#[test]
fn taxonomy_fingerprint_changes_for_every_semantic_dimension() -> Result<(), Box<dyn Error>> {
    let variants = commit_type_variants()?;
    let mut taxonomies = variants
        .iter()
        .map(|definition| taxonomy(vec![definition.clone()]))
        .collect::<Result<Vec<_>, _>>()?;
    let base = taxonomy(vec![variants[0].clone()])?;
    let renamed = Taxonomy::new(
        TaxonomyId::new("renamed")?,
        base.version(),
        base.description().clone(),
        None,
        base.commit_types().to_vec(),
    )?;
    let versioned = Taxonomy::new(
        base.id().clone(),
        TaxonomyVersion::new(2)?,
        base.description().clone(),
        None,
        base.commit_types().to_vec(),
    )?;
    let described = Taxonomy::new(
        base.id().clone(),
        base.version(),
        Description::new("Changed policy.")?,
        None,
        base.commit_types().to_vec(),
    )?;
    let derived = Taxonomy::new(
        base.id().clone(),
        base.version(),
        base.description().clone(),
        Some(TaxonomyLineage::new(
            TaxonomyId::new("conventional")?,
            TaxonomyVersion::V1,
        )),
        base.commit_types().to_vec(),
    )?;
    let pair = taxonomy(vec![variants[0].clone(), variants[2].clone()])?;
    let reversed = taxonomy(vec![variants[2].clone(), variants[0].clone()])?;
    taxonomies.extend([renamed, versioned, described, derived, pair, reversed]);

    let fingerprints = taxonomies
        .iter()
        .map(fingerprint_taxonomy)
        .collect::<BTreeSet<_>>();

    assert_eq!(fingerprints.len(), taxonomies.len());
    assert_eq!(
        fingerprint_taxonomy(&base),
        fingerprint_taxonomy(&taxonomy(vec![variants[0].clone()])?)
    );
    Ok(())
}

#[test]
fn captured_default_lock_is_exact_and_deterministic() -> Result<(), Box<dyn Error>> {
    let (config, first) = default_config_and_lock()?;
    let second = lock_for(&GlobalConfiguration::default(), &config)?;

    assert_eq!(first, second);
    assert_eq!(first.version(), 1);
    assert!(first.matches(&config));
    assert_eq!(first.default_taxonomy().as_str(), "conventional");
    assert_eq!(first.taxonomies(), built_in_taxonomies());
    assert_eq!(
        first.taxonomies()[0]
            .commit_types()
            .iter()
            .map(|definition| definition.id().as_str())
            .collect::<Vec<_>>(),
        [
            "feat", "fix", "refactor", "perf", "test", "docs", "chore", "build", "ci", "style",
            "revert",
        ]
    );
    Ok(())
}

#[test]
fn available_custom_taxonomies_are_recorded_in_the_lock() -> Result<(), Box<dyn Error>> {
    let team = TaxonomyId::new("team")?;
    let mut available = GlobalConfiguration::default()
        .available_taxonomies()
        .to_vec();
    available.push(team.clone());
    let source = &built_in_taxonomies()[0];
    let fork = Taxonomy::new(
        team.clone(),
        TaxonomyVersion::V1,
        source.description().clone(),
        Some(TaxonomyLineage::new(source.id().clone(), source.version())),
        source.commit_types().to_vec(),
    )?;
    let global = GlobalConfiguration::new(
        1,
        TaxonomyId::new("conventional")?,
        available,
        vec![fork.clone()],
    )?;

    let lock = lock_for(&global, &ProjectConfig::inherited())?;

    assert_eq!(lock.taxonomies().len(), built_in_taxonomies().len() + 1);
    assert_eq!(lock.taxonomies().last(), Some(&fork));
    Ok(())
}
