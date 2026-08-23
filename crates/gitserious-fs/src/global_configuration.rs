use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use gitserious_app::{
    CUSTOM_CONFIGURATION_VERSION, CustomConfiguration, DirectoryCreator, GlobalConfigurationStore,
    StorageDirectory,
};
use gitserious_core::{
    ChangeTypeDefinition, ChangeTypeId, ChangeTypeSchema, ConditionId, Description,
    PropertyCondition, PropertyDefinition, PropertyKey, PropertyMultiplicity, PropertyRequirement,
    TaxonomyDefinition, TaxonomyId, TaxonomyVersion, TemplateDefinition, TemplateId,
    TemplateVersion, TypesetDefinition, TypesetId, TypesetVersion,
};
use serde::{Deserialize, Serialize};

use crate::LocalDirectoryCreator;

const CONFIGURATION_FILE: &str = "config.toml";
static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Strict TOML adapter for global reusable taxonomy configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TomlGlobalConfigurationStore {
    directory: StorageDirectory,
}

impl TomlGlobalConfigurationStore {
    /// Creates a store beneath a platform-resolved global config directory.
    #[must_use]
    pub const fn new(directory: StorageDirectory) -> Self {
        Self { directory }
    }

    /// Returns the exact global configuration file path.
    #[must_use]
    pub fn path(&self) -> PathBuf {
        self.directory.as_path().join(CONFIGURATION_FILE)
    }
}

impl GlobalConfigurationStore for TomlGlobalConfigurationStore {
    type Error = GlobalConfigurationError;

    fn load(&self) -> Result<CustomConfiguration, Self::Error> {
        read_configuration(&self.path())
    }

    fn compare_and_swap(
        &self,
        expected: &CustomConfiguration,
        replacement: &CustomConfiguration,
    ) -> Result<(), Self::Error> {
        ensure_config_directory(&self.directory)?;
        let path = self.path();
        if read_configuration(&path)? != *expected {
            return Err(GlobalConfigurationError::ConcurrentChange(path));
        }
        let contents = render_configuration(replacement)?;
        let temporary = write_temporary(self.directory.as_path(), contents.as_bytes())?;
        if read_configuration(&path)? != *expected {
            return Err(rollback_file(
                &temporary,
                GlobalConfigurationError::ConcurrentChange(path),
            ));
        }
        if let Err(source) = fs::rename(&temporary, &path) {
            return Err(rollback_file(
                &temporary,
                GlobalConfigurationError::Io {
                    operation: "replace",
                    path,
                    source,
                },
            ));
        }
        Ok(())
    }
}

fn ensure_config_directory(directory: &StorageDirectory) -> Result<(), GlobalConfigurationError> {
    match fs::symlink_metadata(directory.as_path()) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(
            GlobalConfigurationError::Symlink(directory.as_path().to_path_buf()),
        ),
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(GlobalConfigurationError::ExpectedDirectory(
            directory.as_path().to_path_buf(),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => LocalDirectoryCreator
            .ensure(directory)
            .map_err(|source| GlobalConfigurationError::Io {
                operation: "create",
                path: directory.as_path().to_path_buf(),
                source,
            }),
        Err(source) => Err(GlobalConfigurationError::Io {
            operation: "inspect",
            path: directory.as_path().to_path_buf(),
            source,
        }),
    }
}

fn read_configuration(path: &Path) -> Result<CustomConfiguration, GlobalConfigurationError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(GlobalConfigurationError::Symlink(path.to_path_buf()));
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err(GlobalConfigurationError::ExpectedFile(path.to_path_buf()));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(CustomConfiguration::default());
        }
        Err(source) => {
            return Err(GlobalConfigurationError::Io {
                operation: "inspect",
                path: path.to_path_buf(),
                source,
            });
        }
    }
    let contents = fs::read_to_string(path).map_err(|source| GlobalConfigurationError::Io {
        operation: "read",
        path: path.to_path_buf(),
        source,
    })?;
    let wire = toml::from_str::<ConfigurationWire>(&contents).map_err(|source| {
        GlobalConfigurationError::Format {
            path: path.to_path_buf(),
            source: Box::new(CustomConfigurationFormatError::Toml(source)),
        }
    })?;
    configuration_from_wire(wire).map_err(|source| GlobalConfigurationError::Format {
        path: path.to_path_buf(),
        source: Box::new(source),
    })
}

