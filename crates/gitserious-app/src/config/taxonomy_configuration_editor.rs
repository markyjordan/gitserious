use std::fmt::Display;
use std::path::{Path, PathBuf};

use gitserious_core::TaxonomyId;

use crate::{
    GlobalConfiguration, GlobalConfigurationStore, LibraryDraft, Problem, ProjectConfiguration,
    ProjectConfigurationStore, ProjectRevision, RepositoryLocator, RepositoryRoot, StoredProject,
    TaxonomyCatalog, fingerprint_taxonomy, initial_project_configuration,
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
    /// The project's own taxonomies; none until the project is initialized or staged to be.
    draft: Option<LibraryDraft>,
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
        let (baseline, draft) = match stored {
            Some(stored) => (
                Some(stored.revision),
                Some(LibraryDraft::from_project(stored.configuration)),
            ),
            None => (None, None),
        };
        Self {
            scope: Scope::Project(Box::new(ProjectScope {
                root,
                baseline,
                draft,
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

    /// Returns the configuration Apply would write, or none before initialization or while the
    /// draft has problems.
    #[must_use]
    pub fn project_config(&self) -> Option<ProjectConfiguration> {
        self.project_draft()?.build_project().ok()
    }

    /// Returns the applied project configuration, or none before the first Apply.
    #[must_use]
    pub fn persisted_project_config(&self) -> Option<&ProjectConfiguration> {
        match &self.scope {
            Scope::Project(project) if project.baseline.is_some() => {
                project.draft.as_ref()?.persisted_project()
            }
            _ => None,
        }
    }

    /// Returns the merged taxonomy library of a Global session.
    pub fn catalog(&self) -> Result<TaxonomyCatalog, String> {
        match &self.scope {
            Scope::Global(library) => library
                .persisted()
                .ok_or("a Library session holds the Library")?
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
            Scope::Project(project) => match (&project.baseline, &project.draft) {
                (None, Some(_)) => true,
                (_, Some(draft)) => draft.is_changed(),
                (_, None) => false,
            },
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

    /// Selects the project default and keeps only the `available` taxonomies, in that order.
    ///
    /// Refuses, leaving the draft unchanged, when the result would not apply.
    pub fn configure_project(
        &mut self,
        default: TaxonomyId,
        available: &[TaxonomyId],
    ) -> Result<(), String> {
        let draft = self
            .project_draft_mut()
            .ok_or("project changes require an initialized Project")?;
        let mut candidate = draft.clone();
        candidate.select_project(default, available)?;
        candidate
            .build_project()
            .map_err(|problems| first_problem(&problems))?;
        *draft = candidate;
        Ok(())
    }

    /// Stages initialization with copies of the Library's default environment.
    pub fn initialize_project(&mut self, library: &GlobalConfiguration) -> Result<(), String> {
        let Scope::Project(project) = &mut self.scope else {
            return Err("project changes require the Project destination".into());
        };
        if project.draft.is_some() {
            return Err("project is already initialized".into());
        }
        project.draft = Some(LibraryDraft::from_project(
            initial_project_configuration(library).map_err(|error| error.to_string())?,
        ));
        Ok(())
    }

    /// The Project changes apply would write.
    pub fn review(&self) -> Result<Review, String> {
        let Scope::Project(project) = &self.scope else {
            return Err("Library edits save directly; only Project changes need review".into());
        };
        let draft = project
            .draft
            .as_ref()
            .ok_or("project initialization has not been staged")?;
        let config = draft
            .build_project()
            .map_err(|problems| first_problem(&problems))?;
        let before = self.persisted_project_config();
        let changes = taxonomy_changes(before, &config);
        let was = before
            .filter(|before| before.default_taxonomy() != config.default_taxonomy())
            .map_or_else(String::new, |before| {
                format!(" (was {})", before.default_taxonomy())
            });
        Ok(Review {
            changes: format!(
                "PROJECT CONFIGURATION\n\nDefault\n  {}{was}\n\nTaxonomies\n  {}\n\nChanges\n{}\n\nFile\n  gitserious.toml holds {} complete taxonomy definitions",
                config.default_taxonomy(),
                config
                    .taxonomies()
                    .iter()
                    .map(|taxonomy| taxonomy.id().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                if changes.is_empty() {
                    "  Selection only".to_owned()
                } else {
                    changes.join("\n")
                },
                config.taxonomies().len(),
            ),
        })
    }
}

/// One line per taxonomy that Apply adds, changes, or removes.
fn taxonomy_changes(
    before: Option<&ProjectConfiguration>,
    after: &ProjectConfiguration,
) -> Vec<String> {
    let mut lines = Vec::new();
    for taxonomy in after.taxonomies() {
        match before.and_then(|before| before.find(taxonomy.id())) {
            None => lines.push(format!("  + {}", taxonomy.reference())),
            Some(old) if fingerprint_taxonomy(old) != fingerprint_taxonomy(taxonomy) => {
                lines.push(format!("  ~ {}", taxonomy.reference()));
            }
            Some(_) => {}
        }
    }
    for old in before.map_or(&[][..], ProjectConfiguration::taxonomies) {
        if after.find(old.id()).is_none() {
            lines.push(format!("  - {}", old.id()));
        }
    }
    lines
}

/// The reviewed Project configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Review {
    pub changes: String,
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
    editor.edit(&ConfigurationServices::new(locator, project, global, start))
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
            Scope::Project(project) => {
                let ProjectScope {
                    root,
                    baseline,
                    draft,
                } = project.as_ref();
                let configuration = draft
                    .as_ref()
                    .ok_or("project initialization has not been staged")?
                    .build_project()
                    .map_err(|problems| first_problem(&problems))?;
                let revision = match baseline {
                    None => self.project.create(root, &configuration),
                    Some(expected) => self.project.replace(root, *expected, &configuration),
                }
                .map_err(|error| error.to_string())?;
                Ok(ConfigurationSession::open_project(
                    root.clone(),
                    Some(StoredProject {
                        configuration,
                        revision,
                    }),
                ))
            }
        }
    }

    fn library_path(&self) -> PathBuf {
        self.global.path()
    }
}
