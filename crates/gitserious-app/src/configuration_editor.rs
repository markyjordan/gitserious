use std::fmt::Display;
use std::path::Path;

use gitserious_core::TemplateId;

use crate::{
    ConfigurationCatalog, ConfigurationEdit, CustomConfiguration, GlobalConfigurationStore,
    ProjectConfig, ProjectLock, ProjectState, ProjectStateStore, RepositoryLocator, RepositoryRoot,
    apply_custom_configuration_edits, resolve_project_lock,
};

/// The explicit storage destination of an editing session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigurationDestination {
    /// Personal reusable definitions.
    Global,
    /// Repository-owned definitions and active template.
    Project,
}

#[derive(Clone, Debug)]
enum Baseline {
    Global,
    Project {
        root: RepositoryRoot,
        config: ProjectConfig,
        lock: Box<ProjectLock>,
    },
}

/// A configuration draft and the exact snapshot from which it was authored.
///
/// Staging enforces local identity/version rules; cross-definition references
/// and full typeset coverage are checked at review and again before saving.
#[derive(Clone, Debug)]
pub struct ConfigurationSession {
    baseline: Baseline,
    original: CustomConfiguration,
    working: CustomConfiguration,
    active: Option<TemplateId>,
    edits: Vec<ConfigurationEdit>,
}

impl ConfigurationSession {
    /// Opens a global draft without performing I/O.
    ///
    /// # Errors
    /// Returns an actionable error when the supplied catalog is invalid.
    pub fn global(custom: CustomConfiguration) -> Result<Self, String> {
        ConfigurationCatalog::new(&custom).map_err(|error| error.to_string())?;
        Ok(Self {
            baseline: Baseline::Global,
            original: custom.clone(),
            working: custom,
            active: None,
            edits: Vec::new(),
        })
    }

    /// Opens an initialized project draft without performing I/O.
    ///
    /// # Errors
    /// Returns an actionable error for unresolved or stale project policy.
    pub fn project(
        root: RepositoryRoot,
        config: ProjectConfig,
        lock: ProjectLock,
    ) -> Result<Self, String> {
        if resolve_project_lock(&config).map_err(|error| error.to_string())? != lock {
            return Err(
                "project policy is stale; run `gitserious init` before configuring the project"
                    .into(),
            );
        }
        Ok(Self {
            original: config.custom().clone(),
            working: config.custom().clone(),
            active: Some(config.active_template().clone()),
            baseline: Baseline::Project {
                root,
                config,
                lock: Box::new(lock),
            },
            edits: Vec::new(),
        })
    }

    /// Returns the destination captured when the session was opened.
    #[must_use]
    pub const fn destination(&self) -> ConfigurationDestination {
        match &self.baseline {
            Baseline::Global => ConfigurationDestination::Global,
            Baseline::Project { .. } => ConfigurationDestination::Project,
        }
    }

    /// Returns the project root when editing repository-owned definitions.
    #[must_use]
    pub const fn root(&self) -> Option<&RepositoryRoot> {
        match &self.baseline {
            Baseline::Global => None,
            Baseline::Project { root, .. } => Some(root),
        }
    }

    /// Returns the unmodified snapshot used for guarded persistence.
    #[must_use]
    pub const fn original(&self) -> &CustomConfiguration {
        &self.original
    }

    /// Returns the staged definitions, which may have unresolved references.
    #[must_use]
    pub const fn custom(&self) -> &CustomConfiguration {
        &self.working
    }

    /// Returns the staged project default; global configuration has no default.
    #[must_use]
    pub const fn active_template(&self) -> Option<&TemplateId> {
        self.active.as_ref()
    }

    /// Returns the project default captured when the session was opened.
    #[must_use]
    pub const fn original_active_template(&self) -> Option<&TemplateId> {
        match &self.baseline {
            Baseline::Global => None,
            Baseline::Project { config, .. } => Some(config.active_template()),
        }
    }

