use std::fmt::Display;
use std::path::{Path, PathBuf};

use crate::{
    GlobalConfiguration, GlobalConfigurationStore, LibraryDraft, Problem, ProjectConfiguration,
    ProjectConfigurationStore, RepositoryLocator, RepositoryRoot, StoredProject,
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
    /// The project's own taxonomies; none before initialization.
    draft: Option<LibraryDraft>,
}

/// The working Library, or the project's configuration as loaded.
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

    /// Opens a project snapshot without performing I/O or reading the Library.
    #[must_use]
    pub fn open_project(root: RepositoryRoot, stored: Option<StoredProject>) -> Self {
        Self {
            scope: Scope::Project(Box::new(ProjectScope {
                root,
                draft: stored.map(|stored| LibraryDraft::from_project(stored.configuration)),
            })),
        }
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

    /// Returns the working draft of the project's own taxonomies, or none before initialization.
    #[must_use]
    pub fn project_draft(&self) -> Option<&LibraryDraft> {
        match &self.scope {
            Scope::Global(_) => None,
            Scope::Project(project) => project.draft.as_ref(),
        }
    }

    /// Returns the project draft for editing.
    pub fn project_draft_mut(&mut self) -> Option<&mut LibraryDraft> {
        match &mut self.scope {
            Scope::Global(_) => None,
            Scope::Project(project) => project.draft.as_mut(),
        }
    }

    /// Returns the project's configuration as loaded, or none before initialization.
    #[must_use]
    pub fn persisted_project_config(&self) -> Option<&ProjectConfiguration> {
        self.project_draft()?.persisted_project()
    }

    /// Returns whether the Library differs from what was last loaded or saved. A Project
    /// session is never dirty.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        match &self.scope {
            Scope::Global(library) => library.is_changed(),
            Scope::Project(_) => false,
        }
    }
}

/// The first problem's text, for the callers that need a draft that builds.
fn first_problem(problems: &[Problem]) -> String {
    problems
        .first()
        .map_or_else(|| "the draft does not build".into(), ToString::to_string)
}

/// Application operations exposed to the configuration TUI.
pub trait ConfigurationWorkspace {
    fn load(&self, destination: ConfigurationDestination) -> Result<ConfigurationSession, String>;
    fn save(&self, session: &ConfigurationSession) -> Result<ConfigurationSession, String>;
    /// The file a Global session loads from and saves to.
    fn library_path(&self) -> PathBuf;
}

/// Configuration operations over the Library and the repository that contains `start`.
pub struct ConfigurationServices<'a, L: ?Sized, S: ?Sized, G: ?Sized> {
    pub(crate) locator: &'a L,
    pub(crate) project: &'a S,
    pub(crate) global: &'a G,
    pub(crate) start: &'a Path,
}

impl<'a, L, S, G> ConfigurationServices<'a, L, S, G>
where
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectConfigurationStore + ?Sized,
    S::Error: Display,
    G: GlobalConfigurationStore + ?Sized,
    G::Error: Display,
{
    pub const fn new(locator: &'a L, project: &'a S, global: &'a G, start: &'a Path) -> Self {
        Self {
            locator,
            project,
            global,
            start,
        }
    }
}

impl<L, S, G> ConfigurationWorkspace for ConfigurationServices<'_, L, S, G>
where
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectConfigurationStore + ?Sized,
    S::Error: Display,
    G: GlobalConfigurationStore + ?Sized,
    G::Error: Display,
{
    fn load(&self, destination: ConfigurationDestination) -> Result<ConfigurationSession, String> {
        match destination {
            ConfigurationDestination::Global => Ok(ConfigurationSession::open_global(
                self.global.load().map_err(|error| error.to_string())?,
            )),
            ConfigurationDestination::Project => {
                let root = self
                    .locator
                    .locate(self.start)
                    .map_err(|error| error.to_string())?;
                let stored = self
                    .project
                    .load(&root)
                    .map_err(|error| error.to_string())?;
                Ok(ConfigurationSession::open_project(root, stored))
            }
        }
    }

    fn save(&self, session: &ConfigurationSession) -> Result<ConfigurationSession, String> {
        match &session.scope {
            Scope::Global(library) => {
                let saved = library
                    .saved_on(crate::TaxonomyDate::now_utc())
                    .map_err(|problems| first_problem(&problems))?;
                let (Some(before), Some(working)) = (library.persisted(), saved.persisted()) else {
                    return Err("a Library session holds the Library".into());
                };
                if working != before {
                    self.global
                        .compare_and_swap(before, working)
                        .map_err(|error| error.to_string())?;
                }
                Ok(ConfigurationSession {
                    scope: Scope::Global(saved),
                })
            }
            // Removed by type when Library and Project get separate session types.
            Scope::Project(_) => Err("Config does not change a Project".into()),
        }
    }

    fn library_path(&self) -> PathBuf {
        self.global.path()
    }
}
