use std::fmt::Display;
use std::path::Path;

use gitserious_core::TaxonomyId;

use crate::{
    GlobalConfiguration, GlobalConfigurationStore, LibraryDraft, Problem, ProjectConfiguration,
    ProjectConfigurationStore, ProjectRevision, RepositoryLocator, RepositoryRoot, StoredProject,
    TaxonomyCatalog, initial_project_configuration,
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
    /// The revision Apply must still find on disk; none before initialization.
    baseline: Option<ProjectRevision>,
    original: Option<ProjectConfiguration>,
    working: Option<ProjectConfiguration>,
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

    /// Opens a project snapshot without performing I/O or reading the Library.
    #[must_use]
    pub fn open_project(root: RepositoryRoot, stored: Option<StoredProject>) -> Self {
        let (baseline, original) = match stored {
            Some(stored) => (Some(stored.revision), Some(stored.configuration)),
            None => (None, None),
        };
        Self {
            scope: Scope::Project(Box::new(ProjectScope {
                root,
                baseline,
                working: original.clone(),
                original,
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

    /// Returns the working project configuration, or none before initialization.
    #[must_use]
    pub const fn project_config(&self) -> Option<&ProjectConfiguration> {
        match &self.scope {
            Scope::Global(_) => None,
            Scope::Project(project) => project.working.as_ref(),
        }
    }

    /// Returns the persisted project configuration captured before staged edits.
    #[must_use]
    pub const fn persisted_project_config(&self) -> Option<&ProjectConfiguration> {
        match &self.scope {
            Scope::Global(_) => None,
            Scope::Project(project) => project.original.as_ref(),
        }
    }

    /// Returns the merged taxonomy library of a Global session.
    pub fn catalog(&self) -> Result<TaxonomyCatalog, String> {
        match &self.scope {
            Scope::Global(library) => library
                .persisted()
                .catalog()
                .map_err(|error| error.to_string()),
            Scope::Project(_) => Err("a Project session does not read the Library".into()),
        }
    }

    /// Returns whether this draft differs from its persisted baseline.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        match &self.scope {
            Scope::Global(library) => library.is_changed(),
            Scope::Project(project) => project.working != project.original,
        }
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

    /// Selects the project default and the order of the taxonomies it keeps.
    ///
    /// `available` may name any taxonomy the project held when the session opened.
    pub fn configure_project(
        &mut self,
        default: TaxonomyId,
        available: &[TaxonomyId],
    ) -> Result<(), String> {
        let Scope::Project(project) = &mut self.scope else {
            return Err("project changes require the Project destination".into());
        };
        let pool = project
            .original
            .iter()
            .chain(&project.working)
            .flat_map(ProjectConfiguration::taxonomies)
            .collect::<Vec<_>>();
        let taxonomies = available
            .iter()
            .map(|id| {
                pool.iter()
                    .find(|taxonomy| taxonomy.id() == id)
                    .map(|taxonomy| (*taxonomy).clone())
                    .ok_or_else(|| format!("taxonomy \"{id}\" is not part of this project"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        project.working = Some(
            ProjectConfiguration::new(1, default, taxonomies).map_err(|error| error.to_string())?,
        );
        Ok(())
    }

    /// Stages initialization with copies of the Library's default environment.
    pub fn initialize_project(&mut self, library: &GlobalConfiguration) -> Result<(), String> {
        let Scope::Project(project) = &mut self.scope else {
            return Err("project changes require the Project destination".into());
        };
        if project.working.is_some() {
            return Err("project is already initialized".into());
        }
        project.working =
            Some(initial_project_configuration(library, None).map_err(|error| error.to_string())?);
        Ok(())
    }

    /// The Project changes apply would write.
    pub fn review(&self) -> Result<Review, String> {
        match &self.scope {
            Scope::Global(_) => {
                Err("Library edits save directly; only Project changes need review".into())
            }
            Scope::Project(project) => {
                let config = project
                    .working
                    .as_ref()
                    .ok_or("project initialization has not been staged")?;
                Ok(Review {
                    changes: format!(
                        "PROJECT CONFIGURATION\n\nDefault\n  {}\n\nTaxonomies\n  {}\n\nFile\n  gitserious.toml holds {} complete taxonomy definitions",
                        config.default_taxonomy(),
                        config
                            .taxonomies()
                            .iter()
                            .map(|taxonomy| taxonomy.id().to_string())
                            .collect::<Vec<_>>()
                            .join(", "),
                        config.taxonomies().len(),
                    ),
                })
            }
        }
    }
}

/// The reviewed Project configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Review {
    pub changes: String,
}

/// The first problem's text, for the callers that need a Library that builds.
fn first_problem(problems: &[Problem]) -> String {
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
    S: ProjectConfigurationStore + ?Sized,
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
                    .saved()
                    .map_err(|problems| first_problem(&problems))?;
                let working = saved.persisted();
                if working != library.persisted() {
                    self.global
                        .compare_and_swap(library.persisted(), working)
                        .map_err(|error| error.to_string())?;
                }
                Ok(ConfigurationSession {
                    scope: Scope::Global(saved),
                })
            }
            Scope::Project(project) => {
                let ProjectScope {
                    root,
                    baseline,
                    working,
                    ..
                } = project.as_ref();
                let configuration = working
                    .as_ref()
                    .ok_or("project initialization has not been staged")?;
                let revision = match baseline {
                    None => self.project.create(root, configuration),
                    Some(expected) => self.project.replace(root, *expected, configuration),
                }
                .map_err(|error| error.to_string())?;
                Ok(ConfigurationSession::open_project(
                    root.clone(),
                    Some(StoredProject {
                        configuration: configuration.clone(),
                        revision,
                    }),
                ))
            }
        }
    }
}
