use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use gitserious_app::{
    ProjectConfiguration, ProjectConfigurationStore, ProjectRevision, RepositoryRoot, StoredProject,
};
use gitserious_core::TaxonomyId;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::taxonomy_wire::{TaxonomyWire, taxonomy_from_wire, taxonomy_to_wire};

const CONFIG_FILE: &str = "gitserious.toml";
const LOCAL_STATE_DIRECTORY: &str = ".gitserious";
const LOCAL_IGNORE_CONTENTS: &str = "*\n";

/// Strict TOML adapter for the self-contained root `gitserious.toml`.
#[derive(Clone, Copy, Debug, Default)]
pub struct TomlProjectStore;

impl ProjectConfigurationStore for TomlProjectStore {
    type Error = ProjectStateError;

    fn load(&self, root: &RepositoryRoot) -> Result<Option<StoredProject>, Self::Error> {
        let path = root.as_path().join(CONFIG_FILE);
        if !regular_file_exists(&path)? {
            return Ok(None);
        }
        let contents = read_bytes(&path)?;
        Ok(Some(StoredProject {
            configuration: parse_configuration(&path, &contents)?,
            revision: ProjectRevision::of_bytes(&contents),
        }))
    }

    fn create(
        &self,
        root: &RepositoryRoot,
        configuration: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error> {
        let path = root.as_path().join(CONFIG_FILE);
        if regular_file_exists(&path)? {
            return Err(ProjectStateError::Collision(path));
        }
        let contents = render_configuration(configuration)?;
        persist_new(prepare(root.as_path(), contents.as_bytes())?, &path)?;
        Ok(ProjectRevision::of_bytes(contents.as_bytes()))
    }

    fn replace(
        &self,
        root: &RepositoryRoot,
        expected: ProjectRevision,
        configuration: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error> {
        let path = root.as_path().join(CONFIG_FILE);
        let ensure_current = || -> Result<(), ProjectStateError> {
            if !regular_file_exists(&path)?
                || ProjectRevision::of_bytes(&read_bytes(&path)?) != expected
            {
                return Err(ProjectStateError::ConcurrentChange(path.clone()));
            }
            Ok(())
        };
        ensure_current()?;
        let contents = render_configuration(configuration)?;
        let temporary = prepare(root.as_path(), contents.as_bytes())?;
        ensure_current()?;
        persist_replace(temporary, &path)?;
        Ok(ProjectRevision::of_bytes(contents.as_bytes()))
    }

    fn ensure_local_state(&self, root: &RepositoryRoot) -> Result<(), Self::Error> {
        let directory = root.as_path().join(LOCAL_STATE_DIRECTORY);
        ensure_directory(&directory)?;
        let ignore = directory.join(".gitignore");
        if !regular_file_exists(&ignore)? {
            fs::write(&ignore, LOCAL_IGNORE_CONTENTS).map_err(|source| ProjectStateError::Io {
                operation: "write",
                path: ignore,
                source,
            })?;
        }
        Ok(())
    }
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, ProjectStateError> {
    fs::read(path).map_err(|source| ProjectStateError::Io {
        operation: "read",
        path: path.to_path_buf(),
        source,
    })
}

fn parse_configuration(
    path: &Path,
    contents: &[u8],
) -> Result<ProjectConfiguration, ProjectStateError> {
    let format = |message: String| ProjectStateError::Format {
        path: path.to_path_buf(),
        message,
    };
    let text = std::str::from_utf8(contents).map_err(|error| format(error.to_string()))?;
    let wire: ProjectConfigurationWire =
        toml::from_str(text).map_err(|error| format(error.to_string()))?;
    let default = TaxonomyId::new(wire.default_taxonomy)
        .map_err(|error| format(format!("default-taxonomy: {error}")))?;
    let taxonomies = wire
        .taxonomies
        .into_iter()
        .enumerate()
        .map(|(index, value)| taxonomy_from_wire(value, &format!("taxonomies[{index}]")))
        .collect::<Result<Vec<_>, _>>()
        .map_err(format)?;
    ProjectConfiguration::new(wire.config_version, default, taxonomies)
        .map_err(|error| format(error.to_string()))
}

fn render_configuration(configuration: &ProjectConfiguration) -> Result<String, ProjectStateError> {
    toml::to_string_pretty(&ProjectConfigurationWire {
        config_version: configuration.version(),
        default_taxonomy: configuration.default_taxonomy().to_string(),
        taxonomies: configuration
            .taxonomies()
            .iter()
            .map(taxonomy_to_wire)
            .collect(),
    })
    .map_err(|source| ProjectStateError::Serialize(source.to_string()))
}

fn ensure_directory(path: &Path) -> Result<(), ProjectStateError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(ProjectStateError::UnsafePath(path.to_path_buf())),
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|source| ProjectStateError::Io {
                operation: "create directory",
                path: path.to_path_buf(),
                source,
            })
        }
        Err(source) => Err(ProjectStateError::Io {
            operation: "inspect",
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn regular_file_exists(path: &Path) -> Result<bool, ProjectStateError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(ProjectStateError::UnsafePath(path.to_path_buf()))
        }
        Ok(_) => Ok(true),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(ProjectStateError::Io {
            operation: "inspect",
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn prepare(directory: &Path, contents: &[u8]) -> Result<NamedTempFile, ProjectStateError> {
    let mut file = NamedTempFile::new_in(directory).map_err(|source| ProjectStateError::Io {
        operation: "create temporary file",
        path: directory.to_path_buf(),
        source,
    })?;
    file.write_all(contents)
        .and_then(|()| file.as_file().sync_all())
        .map_err(|source| ProjectStateError::Io {
            operation: "write temporary file",
            path: file.path().to_path_buf(),
            source,
        })?;
    Ok(file)
}

fn persist_new(file: NamedTempFile, path: &Path) -> Result<(), ProjectStateError> {
    file.persist_noclobber(path).map(|_| ()).map_err(|error| {
        if error.error.kind() == io::ErrorKind::AlreadyExists {
            ProjectStateError::Collision(path.to_path_buf())
        } else {
            ProjectStateError::Io {
                operation: "create",
                path: path.to_path_buf(),
                source: error.error,
            }
        }
    })
}

fn persist_replace(file: NamedTempFile, path: &Path) -> Result<(), ProjectStateError> {
    file.persist(path)
        .map(|_| ())
        .map_err(|error| ProjectStateError::Io {
            operation: "replace",
            path: path.to_path_buf(),
            source: error.error,
        })
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct ProjectConfigurationWire {
    config_version: u16,
    default_taxonomy: String,
    taxonomies: Vec<TaxonomyWire>,
}

/// Project persistence failure.
#[derive(Debug)]
pub enum ProjectStateError {
    Format {
        path: PathBuf,
        message: String,
    },
    Serialize(String),
    Collision(PathBuf),
    ConcurrentChange(PathBuf),
    UnsafePath(PathBuf),
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

impl Display for ProjectStateError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format { path, message } => write!(
                formatter,
                "invalid project configuration {}: {message}",
                path.display()
            ),
            Self::Serialize(message) => {
                write!(
                    formatter,
                    "could not serialize project configuration: {message}"
                )
            }
            Self::Collision(path) => write!(
                formatter,
                "refusing to overwrite existing {}",
                path.display()
            ),
            Self::ConcurrentChange(path) => write!(
                formatter,
                "{} changed since it was opened; reload Project before applying",
                path.display()
            ),
            Self::UnsafePath(path) => write!(
                formatter,
                "refusing project configuration path that is a symlink or the wrong file type: {}",
                path.display()
            ),
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "could not {operation} {}: {source}",
                path.display()
            ),
        }
    }
}

impl Error for ProjectStateError {}
