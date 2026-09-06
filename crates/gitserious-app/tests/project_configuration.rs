use std::cell::{Cell, RefCell};
#[path = "support/configuration_session.rs"]
mod editing_session;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use gitserious_app::{
    ConfigurationCatalog, ConfigurationEdit, CustomConfiguration, ProjectConfig,
    ProjectConfigurationEdit, ProjectConfigurationError, ProjectLock, ProjectState,
    ProjectStateStore, RepositoryLocator, RepositoryRoot, apply_custom_configuration_edits,
    apply_project_configuration_edits, fork_conventional_edits, fork_project_configuration,
    import_and_select_project_template, import_project_template, resolve_project_lock,
    select_project_template,
};
use gitserious_core::{Description, TaxonomyDefinition, TaxonomyVersion, TemplateId, TypesetId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FakeError(&'static str);

impl Display for FakeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for FakeError {}

fn repository_root() -> Result<RepositoryRoot, Box<dyn Error>> {
    Ok(RepositoryRoot::new(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fake-repository"),
    )?)
}

#[derive(Clone)]
struct FakeLocator;

impl RepositoryLocator for FakeLocator {
    type Error = FakeError;

    fn locate(&self, _start: &Path) -> Result<RepositoryRoot, Self::Error> {
        repository_root().map_err(|_| FakeError("invalid test root"))
    }
}

struct FakeStore {
    state: RefCell<ProjectState>,
    replacements: Cell<usize>,
    fail_replace: Cell<bool>,
}

impl FakeStore {
    fn new(state: ProjectState) -> Self {
        Self {
            state: RefCell::new(state),
            replacements: Cell::new(0),
            fail_replace: Cell::new(false),
        }
    }
}

impl ProjectStateStore for FakeStore {
    type Error = FakeError;

    fn inspect(&self, _root: &RepositoryRoot) -> Result<ProjectState, Self::Error> {
        Ok(self.state.borrow().clone())
    }

    fn ensure_local_state(&self, _root: &RepositoryRoot) -> Result<(), Self::Error> {
        Err(FakeError("unexpected local-state creation"))
    }

    fn initialize(
        &self,
        _root: &RepositoryRoot,
        _config: &ProjectConfig,
        _lock: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Err(FakeError("unexpected initialization"))
    }

    fn create_lock(&self, _root: &RepositoryRoot, _lock: &ProjectLock) -> Result<(), Self::Error> {
        Err(FakeError("unexpected lock creation"))
    }

    fn replace_lock(
        &self,
        _root: &RepositoryRoot,
        _current: &ProjectLock,
        _replacement: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Err(FakeError("unexpected lock replacement"))
    }

    fn compare_and_swap(
        &self,
        _root: &RepositoryRoot,
        current_config: &ProjectConfig,
        current_lock: &ProjectLock,
        replacement_config: &ProjectConfig,
        replacement_lock: &ProjectLock,
    ) -> Result<(), Self::Error> {
        if self.fail_replace.get() {
            return Err(FakeError("replace failed"));
        }
        let mut state = self.state.borrow_mut();
        if *state
            != (ProjectState::Initialized {
                config: current_config.clone(),
                lock: current_lock.clone(),
            })
        {
            return Err(FakeError("concurrent change"));
        }
        *state = ProjectState::Initialized {
            config: replacement_config.clone(),
            lock: replacement_lock.clone(),
        };
        self.replacements.set(self.replacements.get() + 1);
        Ok(())
    }
}

fn initialized(custom: CustomConfiguration, active: &str) -> Result<ProjectState, Box<dyn Error>> {
    let config = ProjectConfig::new(1, TemplateId::new(active)?, custom)?;
    let lock = resolve_project_lock(&config)?;
    Ok(ProjectState::Initialized { config, lock })
}

fn source_configuration() -> Result<CustomConfiguration, Box<dyn Error>> {
    Ok(apply_custom_configuration_edits::<FakeError>(
        &CustomConfiguration::default(),
        fork_conventional_edits(
            &TemplateId::new("platform")?,
            &gitserious_core::TaxonomyId::new("platform-taxonomy")?,
            &TypesetId::new("platform-typeset")?,
        ),
    )?)
}

fn source_catalog() -> Result<ConfigurationCatalog, Box<dyn Error>> {
    Ok(ConfigurationCatalog::new(&source_configuration()?)?)
}

#[test]
fn project_mutations_require_complete_initialized_state() -> Result<(), Box<dyn Error>> {
    let default = ProjectConfig::default_channel()?;
    let lock = resolve_project_lock(&default)?;
    let cases = [
        (
            ProjectState::Absent,
            "gitserious is not initialized; run `gitserious init` before configuring the project",
        ),
        (
            ProjectState::ConfigOnly(default),
            "gitserious.lock is missing; run `gitserious init` before configuring the project",
        ),
        (
            ProjectState::LockOnly,
            "gitserious.lock exists without gitserious.toml; restore or remove the orphan lock",
        ),
    ];
    for (state, expected) in cases {
        let store = FakeStore::new(state);
        let error = select_project_template(
            &FakeLocator,
            &store,
            Path::new("."),
            TemplateId::new("default")?,
        )
        .err()
        .ok_or("expected state error")?;
        assert_eq!(error.to_string(), expected);
        assert_eq!(store.replacements.get(), 0);
    }

    let stale = ProjectLock::new(
        lock.version(),
        format!("sha256:{}", "09".repeat(32)).parse()?,
        lock.template_reference().clone(),
        lock.resolved_template().clone(),
    )?;
    let store = FakeStore::new(ProjectState::Initialized {
        config: ProjectConfig::default_channel()?,
        lock: stale,
    });
    assert!(matches!(
        select_project_template(
            &FakeLocator,
            &store,
            Path::new("."),
            TemplateId::new("default")?
        ),
        Err(ProjectConfigurationError::StaleLock)
    ));
    Ok(())
}

#[test]
fn fork_and_selection_are_independent_atomic_operations() -> Result<(), Box<dyn Error>> {
    let store = FakeStore::new(initialized(CustomConfiguration::default(), "default")?);
    let template = TemplateId::new("platform")?;
    let taxonomy = gitserious_core::TaxonomyId::new("platform-taxonomy")?;
    let typeset = TypesetId::new("platform-typeset")?;

    let forked = fork_project_configuration(
        &FakeLocator,
        &store,
        Path::new("."),
        &template,
        &taxonomy,
        &typeset,
    )?;
    assert_eq!(forked.config().active_template().as_str(), "default");
    assert_eq!(forked.config().custom().taxonomies().len(), 1);
    assert_eq!(forked.config().custom().typesets().len(), 1);
    assert_eq!(forked.config().custom().templates().len(), 1);

    let selected = select_project_template(&FakeLocator, &store, Path::new("."), template.clone())?;
    assert_eq!(selected.config().active_template(), &template);
    assert_eq!(store.replacements.get(), 2);

    assert!(matches!(
        apply_project_configuration_edits(
            &FakeLocator,
            &store,
            Path::new("."),
            [ProjectConfigurationEdit::Custom(
                ConfigurationEdit::DeleteTemplate(template.clone())
            )]
        ),
        Err(ProjectConfigurationError::ActiveTemplateDeletion(id)) if id == template
    ));

    let removed = apply_project_configuration_edits(
        &FakeLocator,
        &store,
        Path::new("."),
        [
            ProjectConfigurationEdit::SelectTemplate(TemplateId::new("default")?),
            ProjectConfigurationEdit::Custom(ConfigurationEdit::DeleteTemplate(template)),
        ],
    )?;
    assert!(removed.config().custom().templates().is_empty());
    Ok(())
}

#[test]
fn global_import_is_idempotent_independent_and_optionally_selected() -> Result<(), Box<dyn Error>> {
    let source = source_catalog()?;
    let selected = TemplateId::new("platform")?;
    let store = FakeStore::new(initialized(CustomConfiguration::default(), "default")?);

    let imported =
        import_project_template(&FakeLocator, &store, &source, Path::new("."), &selected)?;
    assert_eq!(imported.config().active_template().as_str(), "default");
    assert_eq!(imported.config().custom(), &source_configuration()?);
    assert_eq!(store.replacements.get(), 1);

    import_project_template(&FakeLocator, &store, &source, Path::new("."), &selected)?;
    assert_eq!(store.replacements.get(), 1, "exact import must be a no-op");

    let activated = import_and_select_project_template(
        &FakeLocator,
        &store,
        &source,
        Path::new("."),
        &selected,
    )?;
    assert_eq!(activated.config().active_template(), &selected);
    assert_eq!(store.replacements.get(), 2);

    let source_changed = apply_custom_configuration_edits::<FakeError>(
        &source_configuration()?,
        [ConfigurationEdit::UpdateTaxonomy(
            TaxonomyDefinition::from_trusted(
                gitserious_core::TaxonomyId::new("platform-taxonomy")?,
                TaxonomyVersion::new(2)?,
                Description::new("Changed only in global source.")?,
                source
                    .find_taxonomy(&gitserious_core::TaxonomyId::new("platform-taxonomy")?)
                    .ok_or("missing source taxonomy")?
                    .change_types()
                    .to_vec(),
            ),
        )],
    )?;
    assert_ne!(activated.config().custom(), &source_changed);
    assert_eq!(activated.config().custom(), &source_configuration()?);
    Ok(())
}

#[test]
fn import_conflicts_and_store_failures_preserve_project_state() -> Result<(), Box<dyn Error>> {
    let source = source_catalog()?;
    let selected = TemplateId::new("platform")?;
    let custom = apply_custom_configuration_edits::<FakeError>(
        &source_configuration()?,
        [ConfigurationEdit::UpdateTaxonomy(
            TaxonomyDefinition::from_trusted(
                gitserious_core::TaxonomyId::new("platform-taxonomy")?,
                TaxonomyVersion::new(2)?,
                Description::new("Project-owned divergence.")?,
                source
                    .find_taxonomy(&gitserious_core::TaxonomyId::new("platform-taxonomy")?)
                    .ok_or("missing taxonomy")?
                    .change_types()
                    .to_vec(),
            ),
        )],
    )?;
    let store = FakeStore::new(initialized(custom, "platform")?);
    let before = store.state.borrow().clone();
    assert!(matches!(
        import_project_template(&FakeLocator, &store, &source, Path::new("."), &selected),
        Err(ProjectConfigurationError::ImportConflict(_))
    ));
    assert_eq!(*store.state.borrow(), before);

    store.fail_replace.set(true);
    assert!(matches!(
        select_project_template(
            &FakeLocator,
            &store,
            Path::new("."),
            TemplateId::new("default")?
        ),
        Err(ProjectConfigurationError::Store(FakeError(
            "replace failed"
        )))
    ));
    assert_eq!(*store.state.borrow(), before);
    Ok(())
}

#[test]
fn unknown_selection_and_invalid_custom_batch_never_write() -> Result<(), Box<dyn Error>> {
    let store = FakeStore::new(initialized(CustomConfiguration::default(), "default")?);
    assert!(matches!(
        select_project_template(
            &FakeLocator,
            &store,
            Path::new("."),
            TemplateId::new("missing")?
        ),
        Err(ProjectConfigurationError::UnknownTemplate(id)) if id.as_str() == "missing"
    ));
    assert!(matches!(
        apply_project_configuration_edits(
            &FakeLocator,
            &store,
            Path::new("."),
            [ProjectConfigurationEdit::Custom(
                ConfigurationEdit::DeleteTemplate(TemplateId::new("missing")?)
            )]
        ),
        Err(ProjectConfigurationError::Mutation(_))
    ));
    assert_eq!(store.replacements.get(), 0);
    Ok(())
}

#[test]
fn project_forks_use_built_in_or_local_sources_without_selecting_them() -> Result<(), Box<dyn Error>>
{
    for source in ["default", "ml-research", "infra-ops", "platform"] {
        let initial = initialized(source_configuration()?, "default")?;
        let store = FakeStore::new(initial);
        let result = gitserious_app::fork_project_template(
            &FakeLocator,
            &store,
            Path::new("."),
            &TemplateId::new(source)?,
            &TemplateId::new("copy")?,
            &gitserious_core::TaxonomyId::new("copy-taxonomy")?,
            &TypesetId::new("copy-typeset")?,
        )?;
        assert_eq!(result.config().active_template().as_str(), "default");
        assert_eq!(result.lock(), &resolve_project_lock(result.config())?);
        let catalog = ConfigurationCatalog::new(result.config().custom())?;
        assert_eq!(
            catalog.resolve(&TemplateId::new(source)?)?.change_types(),
            catalog.resolve(&TemplateId::new("copy")?)?.change_types()
        );
        assert_eq!(store.replacements.get(), 1);
        let before = store.state.borrow().clone();
        assert!(
            gitserious_app::fork_project_template(
                &FakeLocator,
                &store,
                Path::new("."),
                &TemplateId::new(source)?,
                &TemplateId::new("copy")?,
                &gitserious_core::TaxonomyId::new("other")?,
                &TypesetId::new("other")?
            )
            .is_err()
        );
        assert_eq!(*store.state.borrow(), before);
    }
    let store = FakeStore::new(initialized(CustomConfiguration::default(), "default")?);
    let before = store.state.borrow().clone();
    assert!(matches!(
        gitserious_app::fork_project_template(
            &FakeLocator,
            &store,
            Path::new("."),
            &TemplateId::new("platform")?,
            &TemplateId::new("copy")?,
            &gitserious_core::TaxonomyId::new("copy-taxonomy")?,
            &TypesetId::new("copy-typeset")?
        ),
        Err(ProjectConfigurationError::Source(_))
    ));
    assert_eq!(*store.state.borrow(), before);
    store.fail_replace.set(true);
    assert!(matches!(
        gitserious_app::fork_project_template(
            &FakeLocator,
            &store,
            Path::new("."),
            &TemplateId::new("ml-research")?,
            &TemplateId::new("copy")?,
            &gitserious_core::TaxonomyId::new("copy-taxonomy")?,
            &TypesetId::new("copy-typeset")?
        ),
        Err(ProjectConfigurationError::Store(_))
    ));
    assert_eq!(*store.state.borrow(), before);
    Ok(())
}
