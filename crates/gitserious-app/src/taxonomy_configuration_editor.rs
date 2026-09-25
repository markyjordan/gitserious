use std::fmt::Display;
use std::path::Path;

use gitserious_core::{Taxonomy, TaxonomyId, TaxonomyVersion};

use crate::{
    GlobalConfiguration, GlobalConfigurationStore, ProjectConfig, ProjectLock, ProjectState,
    ProjectStateStore, RepositoryLocator, RepositoryRoot, TaxonomyCatalog,
    resolve_effective_configuration,
};

/// Persistence destination edited by one reviewed session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigurationDestination {
    Global,
    Project,
}

#[derive(Clone, Debug)]
enum Baseline {
    Global,
    Project {
        root: RepositoryRoot,
        original: Option<(ProjectConfig, ProjectLock)>,
    },
}

/// One in-memory configuration draft and its guarded baseline.
#[derive(Clone, Debug)]
pub struct ConfigurationSession {
    baseline: Baseline,
    global_original: GlobalConfiguration,
    global_working: GlobalConfiguration,
    project_original: Option<ProjectConfig>,
    project_working: Option<ProjectConfig>,
    lock: Option<ProjectLock>,
}

impl ConfigurationSession {
    /// Opens a global snapshot without performing I/O.
    #[must_use]
    pub fn open_global(global: GlobalConfiguration) -> Self {
        Self {
            baseline: Baseline::Global,
            global_original: global.clone(),
            global_working: global,
            project_original: None,
            project_working: None,
            lock: None,
        }
    }

    /// Opens a project snapshot without performing I/O.
    pub fn open_project(
        root: RepositoryRoot,
        global: GlobalConfiguration,
        state: ProjectState,
    ) -> Result<Self, String> {
        let (original, working, lock, baseline) = match state {
            ProjectState::Absent => (None, None, None, None),
            ProjectState::ConfigOnly(config) => (Some(config.clone()), Some(config), None, None),
            ProjectState::Initialized { config, lock } => (
                Some(config.clone()),
                Some(config.clone()),
                Some(lock.clone()),
                Some((config, lock)),
            ),
            ProjectState::LockOnly => {
                return Err("gitserious.lock exists without gitserious.toml; restore or remove the orphan lock".into());
            }
        };
        Ok(Self {
            baseline: Baseline::Project {
                root,
                original: baseline,
            },
            global_original: global.clone(),
            global_working: global,
            project_original: original,
            project_working: working,
            lock,
        })
    }

    /// Returns the session destination.
    #[must_use]
    pub const fn destination(&self) -> ConfigurationDestination {
        match self.baseline {
            Baseline::Global => ConfigurationDestination::Global,
            Baseline::Project { .. } => ConfigurationDestination::Project,
        }
    }

    /// Returns the project root when present.
    #[must_use]
    pub const fn root(&self) -> Option<&RepositoryRoot> {
        match &self.baseline {
            Baseline::Global => None,
            Baseline::Project { root, .. } => Some(root),
        }
    }

    /// Returns the working global environment.
    #[must_use]
    pub const fn global(&self) -> &GlobalConfiguration {
        &self.global_working
    }

    /// Returns the working project overrides, or none before initialization.
    #[must_use]
    pub const fn project_config(&self) -> Option<&ProjectConfig> {
        self.project_working.as_ref()
    }

    /// Returns the persisted project configuration captured before staged edits.
    #[must_use]
    pub const fn persisted_project_config(&self) -> Option<&ProjectConfig> {
        self.project_original.as_ref()
    }

    /// Returns the currently pinned lock.
    #[must_use]
    pub const fn lock(&self) -> Option<&ProjectLock> {
        self.lock.as_ref()
    }

    /// Returns the merged taxonomy library.
    pub fn catalog(&self) -> Result<TaxonomyCatalog, String> {
        self.global_working
            .catalog()
            .map_err(|error| error.to_string())
    }

