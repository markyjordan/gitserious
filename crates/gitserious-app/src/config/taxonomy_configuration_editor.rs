use std::fmt::Display;
use std::path::{Path, PathBuf};

use gitserious_core::Taxonomy;

use crate::{
    GlobalConfigurationStore, LibraryDraft, Problem, ProjectConfiguration,
    ProjectConfigurationStore, RepositoryLocator, RepositoryRoot,
};

/// The project that contains the start directory, as stored.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSnapshot {
    root: RepositoryRoot,
    configuration: Option<ProjectConfiguration>,
}

impl ProjectSnapshot {
    #[must_use]
    pub const fn new(root: RepositoryRoot, configuration: Option<ProjectConfiguration>) -> Self {
        Self {
            root,
            configuration,
        }
    }

    #[must_use]
    pub const fn root(&self) -> &RepositoryRoot {
        &self.root
    }

    /// The project's configuration, or none before initialization.
    #[must_use]
    pub const fn configuration(&self) -> Option<&ProjectConfiguration> {
        self.configuration.as_ref()
    }
}

/// The first problem's text, for the callers that need a draft that builds.
fn first_problem(problems: &[Problem]) -> String {
    problems
        .first()
        .map_or_else(|| "the draft does not build".into(), ToString::to_string)
}

/// Application operations exposed to the Config page. The Library is read and written; the
/// project is only read.
pub trait ConfigurationWorkspace {
    /// The Library as last saved.
    fn load_library(&self) -> Result<LibraryDraft, String>;
    /// Writes `library`'s changes unless the stored Library changed since it was loaded, and
    /// returns the Library as saved.
    fn save_library(&self, library: &LibraryDraft) -> Result<LibraryDraft, String>;
    fn load_project(&self) -> Result<ProjectSnapshot, String>;
    /// The Library's custom taxonomies a project can include, those with at least one type.
    fn library_taxonomies(&self) -> Result<Vec<Taxonomy>, String>;
    /// The file the Library loads from and saves to.
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
    fn load_library(&self) -> Result<LibraryDraft, String> {
        Ok(LibraryDraft::from_configuration(
            self.global.load().map_err(|error| error.to_string())?,
        ))
    }

    fn save_library(&self, library: &LibraryDraft) -> Result<LibraryDraft, String> {
        let saved = library
            .saved_on(crate::TaxonomyDate::now_utc())
            .map_err(|problems| first_problem(&problems))?;
        let (Some(before), Some(working)) = (library.persisted(), saved.persisted()) else {
            return Err("only the Library saves".into());
        };
        if working != before {
            self.global
                .compare_and_swap(before, working)
                .map_err(|error| error.to_string())?;
        }
        Ok(saved)
    }

    fn load_project(&self) -> Result<ProjectSnapshot, String> {
        let root = self
            .locator
            .locate(self.start)
            .map_err(|error| error.to_string())?;
        let stored = self
            .project
            .load(&root)
            .map_err(|error| error.to_string())?;
        Ok(ProjectSnapshot::new(
            root,
            stored.map(|stored| stored.configuration),
        ))
    }

    fn library_taxonomies(&self) -> Result<Vec<Taxonomy>, String> {
        let library = self.global.load().map_err(|error| error.to_string())?;
        Ok(library
            .custom_taxonomies()
            .iter()
            .filter_map(crate::LibraryTaxonomy::usable)
            .collect())
    }

    fn library_path(&self) -> PathBuf {
        self.global.path()
    }
}
