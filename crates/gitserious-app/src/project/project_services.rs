use std::fmt::Display;
use std::path::Path;

use crate::{
    ProjectConfiguration, ProjectConfigurationStore, ProjectOverview, RepositoryInitializer,
    RepositoryLocator, RepositoryStatusReader,
};

/// The project file every gitserious repository commits.
const POLICY_FILE: &str = "gitserious.toml";

/// Project operations over the repository that contains `start`.
pub struct ProjectServices<'a, L: ?Sized, S: ?Sized> {
    locator: &'a L,
    store: &'a S,
    status: &'a dyn RepositoryStatusReader,
    initializer: &'a dyn RepositoryInitializer,
    start: &'a Path,
}

impl<'a, L, S> ProjectServices<'a, L, S>
where
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectConfigurationStore + ?Sized,
    S::Error: Display,
{
    pub fn new(
        locator: &'a L,
        store: &'a S,
        status: &'a dyn RepositoryStatusReader,
        initializer: &'a dyn RepositoryInitializer,
        start: &'a Path,
    ) -> Self {
        Self {
            locator,
            store,
            status,
            initializer,
            start,
        }
    }

    /// The repository and project that contain the start directory.
    pub fn overview(&self) -> ProjectOverview {
        let directory = self.start.to_path_buf();
        let root = match self.locator.locate(self.start) {
            Ok(root) => root,
            Err(error) if L::is_not_repository(&error) => {
                return ProjectOverview::NoRepository { directory };
            }
            Err(error) => {
                return ProjectOverview::Unavailable {
                    directory,
                    error: error.to_string(),
                };
            }
        };
        let stored = match self.store.load(&root) {
            Ok(Some(stored)) => stored,
            Ok(None) => return ProjectOverview::Uninitialized { root },
            Err(error) => {
                return ProjectOverview::Unavailable {
                    directory,
                    error: error.to_string(),
                };
            }
        };
        let default = stored.configuration.default_taxonomy().clone();
        let Some(version) = stored
            .configuration
            .find(&default)
            .map(gitserious_core::Taxonomy::version)
        else {
            return ProjectOverview::Unavailable {
                directory,
                error: format!("{POLICY_FILE} names a missing default taxonomy {default}."),
            };
        };
        let policy_untracked = self.status.read(&root).is_ok_and(|status| {
            status
                .entries
                .iter()
                .any(|entry| entry.untracked && entry.path == Path::new(POLICY_FILE))
        });
        ProjectOverview::Initialized {
            root,
            default,
            version,
            policy_untracked,
        }
    }

    /// Creates the project from `configuration`, and the Git repository first when there is
    /// none; an existing project is never rewritten.
    pub fn initialize(&self, configuration: &ProjectConfiguration) -> Result<(), String> {
        let locate = || self.locator.locate(self.start);
        let root = match locate() {
            Ok(root) => root,
            Err(error) if L::is_not_repository(&error) => {
                self.initializer.initialize(self.start)?;
                locate().map_err(|error| error.to_string())?
            }
            Err(error) => return Err(error.to_string()),
        };
        self.store
            .ensure_local_state(&root)
            .map_err(|error| error.to_string())?;
        self.store
            .create(&root, configuration)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}