    /// Returns whether this draft differs from its persisted baseline.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        match self.destination() {
            ConfigurationDestination::Global => self.global_working != self.global_original,
            ConfigurationDestination::Project => self.project_working != self.project_original,
        }
    }

    /// Returns whether current global state would produce a different project lock.
    #[must_use]
    pub fn update_available(&self) -> bool {
        let (Some(config), Some(lock)) = (&self.project_working, &self.lock) else {
            return false;
        };
        resolve_effective_configuration(&self.global_working, config)
            .and_then(|effective| ProjectLock::capture(config, &effective))
            .is_ok_and(|candidate| candidate != *lock)
    }

    /// Returns the current effective-resolution error, if a source is unavailable.
    #[must_use]
    pub fn source_error(&self) -> Option<String> {
        self.project_working.as_ref().and_then(|config| {
            resolve_effective_configuration(&self.global_working, config)
                .err()
                .map(|error| error.to_string())
        })
    }

    /// Replaces global selection while retaining the taxonomy library.
    pub fn configure_global(
        &mut self,
        default: TaxonomyId,
        available: Vec<TaxonomyId>,
    ) -> Result<(), String> {
        self.require_global()?;
        self.global_working = GlobalConfiguration::new(
            1,
            default,
            available,
            self.global_working.custom_taxonomies().to_vec(),
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    /// Replaces project overrides. Passing none for both fields restores inheritance.
    pub fn configure_project(
        &mut self,
        default: Option<TaxonomyId>,
        available: Option<Vec<TaxonomyId>>,
    ) -> Result<(), String> {
        self.require_project()?;
        let config =
            ProjectConfig::new(1, default, available).map_err(|error| error.to_string())?;
        resolve_effective_configuration(&self.global_working, &config)
            .map_err(|error| error.to_string())?;
        self.project_working = Some(config);
        Ok(())
    }

    /// Stages initialization with inherited global fields.
    pub fn initialize_project(&mut self) -> Result<(), String> {
        self.require_project()?;
        if self.project_working.is_some() {
            return Err("project is already initialized".into());
        }
        self.project_working = Some(ProjectConfig::inherited());
        Ok(())
    }

    /// Adds a new custom taxonomy to the global library only.
    pub fn create_taxonomy(&mut self, taxonomy: Taxonomy) -> Result<(), String> {
        self.require_global()?;
        if self.catalog()?.find(taxonomy.id()).is_some() {
            return Err(format!("taxonomy {:?} already exists", taxonomy.id()));
        }
        let mut custom = self.global_working.custom_taxonomies().to_vec();
        custom.push(taxonomy);
        self.replace_custom(custom)
    }

    /// Forks any installed taxonomy into the custom library.
    pub fn fork_taxonomy(&mut self, source: &TaxonomyId, target: TaxonomyId) -> Result<(), String> {
        self.require_global()?;
        let catalog = self.catalog()?;
        if catalog.find(&target).is_some() {
            return Err(format!("taxonomy {target:?} already exists"));
        }
        let fork = catalog
            .find(source)
            .ok_or_else(|| format!("taxonomy {source:?} is not installed"))?
            .fork(target);
        self.create_taxonomy(fork)
    }

    /// Replaces an existing custom taxonomy under the same identity and next version.
    pub fn update_taxonomy(&mut self, taxonomy: Taxonomy) -> Result<(), String> {
        self.require_global()?;
        let mut custom = self.global_working.custom_taxonomies().to_vec();
        let Some(index) = custom.iter().position(|item| item.id() == taxonomy.id()) else {
            return Err("built-in or missing taxonomies cannot be edited".into());
        };
        let expected = custom[index].version().get().saturating_add(1);
        if taxonomy.version().get() != expected {
            return Err(format!(
                "taxonomy update must advance to version {expected}"
            ));
        }
        custom[index] = taxonomy;
        self.replace_custom(custom)
    }

    /// Removes an unselected custom taxonomy.
    pub fn delete_taxonomy(&mut self, id: &TaxonomyId) -> Result<(), String> {
        self.require_global()?;
        let mut custom = self.global_working.custom_taxonomies().to_vec();
        let before = custom.len();
        custom.retain(|taxonomy| taxonomy.id() != id);
        if before == custom.len() {
            return Err("built-in or missing taxonomies cannot be deleted".into());
        }
        self.replace_custom(custom)
    }

    fn replace_custom(&mut self, custom: Vec<Taxonomy>) -> Result<(), String> {
        self.global_working = GlobalConfiguration::new(
            1,
            self.global_working.default_taxonomy().clone(),
            self.global_working.available_taxonomies().to_vec(),
            custom,
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn require_global(&self) -> Result<(), String> {
        (self.destination() == ConfigurationDestination::Global)
            .then_some(())
            .ok_or_else(|| "taxonomy library changes belong to Global configuration".into())
    }

    fn require_project(&self) -> Result<(), String> {
        (self.destination() == ConfigurationDestination::Project)
            .then_some(())
            .ok_or_else(|| "project overrides require the Project destination".into())
    }

    /// Validates and renders a semantic review without saving.
    pub fn review(&self) -> Result<String, String> {
        match self.destination() {
            ConfigurationDestination::Global => {
                self.global_working
                    .catalog()
                    .map_err(|error| error.to_string())?;
                Ok(format!(
                    "GLOBAL CONFIGURATION\n\nDefault\n  {} -> {}\n\nAvailable\n  {}\n\nCustom taxonomies\n  {} -> {}",
                    self.global_original.default_taxonomy(),
                    self.global_working.default_taxonomy(),
                    self.global_working
                        .available_taxonomies()
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                    self.global_original.custom_taxonomies().len(),
                    self.global_working.custom_taxonomies().len(),
                ))
            }
            ConfigurationDestination::Project => {
                let config = self
                    .project_working
                    .as_ref()
                    .ok_or("project initialization has not been staged")?;
                let effective = resolve_effective_configuration(&self.global_working, config)
                    .map_err(|error| error.to_string())?;
                Ok(format!(
                    "PROJECT CONFIGURATION\n\nDefault\n  {}\n\nAvailable\n  {}\n\nPortable lock\n  {} taxonomy snapshots",
                    effective.default_taxonomy(),
                    effective
                        .taxonomies()
                        .iter()
                        .map(|taxonomy| taxonomy.id().to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                    effective.taxonomies().len(),
                ))
            }
        }
    }
}

/// Application operations exposed to the configuration TUI.
pub trait ConfigurationWorkspace {
    fn load(&self, destination: ConfigurationDestination) -> Result<ConfigurationSession, String>;
    fn save(&self, session: &ConfigurationSession) -> Result<ConfigurationSession, String>;
}

/// Interaction adapter for reviewed configuration authoring.
pub trait ConfigurationEditor {
    fn edit(&self, workspace: &dyn ConfigurationWorkspace) -> Result<(), String>;
}

/// Runs configuration interaction through application-owned persistence rules.
pub fn edit_configuration<L, S, G>(
    locator: &L,
    project: &S,
    global: &G,
    start: &Path,
    editor: &dyn ConfigurationEditor,
) -> Result<(), String>
where
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectStateStore + ?Sized,
    S::Error: Display,
    G: GlobalConfigurationStore + ?Sized,
    G::Error: Display,
{
    editor.edit(&Workspace {
        locator,
        project,
        global,
        start,
    })
}

struct Workspace<'a, L: ?Sized, S: ?Sized, G: ?Sized> {
    locator: &'a L,
    project: &'a S,
    global: &'a G,
    start: &'a Path,
}

impl<L, S, G> ConfigurationWorkspace for Workspace<'_, L, S, G>
where
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectStateStore + ?Sized,
    S::Error: Display,
    G: GlobalConfigurationStore + ?Sized,
    G::Error: Display,
{
    fn load(&self, destination: ConfigurationDestination) -> Result<ConfigurationSession, String> {
        let global = self.global.load().map_err(|error| error.to_string())?;
        match destination {
            ConfigurationDestination::Global => Ok(ConfigurationSession::open_global(global)),
            ConfigurationDestination::Project => {
                let root = self
                    .locator
                    .locate(self.start)
                    .map_err(|error| error.to_string())?;
                let state = self
                    .project
                    .inspect(&root)
                    .map_err(|error| error.to_string())?;
                ConfigurationSession::open_project(root, global, state)
            }
        }
    }

    fn save(&self, session: &ConfigurationSession) -> Result<ConfigurationSession, String> {
        session.review()?;
        match &session.baseline {
            Baseline::Global => {
                if session.global_working != session.global_original {
                    self.global
                        .compare_and_swap(&session.global_original, &session.global_working)
                        .map_err(|error| error.to_string())?;
                }
                Ok(ConfigurationSession::open_global(
                    session.global_working.clone(),
                ))
            }
            Baseline::Project { root, original } => {
                let config = session
                    .project_working
                    .as_ref()
                    .ok_or("project initialization has not been staged")?;
                let effective = resolve_effective_configuration(&session.global_working, config)
                    .map_err(|error| error.to_string())?;
                let lock =
                    ProjectLock::capture(config, &effective).map_err(|error| error.to_string())?;
                match original {
                    None => self.project.initialize(root, config, &lock),
                    Some((old_config, old_lock)) => self
                        .project
                        .compare_and_swap(root, old_config, old_lock, config, &lock),
                }
                .map_err(|error| error.to_string())?;
                ConfigurationSession::open_project(
                    root.clone(),
                    session.global_working.clone(),
                    ProjectState::Initialized {
                        config: config.clone(),
                        lock,
                    },
                )
            }
        }
    }
}

/// Builds an edited taxonomy while enforcing immutable identity and version advance.
pub fn revised_taxonomy(
    original: &Taxonomy,
    description: gitserious_core::Description,
    commit_types: Vec<gitserious_core::CommitTypeDefinition>,
) -> Result<Taxonomy, String> {
    Taxonomy::new(
        original.id().clone(),
        TaxonomyVersion::new(original.version().get().saturating_add(1))
            .map_err(|error| error.to_string())?,
        description,
        original.derived_from().cloned(),
        commit_types,
    )
    .map_err(|error| error.to_string())
}