    /// Returns whether the staged state differs from its original snapshot.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.original != self.working || self.active.as_ref() != self.original_active_template()
    }

    /// Stages a batch, preserving the draft if any local edit is invalid.
    ///
    /// # Errors
    /// Returns identity, version, or reserved-definition errors. An incomplete
    /// dependency graph may be staged so related definitions can be edited next.
    pub fn stage(
        &mut self,
        edits: impl IntoIterator<Item = ConfigurationEdit>,
    ) -> Result<(), String> {
        let edits: Vec<_> = edits.into_iter().collect();
        let mut candidate = self.working.clone();
        for edit in &edits {
            let recreates_original = match edit {
                ConfigurationEdit::CreateTaxonomy(value) => {
                    self.original
                        .taxonomies()
                        .iter()
                        .any(|old| old.id() == value.id())
                        && !candidate
                            .taxonomies()
                            .iter()
                            .any(|old| old.id() == value.id())
                }
                ConfigurationEdit::CreateTypeset(value) => {
                    self.original
                        .typesets()
                        .iter()
                        .any(|old| old.taxonomy() == value.taxonomy() && old.id() == value.id())
                        && !candidate
                            .typesets()
                            .iter()
                            .any(|old| old.taxonomy() == value.taxonomy() && old.id() == value.id())
                }
                ConfigurationEdit::CreateTemplate(value) => {
                    self.original
                        .templates()
                        .iter()
                        .any(|old| old.id() == value.id())
                        && !candidate
                            .templates()
                            .iter()
                            .any(|old| old.id() == value.id())
                }
                _ => false,
            };
            if recreates_original {
                return Err("An existing identity cannot be deleted and recreated in one session. Discard the deletion and edit the definition to preserve its version history.".into());
            }
            crate::configuration_crud::apply_edit::<String>(&mut candidate, edit.clone())
                .map_err(|error| error.to_string())?;
        }
        candidate.sort();
        self.working = candidate;
        self.edits.extend(edits);
        Ok(())
    }

    /// Validates the complete draft for review without saving it.
    ///
    /// # Errors
    /// Returns invalid references, incomplete coverage, or invalid project policy.
    pub fn validate(&self) -> Result<(), String> {
        let custom = apply_custom_configuration_edits::<String>(&self.original, self.edits.clone())
            .map_err(|error| error.to_string())?;
        if let Some(active) = &self.active {
            let config =
                ProjectConfig::new(1, active.clone(), custom).map_err(|error| error.to_string())?;
            resolve_project_lock(&config).map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

/// Application operations available to the configuration interaction adapter.
pub trait ConfigurationWorkspace {
    /// Opens a fresh snapshot for the requested destination.
    ///
    /// # Errors
    /// Returns load, repository discovery, initialization, or policy errors.
    fn load(&self, destination: ConfigurationDestination) -> Result<ConfigurationSession, String>;

    /// Validates and saves one reviewed snapshot, returning a clean session.
    ///
    /// # Errors
    /// Returns validation, concurrent-change, or persistence errors.
    fn save(&self, session: &ConfigurationSession) -> Result<ConfigurationSession, String>;
}

/// Terminal or other interaction adapter for configuration authoring.
pub trait ConfigurationEditor {
    /// Runs an editing interaction, including explicit review before save.
    ///
    /// # Errors
    /// Returns interaction setup or terminal failures.
    fn edit(&self, workspace: &dyn ConfigurationWorkspace) -> Result<(), String>;
}

/// Runs configuration interaction with application-owned persistence rules.
///
/// # Errors
/// Returns interaction errors; recoverable workspace failures are presented by
/// the editor while retaining the current draft.
pub fn edit_configuration<L, S, U>(
    locator: &L,
    project: &S,
    global: &U,
    start: &Path,
    editor: &dyn ConfigurationEditor,
) -> Result<(), String>
where
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectStateStore + ?Sized,
    S::Error: Display,
    U: GlobalConfigurationStore + ?Sized,
    U::Error: Display,
{
    editor.edit(&Workspace {
        locator,
        project,
        global,
        start,
    })
}

struct Workspace<'a, L: ?Sized, S: ?Sized, U: ?Sized> {
    locator: &'a L,
    project: &'a S,
    global: &'a U,
    start: &'a Path,
}

impl<L, S, U> ConfigurationWorkspace for Workspace<'_, L, S, U>
where
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectStateStore + ?Sized,
    S::Error: Display,
    U: GlobalConfigurationStore + ?Sized,
    U::Error: Display,
{
    fn load(&self, destination: ConfigurationDestination) -> Result<ConfigurationSession, String> {
        match destination {
            ConfigurationDestination::Global => {
                ConfigurationSession::global(self.global.load().map_err(|error| error.to_string())?)
            }
            ConfigurationDestination::Project => {
                let root = self
                    .locator
                    .locate(self.start)
                    .map_err(|error| error.to_string())?;
                match self.project.inspect(&root).map_err(|error| error.to_string())? {
                    ProjectState::Initialized { config, lock } => ConfigurationSession::project(root, config, lock),
                    ProjectState::Absent | ProjectState::ConfigOnly(_) => Err("run `gitserious init` before configuring the project".into()),
                    ProjectState::LockOnly => Err("gitserious.lock exists without gitserious.toml; restore the project configuration".into()),
                }
            }
        }
    }

    fn save(&self, session: &ConfigurationSession) -> Result<ConfigurationSession, String> {
        session.validate()?;
        if !session.is_dirty() {
            return Ok(session.clone());
        }
        match &session.baseline {
            Baseline::Global => {
                self.global
                    .compare_and_swap(&session.original, &session.working)
                    .map_err(|error| error.to_string())?;
                ConfigurationSession::global(session.working.clone())
            }
            Baseline::Project { root, config, lock } => {
                let active = session
                    .active
                    .clone()
                    .ok_or("project template is missing")?;
                let replacement = ProjectConfig::new(1, active, session.working.clone())
                    .map_err(|error| error.to_string())?;
                let new_lock =
                    resolve_project_lock(&replacement).map_err(|error| error.to_string())?;
                self.project
                    .compare_and_swap(root, config, lock, &replacement, &new_lock)
                    .map_err(|error| error.to_string())?;
                ConfigurationSession::project(root.clone(), replacement, new_lock)
            }
        }
    }
}
