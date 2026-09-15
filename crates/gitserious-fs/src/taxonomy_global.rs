use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use gitserious_app::{GlobalConfiguration, GlobalConfigurationStore, StorageDirectory};
use gitserious_core::TaxonomyId;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::taxonomy_wire::{
    TaxonomyWire, configuration_error, legacy_shape, taxonomy_from_wire, taxonomy_to_wire,
};

/// Strict TOML-backed global taxonomy environment.
#[derive(Clone, Debug)]
pub struct TomlGlobalConfigurationStore {
    directory: StorageDirectory,
}

impl TomlGlobalConfigurationStore {
    #[must_use]
    pub const fn new(directory: StorageDirectory) -> Self {
        Self { directory }
    }
    fn path(&self) -> PathBuf {
        self.directory.as_path().join("config.toml")
    }
}

impl GlobalConfigurationStore for TomlGlobalConfigurationStore {
    type Error = GlobalConfigurationError;

    fn load(&self) -> Result<GlobalConfiguration, Self::Error> {
        read_configuration(&self.path())
    }

    fn compare_and_swap(
        &self,
        expected: &GlobalConfiguration,
        replacement: &GlobalConfiguration,
    ) -> Result<(), Self::Error> {
        let path = self.path();
        let observed = read_configuration(&path)?;
        if &observed != expected {
            return Err(GlobalConfigurationError::ConcurrentChange(path));
        }
        fs::create_dir_all(self.directory.as_path()).map_err(|source| {
            GlobalConfigurationError::Io {
                operation: "create directory",
                path: self.directory.as_path().to_path_buf(),
                source,
            }
        })?;
        reject_unsafe_existing(&path)?;
        let contents = render_configuration(replacement)?;
        let mut temporary = NamedTempFile::new_in(self.directory.as_path()).map_err(|source| {
            GlobalConfigurationError::Io {
                operation: "create temporary file",
                path: self.directory.as_path().to_path_buf(),
                source,
            }
        })?;
        temporary
            .write_all(contents.as_bytes())
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|source| GlobalConfigurationError::Io {
                operation: "write temporary file",
                path: temporary.path().to_path_buf(),
                source,
            })?;
        if read_configuration(&path)? != *expected {
            return Err(GlobalConfigurationError::ConcurrentChange(path));
        }
        temporary
            .persist(&path)
            .map_err(|error| GlobalConfigurationError::Io {
                operation: "replace",
                path,
                source: error.error,
            })?;
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct GlobalWire {
    config_version: u16,
    default_taxonomy: String,
    available_taxonomies: Vec<String>,
    taxonomies: Vec<TaxonomyWire>,
}

fn read_configuration(path: &Path) -> Result<GlobalConfiguration, GlobalConfigurationError> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            if legacy_shape(&contents) {
                return Err(GlobalConfigurationError::LegacyFormat(path.to_path_buf()));
            }
            let wire: GlobalWire =
                toml::from_str(&contents).map_err(|source| GlobalConfigurationError::Format {
                    path: path.to_path_buf(),
                    message: source.to_string(),
                })?;
            let default = TaxonomyId::new(wire.default_taxonomy).map_err(|error| {
                GlobalConfigurationError::Format {
                    path: path.to_path_buf(),
                    message: error.to_string(),
                }
            })?;
            let available = wire
                .available_taxonomies
                .into_iter()
                .map(TaxonomyId::new)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| GlobalConfigurationError::Format {
                    path: path.to_path_buf(),
                    message: error.to_string(),
                })?;
            let taxonomies = wire
                .taxonomies
                .into_iter()
                .enumerate()
                .map(|(index, value)| taxonomy_from_wire(value, &format!("taxonomies[{index}]")))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|message| GlobalConfigurationError::Format {
                    path: path.to_path_buf(),
                    message,
                })?;
            GlobalConfiguration::new(wire.config_version, default, available, taxonomies).map_err(
                |error| GlobalConfigurationError::Format {
                    path: path.to_path_buf(),
                    message: configuration_error(error),
                },
            )
        }
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            Ok(GlobalConfiguration::default())
        }
        Err(source) => Err(GlobalConfigurationError::Io {
            operation: "read",
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn render_configuration(
    configuration: &GlobalConfiguration,
) -> Result<String, GlobalConfigurationError> {
    toml::to_string_pretty(&GlobalWire {
        config_version: configuration.version(),
        default_taxonomy: configuration.default_taxonomy().to_string(),
        available_taxonomies: configuration
            .available_taxonomies()
            .iter()
            .map(ToString::to_string)
            .collect(),
        taxonomies: configuration
            .custom_taxonomies()
            .iter()
            .map(taxonomy_to_wire)
            .collect(),
    })
    .map_err(|source| GlobalConfigurationError::Serialize(source.to_string()))
}

fn reject_unsafe_existing(path: &Path) -> Result<(), GlobalConfigurationError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(GlobalConfigurationError::UnsafePath(path.to_path_buf()))
        }
        Ok(_) => Ok(()),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(GlobalConfigurationError::Io {
            operation: "inspect",
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Global taxonomy persistence failure.
#[derive(Debug)]
pub enum GlobalConfigurationError {
    LegacyFormat(PathBuf),
    Format {
        path: PathBuf,
        message: String,
    },
    Serialize(String),
    ConcurrentChange(PathBuf),
    UnsafePath(PathBuf),
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

impl Display for GlobalConfigurationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::LegacyFormat(path) => write!(
                formatter,
                "{} uses the superseded pre-release template/typeset format; back it up and recreate it with taxonomy-first gitserious config",
                path.display()
            ),
            Self::Format { path, message } => write!(
                formatter,
                "invalid global configuration {}: {message}",
                path.display()
            ),
            Self::Serialize(message) => write!(
                formatter,
                "could not serialize global configuration: {message}"
            ),
            Self::ConcurrentChange(path) => write!(
                formatter,
                "global configuration changed while editing: {}",
                path.display()
            ),
            Self::UnsafePath(path) => write!(
                formatter,
                "global configuration path is not a regular file: {}",
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

impl Error for GlobalConfigurationError {}
