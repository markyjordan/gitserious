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
        location: None,
        message,
    };
    let text = std::str::from_utf8(contents).map_err(|error| format(error.to_string()))?;
    if let Some(line) = conflict_marker_line(text) {
        return Err(ProjectStateError::ConflictMarkers {
            path: path.to_path_buf(),
            line,
        });
    }
    let wire: ProjectConfigurationWire =
        toml::from_str(text).map_err(|error| ProjectStateError::Format {
            path: path.to_path_buf(),
            location: error.span().map(|span| TextLocation::of(text, span.start)),
            message: error.message().trim().to_owned(),
        })?;
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

/// The first line of `text` that a Git merge conflict left, which no TOML document holds.
fn conflict_marker_line(text: &str) -> Option<usize> {
    text.lines()
        .position(|line| {
            ["<<<<<<< ", ">>>>>>> ", "======="]
                .iter()
                .any(|marker| line.starts_with(marker) || line == marker.trim_end())
        })
        .map(|index| index + 1)
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

/// Where in a file a problem starts, counted from 1 as editors count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextLocation {
    pub line: usize,
    pub column: usize,
}

impl TextLocation {
    /// The line and column of byte `offset` in `text`, the column in characters.
    fn of(text: &str, offset: usize) -> Self {
        let before = &text[..offset.min(text.len())];
        let line_start = before.rfind('\n').map_or(0, |index| index + 1);
        Self {
            line: before.matches('\n').count() + 1,
            column: before[line_start..].chars().count() + 1,
        }
    }
}

/// Project persistence failure.
#[derive(Debug)]
pub enum ProjectStateError {
    /// The file is not a valid project configuration. `location` is where the parser
    /// stopped, when it knows.
    Format {
        path: PathBuf,
        location: Option<TextLocation>,
        message: String,
    },
    /// A Git merge left conflict markers in the file, first at `line`.
    ConflictMarkers {
        path: PathBuf,
        line: usize,
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
            Self::Format {
                path,
                location: Some(TextLocation { line, column }),
                message,
            } => write!(
                formatter,
                "invalid project configuration in {} at line {line}, column {column}: {message}",
                file_name(path)
            ),
            Self::Format {
                path,
                location: None,
                message,
            } => write!(
                formatter,
                "invalid project configuration in {}: {message}",
                file_name(path)
            ),
            Self::ConflictMarkers { path, line } => write!(
                formatter,
                "{name} has Git conflict markers from line {line}. Resolve {name} in Git, \
                 then refresh.",
                name = file_name(path)
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

/// `path`'s file name, which every project error names since the file sits at the root.
fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}
