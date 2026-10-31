use std::fmt::Display;
use std::path::Path;

use gitserious_core::TaxonomyId;

use crate::{
    GlobalConfiguration, GlobalConfigurationStore, LibraryDraft, ProjectConfig, ProjectLock,
    ProjectState, ProjectStateStore, RepositoryLocator, RepositoryRoot, TaxonomyCatalog,
    resolve_effective_configuration,
};

/// Persistence destination edited by one reviewed session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigurationDestination {
    Global,
    Project,
}

#[derive(Clone, Debug)]
enum Scope {
    /// The working Library; it holds the configuration it was loaded from.
    Global(LibraryDraft),
    Project(Box<ProjectScope>),
}

#[derive(Clone, Debug)]
struct ProjectScope {
    root: RepositoryRoot,
    /// The Library the project resolves against; a Project session never edits it.
    global: GlobalConfiguration,
    baseline: Option<(ProjectConfig, ProjectLock)>,
    original: Option<ProjectConfig>,
    working: Option<ProjectConfig>,
    lock: Option<ProjectLock>,
}

/// One in-memory configuration draft and its guarded baseline.
#[derive(Clone, Debug)]
pub struct ConfigurationSession {
    scope: Scope,
}

impl ConfigurationSession {
    /// Opens a global snapshot without performing I/O.
    #[must_use]
    pub fn open_global(global: GlobalConfiguration) -> Self {
        Self {
            scope: Scope::Global(LibraryDraft::from_configuration(global)),
        }
    }

    /// Opens a project snapshot without performing I/O.
    pub fn open_project(
        root: RepositoryRoot,
        global: GlobalConfiguration,
        state: ProjectState,
    ) -> Result<Self, String> {
        let (original, lock, baseline) = match state {
            ProjectState::Absent => (None, None, None),
            ProjectState::ConfigOnly(config) => (Some(config), None, None),
            ProjectState::Initialized { config, lock } => (
                Some(config.clone()),
                Some(lock.clone()),
                Some((config, lock)),
            ),
            ProjectState::LockOnly => {
                return Err("gitserious.lock exists without gitserious.toml; restore or remove the orphan lock".into());
            }
        };
        Ok(Self {
            scope: Scope::Project(Box::new(ProjectScope {
                root,
                global,
                baseline,
                working: original.clone(),
                original,
                lock,
            })),
        })
    }

    /// Returns the project root when present.
    #[must_use]
    pub const fn root(&self) -> Option<&RepositoryRoot> {
        match &self.scope {
            Scope::Global(_) => None,
            Scope::Project(project) => Some(&project.root),
        }
    }

    /// Returns the working Library of a Global session.
    #[must_use]
    pub const fn library(&self) -> Option<&LibraryDraft> {
        match &self.scope {
            Scope::Global(library) => Some(library),
            Scope::Project(_) => None,
        }
    }

    /// Returns the working Library of a Global session for editing.
    pub const fn library_mut(&mut self) -> Option<&mut LibraryDraft> {
        match &mut self.scope {
            Scope::Global(library) => Some(library),
            Scope::Project(_) => None,
        }
    }

    /// Returns the Library a Project session resolves against.
    #[must_use]
    pub const fn global(&self) -> Option<&GlobalConfiguration> {
        match &self.scope {
            Scope::Global(_) => None,
            Scope::Project(project) => Some(&project.global),
        }
    }

    /// Returns the working project overrides, or none before initialization.
    #[must_use]
    pub const fn project_config(&self) -> Option<&ProjectConfig> {
        match &self.scope {
            Scope::Global(_) => None,
            Scope::Project(project) => project.working.as_ref(),
        }
    }

    /// Returns the persisted project configuration captured before staged edits.
    #[must_use]
    pub const fn persisted_project_config(&self) -> Option<&ProjectConfig> {
        match &self.scope {
            Scope::Global(_) => None,
            Scope::Project(project) => project.original.as_ref(),
        }
    }

    /// Returns the currently pinned lock.
    #[must_use]
    pub const fn lock(&self) -> Option<&ProjectLock> {
        match &self.scope {
            Scope::Global(_) => None,
            Scope::Project(project) => project.lock.as_ref(),
        }
    }

    /// Returns the merged taxonomy library.
    pub fn catalog(&self) -> Result<TaxonomyCatalog, String> {
        match &self.scope {
            Scope::Global(library) => library
                .build()
                .map_err(|problems| first_problem(&problems))?
                .catalog(),
            Scope::Project(project) => project.global.catalog(),
        }
        .map_err(|error| error.to_string())
    }

