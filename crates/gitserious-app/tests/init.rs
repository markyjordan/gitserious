use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use gitserious_app::{
    ConfigurationError, GlobalConfiguration, InitOutcome, InitStatus, InitializeProjectError,
    ProjectConfiguration, ProjectConfigurationStore, ProjectRevision, RepositoryLocator,
    RepositoryRoot, StoredProject, fingerprint_taxonomy, initial_project_configuration,
    initialize_project,
};
use gitserious_core::{
    CommitTypeDefinition, CommitTypeId, ConditionId, Description, PropertyCondition,
    PropertyDefinition, PropertyKey, PropertyRequirement, SchemaVersion, Taxonomy, TaxonomyId,
    TaxonomyLineage, TaxonomyVersion, built_in_taxonomies,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FakeError {
    Locate,
    Load,
    EnsureLocalState,
    Create,
    Replace,
}

impl Display for FakeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
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

    fn is_not_repository(_: &Self::Error) -> bool {
        false
    }

    fn locate(&self, _start: &Path) -> Result<RepositoryRoot, Self::Error> {
        self.calls.set(self.calls.get() + 1);
        self.result.clone()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum StoreCall {
    Load,
    EnsureLocalState,
    Create(ProjectConfiguration),
}

struct FakeStore {
    project: Option<ProjectConfiguration>,
    failure: Option<FakeError>,
    calls: RefCell<Vec<StoreCall>>,
}

impl FakeStore {
    fn new(project: Option<ProjectConfiguration>) -> Self {
        Self {
            project,
            failure: None,
            calls: RefCell::new(Vec::new()),
        }
    }

    fn failing(failure: FakeError) -> Self {
        Self {
            project: None,
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

impl ProjectConfigurationStore for FakeStore {
    type Error = FakeError;

    fn load(&self, _root: &RepositoryRoot) -> Result<Option<StoredProject>, Self::Error> {
        self.calls.borrow_mut().push(StoreCall::Load);
        self.maybe_fail(FakeError::Load)?;
        Ok(self.project.clone().map(|configuration| StoredProject {
            configuration,
            revision: ProjectRevision::of_bytes(b"existing"),
        }))
    }

    fn create(
        &self,
        _root: &RepositoryRoot,
        configuration: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error> {
        self.calls
            .borrow_mut()
            .push(StoreCall::Create(configuration.clone()));
        self.maybe_fail(FakeError::Create)?;
        Ok(ProjectRevision::of_bytes(b"created"))
    }

    fn replace(
        &self,
        _root: &RepositoryRoot,
        _expected: ProjectRevision,
        _configuration: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error> {
        Err(FakeError::Replace)
    }

    fn ensure_local_state(&self, _root: &RepositoryRoot) -> Result<(), Self::Error> {
        self.calls.borrow_mut().push(StoreCall::EnsureLocalState);
        self.maybe_fail(FakeError::EnsureLocalState)
    }

    fn is_invalid_document(_: &Self::Error) -> bool {
        false
    }
}

fn default_project() -> Result<ProjectConfiguration, Box<dyn Error>> {
    Ok(initial_project_configuration(
        &GlobalConfiguration::default(),
        None,
    )?)
}

fn init(
    locator: &FakeLocator,
    store: &FakeStore,
    global: &GlobalConfiguration,
    taxonomy: Option<&TaxonomyId>,
) -> Result<InitOutcome, InitializeProjectError<FakeError, FakeError>> {
    initialize_project(locator, store, global, taxonomy, &repository_path())
}

fn repository_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fake-repository")
}

fn assert_resolution(status: InitStatus, outcome: &InitOutcome) {
    assert_eq!(outcome.status(), status);
    assert_eq!(outcome.root().as_path(), repository_path());
    assert_eq!(outcome.taxonomy().as_str(), "conventional");
    assert_eq!(Ok(outcome.version()), TaxonomyVersion::new(1));
}

#[test]
fn absent_project_is_created_once_with_complete_definitions() -> Result<(), Box<dyn Error>> {
    let locator = FakeLocator::available()?;
    let store = FakeStore::new(None);

    let outcome = init(&locator, &store, &GlobalConfiguration::default(), None)?;

    assert_resolution(InitStatus::Initialized, &outcome);
    assert_eq!(locator.calls.get(), 1);
    assert_eq!(
        store.calls.borrow().as_slice(),
        [
            StoreCall::Load,
            StoreCall::EnsureLocalState,
            StoreCall::Create(default_project()?),
        ]
    );
    Ok(())
}

#[test]
fn existing_project_is_never_rewritten_even_when_the_library_changed() -> Result<(), Box<dyn Error>>
{
    let locator = FakeLocator::available()?;
    let store = FakeStore::new(Some(default_project()?));
    let research_only = GlobalConfiguration::new(
        1,
        TaxonomyId::new("research")?,
        vec![TaxonomyId::new("research")?],
        Vec::new(),
    )?;

    let outcome = init(
        &locator,
        &store,
        &research_only,
        Some(&TaxonomyId::new("research")?),
    )?;

    assert_resolution(InitStatus::AlreadyInitialized, &outcome);
    assert_eq!(
        store.calls.borrow().as_slice(),
        [StoreCall::Load, StoreCall::EnsureLocalState]
    );
    Ok(())
}

#[test]
fn unknown_requested_taxonomy_is_refused_without_a_write() -> Result<(), Box<dyn Error>> {
    let locator = FakeLocator::available()?;
    let store = FakeStore::new(None);

    let error = init(
        &locator,
        &store,
        &GlobalConfiguration::default(),
        Some(&TaxonomyId::new("custom")?),
    )
    .err();

    assert!(matches!(
        error,
        Some(InitializeProjectError::Configuration(
            ConfigurationError::UnknownTaxonomy(id)
        )) if id.as_str() == "custom"
    ));
    assert_eq!(store.calls.borrow().as_slice(), [StoreCall::Load]);
    Ok(())
}

#[test]
fn locator_and_each_store_failure_remain_distinguishable() -> Result<(), Box<dyn Error>> {
    let store = FakeStore::new(None);
    let locator_error = init(
        &FakeLocator::failing(),
        &store,
        &GlobalConfiguration::default(),
        None,
    );
    assert!(matches!(
        locator_error,
        Err(InitializeProjectError::Repository(FakeError::Locate))
    ));
    assert!(store.calls.borrow().is_empty());

    let locator = FakeLocator::available()?;
    for failure in [
        FakeError::Load,
        FakeError::EnsureLocalState,
        FakeError::Create,
    ] {
        let store = FakeStore::failing(failure);
        let error = init(&locator, &store, &GlobalConfiguration::default(), None);
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
fn initial_project_copies_the_default_environment_in_order() -> Result<(), Box<dyn Error>> {
    let first = default_project()?;

    assert_eq!(first, default_project()?);
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
fn initial_project_copies_available_custom_taxonomies() -> Result<(), Box<dyn Error>> {
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
        vec![fork.clone()].into_iter().map(Into::into).collect(),
    )?;

    let project = initial_project_configuration(&global, None)?;

    assert_eq!(project.taxonomies().len(), built_in_taxonomies().len() + 1);
    assert_eq!(project.taxonomies().last(), Some(&fork));
    Ok(())
}

#[test]
fn requested_default_outside_the_available_list_is_appended() -> Result<(), Box<dyn Error>> {
    let global = GlobalConfiguration::new(
        1,
        TaxonomyId::new("conventional")?,
        vec![TaxonomyId::new("conventional")?],
        Vec::new(),
    )?;

    let project = initial_project_configuration(&global, Some(&TaxonomyId::new("research")?))?;

    assert_eq!(project.default_taxonomy().as_str(), "research");
    assert_eq!(
        project
            .taxonomies()
            .iter()
            .map(|taxonomy| taxonomy.id().as_str())
            .collect::<Vec<_>>(),
        ["conventional", "research"]
    );
    Ok(())
}
