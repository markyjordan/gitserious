use std::cell::{Cell, RefCell};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use gitserious_app::{
    ConfigurationCatalog, ConfigurationCatalogError, CustomConfiguration, Fingerprint, InitStatus,
    InitializeProjectError, ProjectConfig, ProjectLock, ProjectState, ProjectStateStore,
    RepositoryLocator, RepositoryRoot, built_in_effective_catalog,
    fingerprint_commit_message_template, fingerprint_commit_type_definition,
    fingerprint_project_config, initialize_project, resolve_project_lock,
};
use gitserious_core::{
    CommitMessageTemplateDefinition, CommitTypeDefinition, CommitTypeId, ConditionId, Description,
    PropertyCondition, PropertyDefinition, PropertyKey, PropertyMultiplicity, PropertyRequirement,
    SchemaVersion, TemplateDefinition, TemplateId, TemplateVersion, built_in_configuration,
    default_commit_message_template,
};

fn catalog() -> Result<ConfigurationCatalog, ConfigurationCatalogError> {
    built_in_effective_catalog()
}

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

fn default_config_and_lock() -> Result<(ProjectConfig, ProjectLock), Box<dyn Error>> {
    let config = ProjectConfig::default_channel()?;
    let lock = resolve_project_lock(&config)?;
    Ok((config, lock))
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
        current.template_reference().clone(),
        current.resolved_template().clone(),
    )?)
}

fn assert_resolution(status: InitStatus, outcome: &gitserious_app::InitOutcome) {
    assert_eq!(outcome.status(), status);
    assert_eq!(outcome.root().as_path(), repository_path());
    assert_eq!(outcome.template_reference().as_str(), "default");
    assert_eq!(outcome.resolved_template().as_str(), "conventional");
    assert_eq!(outcome.resolved_version(), TemplateVersion::V1);
}

#[test]
fn absent_state_creates_config_and_lock_once() -> Result<(), Box<dyn Error>> {
    let locator = FakeLocator::available()?;
    let store = FakeStore::new(ProjectState::Absent);
    let (expected_config, expected_lock) = default_config_and_lock()?;

    let outcome = initialize_project(
        &locator,
        &store,
        &catalog()?,
        None,
        &repository_path().join("subdir"),
    )?;

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

    let outcome = initialize_project(&locator, &store, &catalog()?, None, &repository_path())?;

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

    let outcome = initialize_project(&locator, &store, &catalog()?, None, &repository_path())?;

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

    let outcome = initialize_project(&locator, &store, &catalog()?, None, &repository_path())?;

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

    let error = initialize_project(&locator, &store, &catalog()?, None, &repository_path()).err();

    assert!(matches!(error, Some(InitializeProjectError::OrphanLock)));
    assert_eq!(store.calls.borrow().as_slice(), [StoreCall::Inspect]);
    Ok(())
}

#[test]
fn unknown_authored_template_is_refused_without_a_write() -> Result<(), Box<dyn Error>> {
    let locator = FakeLocator::available()?;
    let config = ProjectConfig::new(
        1,
        TemplateId::new("custom")?,
        CustomConfiguration::default(),
    )?;
    let store = FakeStore::new(ProjectState::ConfigOnly(config));

    let error = initialize_project(&locator, &store, &catalog()?, None, &repository_path()).err();

    assert!(matches!(error, Some(InitializeProjectError::Policy(_))));
    assert_eq!(store.calls.borrow().as_slice(), [StoreCall::Inspect]);
    Ok(())
}