fn render_configuration(
    configuration: &CustomConfiguration,
) -> Result<String, GlobalConfigurationError> {
    let wire = configuration_to_wire(configuration);
    let mut rendered =
        toml::to_string(&wire).map_err(|source| GlobalConfigurationError::Serialization {
            source: Box::new(source),
        })?;
    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    Ok(rendered)
}

fn write_temporary(directory: &Path, contents: &[u8]) -> Result<PathBuf, GlobalConfigurationError> {
    for _ in 0..100 {
        let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(
            ".{CONFIGURATION_FILE}.tmp.{}.{}",
            std::process::id(),
            sequence
        ));
        match write_new_file(&path, contents) {
            Ok(()) => return Ok(path),
            Err(GlobalConfigurationError::Collision(_)) => {}
            Err(error) => return Err(error),
        }
    }
    Err(GlobalConfigurationError::TemporaryFileUnavailable(
        directory.to_path_buf(),
    ))
}

fn write_new_file(path: &Path, contents: &[u8]) -> Result<(), GlobalConfigurationError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| {
            if source.kind() == io::ErrorKind::AlreadyExists {
                GlobalConfigurationError::Collision(path.to_path_buf())
            } else {
                GlobalConfigurationError::Io {
                    operation: "create",
                    path: path.to_path_buf(),
                    source,
                }
            }
        })?;
    if let Err(source) = file.write_all(contents).and_then(|()| file.sync_all()) {
        return Err(rollback_file(
            path,
            GlobalConfigurationError::Io {
                operation: "write",
                path: path.to_path_buf(),
                source,
            },
        ));
    }
    Ok(())
}