    /// Returns whether this draft differs from its persisted baseline.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        match &self.scope {
            Scope::Global(library) => library.is_changed(),
            Scope::Project(project) => project.working != project.original,
        }
    }

    /// Returns whether current global state would produce a different project lock.
    #[must_use]
    pub fn update_available(&self) -> bool {
        let Scope::Project(project) = &self.scope else {
            return false;
        };
        let (Some(config), Some(lock)) = (&project.working, &project.lock) else {
            return false;
        };
        resolve_effective_configuration(&project.global, config)
            .and_then(|effective| ProjectLock::capture(config, &effective))
            .is_ok_and(|candidate| candidate != *lock)
    }

    /// Returns the current effective-resolution error, if a source is unavailable.
    #[must_use]
    pub fn source_error(&self) -> Option<String> {
        let Scope::Project(project) = &self.scope else {
            return None;
        };
        resolve_effective_configuration(&project.global, project.working.as_ref()?)
            .err()
            .map(|error| error.to_string())
    }

    /// Replaces global selection while retaining the taxonomy library.
    pub fn configure_global(
        &mut self,
        default: TaxonomyId,
        available: Vec<TaxonomyId>,
    ) -> Result<(), String> {
        let Scope::Global(library) = &mut self.scope else {
            return Err("taxonomy library changes belong to Global configuration".into());
        };
        let built = library
            .build()
            .map_err(|problems| first_problem(&problems))?;
        GlobalConfiguration::new(
            1,
            default.clone(),
            available.clone(),
            built.custom_taxonomies().to_vec(),
        )
        .map_err(|error| error.to_string())?;
        library.select(default, available);
        Ok(())
    }

    /// Replaces project overrides. Passing none for both fields restores inheritance.
    pub fn configure_project(
        &mut self,
        default: Option<TaxonomyId>,
        available: Option<Vec<TaxonomyId>>,
    ) -> Result<(), String> {
        let Scope::Project(project) = &mut self.scope else {
            return Err("project overrides require the Project destination".into());
        };
        let config =
            ProjectConfig::new(1, default, available).map_err(|error| error.to_string())?;
        resolve_effective_configuration(&project.global, &config)
            .map_err(|error| error.to_string())?;
        project.working = Some(config);
        Ok(())
    }

    /// Stages initialization with inherited global fields.
    pub fn initialize_project(&mut self) -> Result<(), String> {
        let Scope::Project(project) = &mut self.scope else {
            return Err("project overrides require the Project destination".into());
        };
        if project.working.is_some() {
            return Err("project is already initialized".into());
        }
        project.working = Some(ProjectConfig::inherited());
        Ok(())
    }

    /// Validates and renders a semantic review without saving.
    pub fn review(&self) -> Result<String, String> {
        match &self.scope {
            Scope::Global(library) => {
                let working = library
                    .build()
                    .map_err(|problems| first_problem(&problems))?;
                let original = library.persisted();
                let mut sections = vec!["GLOBAL CONFIGURATION".to_owned()];
                let changes = library.changes();
                if !changes.taxonomies().is_empty() {
                    let lines = changes
                        .taxonomies()
                        .iter()
                        .map(|change| format!("  {change}"))
                        .collect::<Vec<_>>();
                    sections.push(format!("Taxonomies\n{}", lines.join("\n")));
                }
                let before = original.default_taxonomy();
                let after = working.default_taxonomy();
                sections.push(if before == after {
                    format!("Default\n  {after}")
                } else {
                    format!("Default\n  {before} -> {after}")
                });
                sections.push(format!(
                    "Available\n  {}",
                    working
                        .available_taxonomies()
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                Ok(sections.join("\n\n"))
            }
            Scope::Project(project) => {
                let config = project
                    .working
                    .as_ref()
                    .ok_or("project initialization has not been staged")?;
                let effective = resolve_effective_configuration(&project.global, config)
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

/// The first problem's text, until the review lists every problem.
fn first_problem(problems: &[crate::Problem]) -> String {
    problems
        .first()
        .map_or_else(|| "the Library does not build".into(), ToString::to_string)
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

pub(crate) struct Workspace<'a, L: ?Sized, S: ?Sized, G: ?Sized> {
    pub(crate) locator: &'a L,
    pub(crate) project: &'a S,
    pub(crate) global: &'a G,
    pub(crate) start: &'a Path,
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
        match &session.scope {
            Scope::Global(library) => {
                let working = library
                    .build()
                    .map_err(|problems| first_problem(&problems))?;
                if &working != library.persisted() {
                    self.global
                        .compare_and_swap(library.persisted(), &working)
                        .map_err(|error| error.to_string())?;
                }
                Ok(ConfigurationSession::open_global(working))
            }
            Scope::Project(project) => {
                let ProjectScope {
                    root,
                    global,
                    baseline,
                    working,
                    ..
                } = project.as_ref();
                let config = working
                    .as_ref()
                    .ok_or("project initialization has not been staged")?;
                let effective = resolve_effective_configuration(global, config)
                    .map_err(|error| error.to_string())?;
                let lock =
                    ProjectLock::capture(config, &effective).map_err(|error| error.to_string())?;
                match baseline {
                    None => self.project.initialize(root, config, &lock),
                    Some((old_config, old_lock)) => self
                        .project
                        .compare_and_swap(root, old_config, old_lock, config, &lock),
                }
                .map_err(|error| error.to_string())?;
                ConfigurationSession::open_project(
                    root.clone(),
                    global.clone(),
                    ProjectState::Initialized {
                        config: config.clone(),
                        lock,
                    },
                )
            }
        }
    }
}