#[test]
fn locator_and_each_store_failure_remain_distinguishable() -> Result<(), Box<dyn Error>> {
    let store = FakeStore::new(ProjectState::Absent);
    let locator_error = initialize_project(
        &FakeLocator::failing(),
        &store,
        &catalog()?,
        None,
        &repository_path(),
    );
    assert!(matches!(
        locator_error,
        Err(InitializeProjectError::Repository(FakeError::Locate))
    ));
    assert!(store.calls.borrow().is_empty());

    let locator = FakeLocator::available()?;
    for (state, failure) in [
        (ProjectState::Absent, FakeError::Inspect),
        (ProjectState::Absent, FakeError::EnsureLocalState),
        (ProjectState::Absent, FakeError::Initialize),
        (
            ProjectState::ConfigOnly(ProjectConfig::default_channel()?),
            FakeError::CreateLock,
        ),
        (
            ProjectState::Initialized {
                config: default_config_and_lock()?.0,
                lock: stale_lock(&default_config_and_lock()?.1)?,
            },
            FakeError::ReplaceLock,
        ),
    ] {
        let store = FakeStore::failing(state, failure);
        let error = initialize_project(&locator, &store, &catalog()?, None, &repository_path());
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
    multiplicity: PropertyMultiplicity,
) -> Result<PropertyDefinition, Box<dyn Error>> {
    Ok(PropertyDefinition::new(
        PropertyKey::new(key)?,
        description,
        requirement,
        multiplicity,
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

fn fingerprint_identity_variants() -> Result<Vec<CommitTypeDefinition>, Box<dyn Error>> {
    let required = property(
        "intent",
        "Why.",
        PropertyRequirement::Required,
        PropertyMultiplicity::Single,
    )?;
    let second = property(
        "behavior",
        "What.",
        PropertyRequirement::Recommended,
        PropertyMultiplicity::Single,
    )?;
    Ok(vec![
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
        definition(
            1,
            "feat",
            "Feature.",
            vec![second.clone(), required.clone()],
        )?,
    ])
}

fn fingerprint_property_variants() -> Result<Vec<CommitTypeDefinition>, Box<dyn Error>> {
    let variants = [
        (
            "intent",
            "Why.",
            PropertyRequirement::Required,
            PropertyMultiplicity::Single,
        ),
        (
            "purpose",
            "Why.",
            PropertyRequirement::Required,
            PropertyMultiplicity::Single,
        ),
        (
            "intent",
            "Changed.",
            PropertyRequirement::Required,
            PropertyMultiplicity::Single,
        ),
        (
            "intent",
            "Why.",
            PropertyRequirement::Optional,
            PropertyMultiplicity::Single,
        ),
        (
            "intent",
            "Why.",
            PropertyRequirement::Required,
            PropertyMultiplicity::Multiple,
        ),
    ];
    variants
        .into_iter()
        .map(|(key, description, requirement, multiplicity)| {
            definition(
                1,
                "feat",
                "Feature.",
                vec![property(key, description, requirement, multiplicity)?],
            )
        })
        .collect()
}

fn fingerprint_condition_variants() -> Result<Vec<CommitTypeDefinition>, Box<dyn Error>> {
    [
        ("known-cost", "When known."),
        ("different-condition", "When known."),
        ("known-cost", "A different rationale."),
    ]
    .into_iter()
    .map(|(id, rationale)| {
        let condition = PropertyCondition::new(ConditionId::new(id)?, rationale)?;
        definition(
            1,
            "feat",
            "Feature.",
            vec![property(
                "intent",
                "Why.",
                PropertyRequirement::Conditional(condition),
                PropertyMultiplicity::Single,
            )?],
        )
    })
    .collect()
}

#[test]
fn definition_fingerprint_changes_for_every_semantic_dimension() -> Result<(), Box<dyn Error>> {
    let mut definitions = fingerprint_identity_variants()?;
    definitions.extend(fingerprint_property_variants()?);
    definitions.extend(fingerprint_condition_variants()?);
    let fingerprints = definitions
        .iter()
        .map(fingerprint_commit_type_definition)
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(fingerprints.len(), definitions.len());
    Ok(())
}

#[test]
fn config_and_template_fingerprints_are_stable_and_order_sensitive() -> Result<(), Box<dyn Error>> {
    let (config, lock) = default_config_and_lock()?;
    assert_eq!(
        fingerprint_project_config(&config).to_string(),
        "sha256:e42b105788a902758375a4fa48d875a753a7954c654e8bc8bfc13456d1e95f98"
    );
    assert_eq!(
        fingerprint_commit_message_template(default_commit_message_template()).to_string(),
        lock.resolved_template().fingerprint().to_string()
    );
    let custom_config = ProjectConfig::new(
        1,
        TemplateId::new("custom")?,
        CustomConfiguration::default(),
    )?;
    assert_ne!(
        fingerprint_project_config(&config),
        fingerprint_project_config(&custom_config)
    );

    let original = default_commit_message_template();
    let mut reversed = original.commit_types().to_vec();
    reversed.reverse();
    let reordered = CommitMessageTemplateDefinition::new(
        original.version(),
        original.id().clone(),
        original.description(),
        reversed,
    )?;
    assert_ne!(
        fingerprint_commit_message_template(original),
        fingerprint_commit_message_template(&reordered)
    );
    let renamed = CommitMessageTemplateDefinition::new(
        original.version(),
        TemplateId::new("renamed")?,
        original.description(),
        original.commit_types().to_vec(),
    )?;
    let versioned = CommitMessageTemplateDefinition::new(
        TemplateVersion::new(2)?,
        original.id().clone(),
        original.description(),
        original.commit_types().to_vec(),
    )?;
    assert_ne!(
        fingerprint_commit_message_template(original),
        fingerprint_commit_message_template(&renamed)
    );
    assert_ne!(
        fingerprint_commit_message_template(original),
        fingerprint_commit_message_template(&versioned)
    );
    Ok(())
}

#[test]
fn project_fingerprint_covers_inactive_custom_definitions() -> Result<(), Box<dyn Error>> {
    let built_in = built_in_configuration();
    let template = |description: &str| -> Result<TemplateDefinition, Box<dyn Error>> {
        Ok(TemplateDefinition::new(
            TemplateId::new("inactive")?,
            TemplateVersion::V1,
            Description::new(description)?,
            built_in.taxonomy().id().clone(),
            built_in.typeset().id().clone(),
        ))
    };
    let config = |description: &str| -> Result<ProjectConfig, Box<dyn Error>> {
        Ok(ProjectConfig::new(
            1,
            built_in.template().id().clone(),
            CustomConfiguration::new(vec![], vec![], vec![template(description)?])?,
        )?)
    };

    assert_ne!(
        fingerprint_project_config(&config("First inactive description.")?),
        fingerprint_project_config(&config("Changed inactive description.")?)
    );
    Ok(())
}

#[test]
fn resolved_default_lock_is_exact_and_deterministic() -> Result<(), Box<dyn Error>> {
    let (config, first) = default_config_and_lock()?;
    let second = resolve_project_lock(&config)?;

    assert_eq!(first, second);
    assert_eq!(first.version(), 1);
    assert_eq!(first.template_reference().as_str(), "default");
    assert_eq!(first.resolved_template().id().as_str(), "conventional");
    assert_eq!(first.resolved_template().version(), TemplateVersion::V1);
    assert_eq!(first.resolved_template().commit_types().len(), 11);
    assert_eq!(
        first.resolved_templates().len(),
        built_in_configuration().templates().len()
    );
    assert!(
        first
            .resolved_templates()
            .iter()
            .any(|template| template.id().as_str() == "ml-research")
    );
    assert!(
        first
            .resolved_templates()
            .iter()
            .any(|template| template.id().as_str() == "infra-ops")
    );
    assert_eq!(
        first
            .resolved_template()
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
fn custom_project_templates_are_all_recorded_in_the_lock() -> Result<(), Box<dyn Error>> {
    let custom = CustomConfiguration::new(
        Vec::new(),
        Vec::new(),
        vec![TemplateDefinition::new(
            TemplateId::new("inactive")?,
            TemplateVersion::V1,
            Description::new("An inactive template.")?,
            built_in_configuration().taxonomy().id().clone(),
            built_in_configuration().typeset().id().clone(),
        )],
    )?;
    let config = ProjectConfig::new(1, TemplateId::new("default")?, custom)?;
    let lock = resolve_project_lock(&config)?;
    assert_eq!(
        lock.resolved_templates().len(),
        built_in_configuration().templates().len() + 1
    );
    assert!(
        lock.resolved_templates()
            .iter()
            .any(|template| template.id().as_str() == "inactive")
    );
    Ok(())
}