fn rollback_file(path: &Path, original: GlobalConfigurationError) -> GlobalConfigurationError {
    match fs::remove_file(path) {
        Ok(()) => original,
        Err(source) if source.kind() == io::ErrorKind::NotFound => original,
        Err(source) => GlobalConfigurationError::Rollback {
            original: Box::new(original),
            path: path.to_path_buf(),
            source,
        },
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct ConfigurationWire {
    config_version: u16,
    taxonomies: Vec<TaxonomyWire>,
    typesets: Vec<TypesetWire>,
    templates: Vec<TemplateWire>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct TaxonomyWire {
    id: String,
    version: u16,
    description: String,
    change_types: Vec<ChangeTypeWire>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct ChangeTypeWire {
    id: String,
    description: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct TypesetWire {
    taxonomy: String,
    id: String,
    version: u16,
    description: String,
    schemas: Vec<ChangeTypeSchemaWire>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct ChangeTypeSchemaWire {
    change_type: String,
    properties: Vec<PropertyWire>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct PropertyWire {
    key: String,
    description: String,
    multiplicity: MultiplicityWire,
    requirement: RequirementWire,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum MultiplicityWire {
    Single,
    Multiple,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "level", rename_all = "kebab-case", deny_unknown_fields)]
enum RequirementWire {
    Required,
    Recommended,
    Optional,
    Conditional {
        condition: String,
        rationale: String,
    },
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct TemplateWire {
    id: String,
    version: u16,
    description: String,
    taxonomy: String,
    typeset: String,
}

fn configuration_from_wire(
    wire: ConfigurationWire,
) -> Result<CustomConfiguration, CustomConfigurationFormatError> {
    if wire.config_version != CUSTOM_CONFIGURATION_VERSION {
        return Err(CustomConfigurationFormatError::UnsupportedVersion(
            wire.config_version,
        ));
    }
    let taxonomies = wire
        .taxonomies
        .into_iter()
        .enumerate()
        .map(|(index, taxonomy)| taxonomy_from_wire(index, taxonomy))
        .collect::<Result<Vec<_>, _>>()?;
    let typesets = wire
        .typesets
        .into_iter()
        .enumerate()
        .map(|(index, typeset)| typeset_from_wire(index, typeset))
        .collect::<Result<Vec<_>, _>>()?;
    let templates = wire
        .templates
        .into_iter()
        .enumerate()
        .map(|(index, template)| template_from_wire(index, template))
        .collect::<Result<Vec<_>, _>>()?;
    CustomConfiguration::new(taxonomies, typesets, templates)
        .map_err(|error| value_error("configuration", error))
}

fn taxonomy_from_wire(
    index: usize,
    wire: TaxonomyWire,
) -> Result<TaxonomyDefinition, CustomConfigurationFormatError> {
    let location = format!("taxonomies[{index}]");
    let id =
        TaxonomyId::new(wire.id).map_err(|error| value_error(format!("{location}.id"), error))?;
    let version = TaxonomyVersion::new(wire.version)
        .map_err(|error| value_error(format!("{location}.version"), error))?;
    let description = Description::new(wire.description)
        .map_err(|error| value_error(format!("{location}.description"), error))?;
    let change_types = wire
        .change_types
        .into_iter()
        .enumerate()
        .map(|(change_index, change_type)| {
            let change_location = format!("{location}.change-types[{change_index}]");
            let id = ChangeTypeId::new(change_type.id)
                .map_err(|error| value_error(format!("{change_location}.id"), error))?;
            let description = Description::new(change_type.description)
                .map_err(|error| value_error(format!("{change_location}.description"), error))?;
            Ok(ChangeTypeDefinition::new(id, description))
        })
        .collect::<Result<Vec<_>, CustomConfigurationFormatError>>()?;
    TaxonomyDefinition::new(id, version, description, change_types)
        .map_err(|error| value_error(location, error))
}

fn typeset_from_wire(
    index: usize,
    wire: TypesetWire,
) -> Result<TypesetDefinition, CustomConfigurationFormatError> {
    let location = format!("typesets[{index}]");
    let taxonomy = TaxonomyId::new(wire.taxonomy)
        .map_err(|error| value_error(format!("{location}.taxonomy"), error))?;
    let id =
        TypesetId::new(wire.id).map_err(|error| value_error(format!("{location}.id"), error))?;
    let version = TypesetVersion::new(wire.version)
        .map_err(|error| value_error(format!("{location}.version"), error))?;
    let description = Description::new(wire.description)
        .map_err(|error| value_error(format!("{location}.description"), error))?;
    let schemas = wire
        .schemas
        .into_iter()
        .enumerate()
        .map(|(schema_index, schema)| {
            schema_from_wire(format!("{location}.schemas[{schema_index}]"), schema)
        })
        .collect::<Result<Vec<_>, _>>()?;
    TypesetDefinition::new(taxonomy, id, version, description, schemas)
        .map_err(|error| value_error(location, error))
}

fn schema_from_wire(
    location: String,
    wire: ChangeTypeSchemaWire,
) -> Result<ChangeTypeSchema, CustomConfigurationFormatError> {
    let change_type = ChangeTypeId::new(wire.change_type)
        .map_err(|error| value_error(format!("{location}.change-type"), error))?;
    let properties = wire
        .properties
        .into_iter()
        .enumerate()
        .map(|(property_index, property)| {
            property_from_wire(format!("{location}.properties[{property_index}]"), property)
        })
        .collect::<Result<Vec<_>, _>>()?;
    ChangeTypeSchema::new(change_type, properties).map_err(|error| value_error(location, error))
}

fn property_from_wire(
    location: String,
    wire: PropertyWire,
) -> Result<PropertyDefinition, CustomConfigurationFormatError> {
    let key = PropertyKey::new(wire.key)
        .map_err(|error| value_error(format!("{location}.key"), error))?;
    let requirement = requirement_from_wire(&location, wire.requirement)?;
    let multiplicity = match wire.multiplicity {
        MultiplicityWire::Single => PropertyMultiplicity::Single,
        MultiplicityWire::Multiple => PropertyMultiplicity::Multiple,
    };
    PropertyDefinition::new(key, wire.description, requirement, multiplicity)
        .map_err(|error| value_error(location, error))
}

fn requirement_from_wire(
    location: &str,
    wire: RequirementWire,
) -> Result<PropertyRequirement, CustomConfigurationFormatError> {
    Ok(match wire {
        RequirementWire::Required => PropertyRequirement::Required,
        RequirementWire::Recommended => PropertyRequirement::Recommended,
        RequirementWire::Optional => PropertyRequirement::Optional,
        RequirementWire::Conditional {
            condition,
            rationale,
        } => {
            let id = ConditionId::new(condition)
                .map_err(|error| value_error(format!("{location}.condition"), error))?;
            let condition = PropertyCondition::new(id, rationale)
                .map_err(|error| value_error(format!("{location}.rationale"), error))?;
            PropertyRequirement::Conditional(condition)
        }
    })
}

fn template_from_wire(
    index: usize,
    wire: TemplateWire,
) -> Result<TemplateDefinition, CustomConfigurationFormatError> {
    let location = format!("templates[{index}]");
    let id =
        TemplateId::new(wire.id).map_err(|error| value_error(format!("{location}.id"), error))?;
    let version = TemplateVersion::new(wire.version)
        .map_err(|error| value_error(format!("{location}.version"), error))?;
    let description = Description::new(wire.description)
        .map_err(|error| value_error(format!("{location}.description"), error))?;
    let taxonomy = TaxonomyId::new(wire.taxonomy)
        .map_err(|error| value_error(format!("{location}.taxonomy"), error))?;
    let typeset = TypesetId::new(wire.typeset)
        .map_err(|error| value_error(format!("{location}.typeset"), error))?;
    Ok(TemplateDefinition::new(
        id,
        version,
        description,
        taxonomy,
        typeset,
    ))
}

fn configuration_to_wire(configuration: &CustomConfiguration) -> ConfigurationWire {
    ConfigurationWire {
        config_version: CUSTOM_CONFIGURATION_VERSION,
        taxonomies: configuration
            .taxonomies()
            .iter()
            .map(taxonomy_to_wire)
            .collect(),
        typesets: configuration
            .typesets()
            .iter()
            .map(typeset_to_wire)
            .collect(),
        templates: configuration
            .templates()
            .iter()
            .map(template_to_wire)
            .collect(),
    }
}

fn taxonomy_to_wire(taxonomy: &TaxonomyDefinition) -> TaxonomyWire {
    TaxonomyWire {
        id: taxonomy.id().to_string(),
        version: taxonomy.version().get(),
        description: taxonomy.description().to_string(),
        change_types: taxonomy
            .change_types()
            .iter()
            .map(|change_type| ChangeTypeWire {
                id: change_type.id().to_string(),
                description: change_type.description().to_string(),
            })
            .collect(),
    }
}

fn typeset_to_wire(typeset: &TypesetDefinition) -> TypesetWire {
    TypesetWire {
        taxonomy: typeset.taxonomy().to_string(),
        id: typeset.id().to_string(),
        version: typeset.version().get(),
        description: typeset.description().to_string(),
        schemas: typeset
            .schemas()
            .iter()
            .map(|schema| ChangeTypeSchemaWire {
                change_type: schema.change_type().to_string(),
                properties: schema.properties().iter().map(property_to_wire).collect(),
            })
            .collect(),
    }
}

fn property_to_wire(property: &PropertyDefinition) -> PropertyWire {
    let requirement = match property.requirement() {
        PropertyRequirement::Required => RequirementWire::Required,
        PropertyRequirement::Recommended => RequirementWire::Recommended,
        PropertyRequirement::Optional => RequirementWire::Optional,
        PropertyRequirement::Conditional(condition) => RequirementWire::Conditional {
            condition: condition.id().to_string(),
            rationale: condition.rationale().to_owned(),
        },
    };
    PropertyWire {
        key: property.key().to_string(),
        description: property.description().to_owned(),
        multiplicity: match property.multiplicity() {
            PropertyMultiplicity::Single => MultiplicityWire::Single,
            PropertyMultiplicity::Multiple => MultiplicityWire::Multiple,
        },
        requirement,
    }
}

fn template_to_wire(template: &TemplateDefinition) -> TemplateWire {
    TemplateWire {
        id: template.id().to_string(),
        version: template.version().get(),
        description: template.description().to_string(),
        taxonomy: template.taxonomy().to_string(),
        typeset: template.typeset().to_string(),
    }
}

fn value_error(
    location: impl Into<String>,
    source: impl Error + Send + Sync + 'static,
) -> CustomConfigurationFormatError {
    CustomConfigurationFormatError::Value {
        location: location.into(),
        source: Box::new(source),
    }
}

/// Failure to decode the strict global configuration format.
#[derive(Debug)]
pub enum CustomConfigurationFormatError {
    /// TOML syntax or shape is invalid.
    Toml(toml::de::Error),
    /// The file declares an unsupported format version.
    UnsupportedVersion(u16),
    /// One domain value or aggregate violates its invariants.
    Value {
        /// Stable structural location in the aggregate.
        location: String,
        /// Exact domain validation failure.
        source: Box<dyn Error + Send + Sync>,
    },
}

impl Display for CustomConfigurationFormatError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml(error) => Display::fmt(error, formatter),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported global config version {version}")
            }
            Self::Value { location, source } => write!(formatter, "invalid {location}: {source}"),
        }
    }
}

impl Error for CustomConfigurationFormatError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Toml(error) => Some(error),
            Self::Value { source, .. } => Some(source.as_ref()),
            Self::UnsupportedVersion(_) => None,
        }
    }
}

/// Failure to read or atomically replace global custom configuration.
#[derive(Debug)]
pub enum GlobalConfigurationError {
    /// An operating-system filesystem operation failed.
    Io {
        /// Attempted operation.
        operation: &'static str,
        /// Affected path.
        path: PathBuf,
        /// Underlying failure.
        source: io::Error,
    },
    /// A protected configuration path is symbolic.
    Symlink(PathBuf),
    /// The selected configuration root is not a directory.
    ExpectedDirectory(PathBuf),
    /// The configuration file path is not a regular file.
    ExpectedFile(PathBuf),
    /// Exclusive temporary-file creation encountered an existing path.
    Collision(PathBuf),
    /// Stored TOML could not be decoded safely.
    Format {
        /// Invalid configuration path.
        path: PathBuf,
        /// Exact format failure.
        source: Box<CustomConfigurationFormatError>,
    },
    /// Valid domain state could not be serialized.
    Serialization {
        /// TOML serialization failure.
        source: Box<toml::ser::Error>,
    },
    /// Stored state changed after the caller loaded its expected snapshot.
    ConcurrentChange(PathBuf),
    /// No exclusive temporary path could be reserved.
    TemporaryFileUnavailable(PathBuf),
    /// Cleanup after another failure also failed.
    Rollback {
        /// Original failure.
        original: Box<GlobalConfigurationError>,
        /// Temporary artifact that remained.
        path: PathBuf,
        /// Cleanup failure.
        source: io::Error,
    },
}

impl Display for GlobalConfigurationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "could not {operation} global configuration {}: {source}",
                path.display()
            ),
            Self::Symlink(path) => write!(
                formatter,
                "refusing symbolic link at protected global configuration path {}",
                path.display()
            ),
            Self::ExpectedDirectory(path) => {
                write!(formatter, "expected a directory at {}", path.display())
            }
            Self::ExpectedFile(path) => {
                write!(formatter, "expected a regular file at {}", path.display())
            }
            Self::Collision(path) => write!(
                formatter,
                "refusing to overwrite temporary global configuration at {}",
                path.display()
            ),
            Self::Format { path, source } => {
                write!(
                    formatter,
                    "invalid global configuration {}: {source}",
                    path.display()
                )
            }
            Self::Serialization { source } => {
                write!(
                    formatter,
                    "could not serialize global configuration: {source}"
                )
            }
            Self::ConcurrentChange(path) => write!(
                formatter,
                "global configuration changed concurrently at {}; retry the operation",
                path.display()
            ),
            Self::TemporaryFileUnavailable(path) => write!(
                formatter,
                "could not reserve a temporary global configuration in {}; retry the operation",
                path.display()
            ),
            Self::Rollback {
                original,
                path,
                source,
            } => write!(
                formatter,
                "{original}; cleanup also failed for {}: {source}",
                path.display()
            ),
        }
    }
}

impl Error for GlobalConfigurationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Format { source, .. } => Some(source.as_ref()),
            Self::Serialization { source } => Some(source.as_ref()),
            Self::Rollback { original, .. } => Some(original.as_ref()),
            Self::Symlink(_)
            | Self::ExpectedDirectory(_)
            | Self::ExpectedFile(_)
            | Self::Collision(_)
            | Self::ConcurrentChange(_)
            | Self::TemporaryFileUnavailable(_) => None,
        }
    }
}
