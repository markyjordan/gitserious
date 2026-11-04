use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use gitserious_core::{
    CommitTypeDefinition, Fingerprint, PropertyRequirement, Taxonomy, TaxonomyId,
    built_in_taxonomies,
};
use sha2::{Digest, Sha256};

use crate::LibraryTaxonomy;

/// The canonical unreleased global-configuration format.
pub(crate) const GLOBAL_CONFIG_VERSION: u16 = 1;
/// The canonical unreleased project-configuration format.
pub(crate) const PROJECT_CONFIG_VERSION: u16 = 1;

/// Whether a taxonomy is compiled in or belongs to the user library.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaxonomyOrigin {
    /// Immutable taxonomy distributed with gitserious.
    BuiltIn,
    /// Editable taxonomy stored in global configuration.
    Custom,
}

impl TaxonomyOrigin {
    /// Returns the stable presentation label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BuiltIn => "built-in",
            Self::Custom => "custom",
        }
    }
}

/// Built-in and user taxonomies merged into one validated library.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaxonomyCatalog {
    taxonomies: Vec<Taxonomy>,
}

impl TaxonomyCatalog {
    /// Builds the effective library and rejects built-in shadowing or duplicates.
    pub fn new(custom: &[Taxonomy]) -> Result<Self, ConfigurationError> {
        let mut ids = BTreeSet::new();
        for taxonomy in built_in_taxonomies() {
            ids.insert(taxonomy.id());
        }
        for taxonomy in custom {
            if !ids.insert(taxonomy.id()) {
                return Err(ConfigurationError::DuplicateOrReservedTaxonomy(
                    taxonomy.id().clone(),
                ));
            }
        }
        let mut taxonomies = built_in_taxonomies().to_vec();
        taxonomies.extend_from_slice(custom);
        Ok(Self { taxonomies })
    }

    /// Returns built-ins followed by custom taxonomies in storage order.
    #[must_use]
    pub fn taxonomies(&self) -> &[Taxonomy] {
        &self.taxonomies
    }

    /// Finds one taxonomy.
    #[must_use]
    pub fn find(&self, id: &TaxonomyId) -> Option<&Taxonomy> {
        self.taxonomies.iter().find(|taxonomy| taxonomy.id() == id)
    }

    /// Classifies a taxonomy identity.
    #[must_use]
    pub fn origin(&self, id: &TaxonomyId) -> Option<TaxonomyOrigin> {
        self.find(id).map(|_| {
            if built_in_taxonomies()
                .iter()
                .any(|taxonomy| taxonomy.id() == id)
            {
                TaxonomyOrigin::BuiltIn
            } else {
                TaxonomyOrigin::Custom
            }
        })
    }
}

/// User-level taxonomy library and default commit environment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalConfiguration {
    version: u16,
    default_taxonomy: TaxonomyId,
    available_taxonomies: Vec<TaxonomyId>,
    custom_taxonomies: Vec<LibraryTaxonomy>,
}

impl GlobalConfiguration {
    /// Creates and validates a global configuration snapshot.
    pub fn new(
        version: u16,
        default_taxonomy: TaxonomyId,
        available_taxonomies: Vec<TaxonomyId>,
        custom_taxonomies: Vec<LibraryTaxonomy>,
    ) -> Result<Self, ConfigurationError> {
        if version != GLOBAL_CONFIG_VERSION {
            return Err(ConfigurationError::UnsupportedGlobalVersion(version));
        }
        let candidate = Self {
            version,
            default_taxonomy,
            available_taxonomies,
            custom_taxonomies,
        };
        candidate.validate()?;
        Ok(candidate)
    }

    /// Returns the built-in initial environment.
    #[must_use]
    pub fn built_in_default() -> Self {
        Self {
            version: GLOBAL_CONFIG_VERSION,
            default_taxonomy: TaxonomyId::new("conventional")
                .unwrap_or_else(|_| unreachable!("compiled taxonomy identity is valid")),
            available_taxonomies: built_in_taxonomies()
                .iter()
                .map(|taxonomy| taxonomy.id().clone())
                .collect(),
            custom_taxonomies: Vec::new(),
        }
    }

    fn validate(&self) -> Result<(), ConfigurationError> {
        let catalog = self.catalog()?;
        validate_selection(
            |id| {
                catalog.find(id).is_some()
                    || self
                        .custom_taxonomies
                        .iter()
                        .any(|record| record.id() == id)
            },
            &self.default_taxonomy,
            &self.available_taxonomies,
        )
    }

    /// Returns the format version.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// Returns the global default taxonomy.
    #[must_use]
    pub const fn default_taxonomy(&self) -> &TaxonomyId {
        &self.default_taxonomy
    }

    /// Returns taxonomies available to newly resolved project policy.
    #[must_use]
    pub fn available_taxonomies(&self) -> &[TaxonomyId] {
        &self.available_taxonomies
    }

    /// Returns editable user-library taxonomies.
    #[must_use]
    pub fn custom_taxonomies(&self) -> &[LibraryTaxonomy] {
        &self.custom_taxonomies
    }

    /// Returns the merged taxonomy catalog.
    pub fn catalog(&self) -> Result<TaxonomyCatalog, ConfigurationError> {
        let mut ids = built_in_taxonomies()
            .iter()
            .map(Taxonomy::id)
            .collect::<BTreeSet<_>>();
        for record in &self.custom_taxonomies {
            if !ids.insert(record.id()) {
                return Err(ConfigurationError::DuplicateOrReservedTaxonomy(
                    record.id().clone(),
                ));
            }
        }
        TaxonomyCatalog::new(
            &self
                .custom_taxonomies
                .iter()
                .filter_map(LibraryTaxonomy::usable)
                .collect::<Vec<_>>(),
        )
    }
}

impl Default for GlobalConfiguration {
    fn default() -> Self {
        Self::built_in_default()
    }
}

/// Self-contained project policy: a default plus complete taxonomies in picker order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectConfiguration {
    default_taxonomy: TaxonomyId,
    taxonomies: Vec<Taxonomy>,
}

impl ProjectConfiguration {
    /// Validates a project document without consulting any other configuration.
    pub fn new(
        version: u16,
        default_taxonomy: TaxonomyId,
        taxonomies: Vec<Taxonomy>,
    ) -> Result<Self, ConfigurationError> {
        if version != PROJECT_CONFIG_VERSION {
            return Err(ConfigurationError::UnsupportedProjectVersion(version));
        }
        validate_ids(
            &taxonomies
                .iter()
                .map(Taxonomy::id)
                .cloned()
                .collect::<Vec<_>>(),
        )?;
        if !taxonomies
            .iter()
            .any(|taxonomy| taxonomy.id() == &default_taxonomy)
        {
            return Err(ConfigurationError::DefaultNotAvailable(default_taxonomy));
        }
        Ok(Self {
            default_taxonomy,
            taxonomies,
        })
    }

    /// A new project in `taxonomies` order, whose first taxonomy is the default.
    ///
    /// # Errors
    /// Returns an error when `taxonomies` is empty or repeats an identity.
    pub fn with_first_as_default(taxonomies: Vec<Taxonomy>) -> Result<Self, ConfigurationError> {
        let default = taxonomies
            .first()
            .ok_or(ConfigurationError::EmptyAvailableTaxonomies)?
            .id()
            .clone();
        Self::new(PROJECT_CONFIG_VERSION, default, taxonomies)
    }

    /// Returns the format version this configuration serializes as.
    #[must_use]
    pub const fn version(&self) -> u16 {
        PROJECT_CONFIG_VERSION
    }

    /// Returns the project default taxonomy.
    #[must_use]
    pub const fn default_taxonomy(&self) -> &TaxonomyId {
        &self.default_taxonomy
    }

    /// Returns the project's complete taxonomies in picker order.
    #[must_use]
    pub fn taxonomies(&self) -> &[Taxonomy] {
        &self.taxonomies
    }

    /// Finds one project-owned taxonomy.
    #[must_use]
    pub fn find(&self, id: &TaxonomyId) -> Option<&Taxonomy> {
        self.taxonomies.iter().find(|taxonomy| taxonomy.id() == id)
    }
}

/// Identity of the exact persisted project file, used to refuse stale writes.
///
/// It hashes raw bytes, so an external edit that only touches comments still counts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectRevision(Fingerprint);

impl ProjectRevision {
    /// Identifies persisted project file contents.
    #[must_use]
    pub fn of_bytes(contents: &[u8]) -> Self {
        let mut canonical = CanonicalHasher::new(b"gitserious.project-file.v1");
        canonical.bytes(contents);
        Self(canonical.finish())
    }
}

/// A loaded project configuration and the revision it was read from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredProject {
    /// The validated configuration.
    pub configuration: ProjectConfiguration,
    /// The file revision `configuration` was read from.
    pub revision: ProjectRevision,
}

fn validate_ids(ids: &[TaxonomyId]) -> Result<(), ConfigurationError> {
    if ids.is_empty() {
        return Err(ConfigurationError::EmptyAvailableTaxonomies);
    }
    let mut unique = BTreeSet::new();
    for id in ids {
        if !unique.insert(id) {
            return Err(ConfigurationError::DuplicateAvailableTaxonomy(id.clone()));
        }
    }
    Ok(())
}

/// Checks that `available` names distinct installed taxonomies and includes `default`.
pub(crate) fn validate_selection(
    installed: impl Fn(&TaxonomyId) -> bool,
    default: &TaxonomyId,
    available: &[TaxonomyId],
) -> Result<(), ConfigurationError> {
    validate_ids(available)?;
    for id in available {
        if !installed(id) {
            return Err(ConfigurationError::UnknownTaxonomy(id.clone()));
        }
    }
    if !available.contains(default) {
        return Err(ConfigurationError::DefaultNotAvailable(default.clone()));
    }
    Ok(())
}

/// A taxonomy configuration failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigurationError {
    /// A global format version is unsupported.
    UnsupportedGlobalVersion(u16),
    /// A project format version is unsupported.
    UnsupportedProjectVersion(u16),
    /// A custom taxonomy repeats or shadows an identity.
    DuplicateOrReservedTaxonomy(TaxonomyId),
    /// No taxonomies are available.
    EmptyAvailableTaxonomies,
    /// The available list repeats an identity.
    DuplicateAvailableTaxonomy(TaxonomyId),
    /// A selected taxonomy is absent from the library.
    UnknownTaxonomy(TaxonomyId),
    /// The selected default does not occur in the available list.
    DefaultNotAvailable(TaxonomyId),
}

impl Display for ConfigurationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedGlobalVersion(version) => {
                write!(formatter, "unsupported global config version {version}")
            }
            Self::UnsupportedProjectVersion(version) => {
                write!(formatter, "unsupported project config version {version}")
            }
            Self::DuplicateOrReservedTaxonomy(id) => {
                write!(
                    formatter,
                    "taxonomy \"{id}\" repeats or shadows an existing identity"
                )
            }
            Self::EmptyAvailableTaxonomies => {
                formatter.write_str("at least one taxonomy must be available")
            }
            Self::DuplicateAvailableTaxonomy(id) => {
                write!(formatter, "available taxonomies repeat \"{id}\"")
            }
            Self::UnknownTaxonomy(id) => write!(formatter, "taxonomy \"{id}\" is not installed"),
            Self::DefaultNotAvailable(id) => {
                write!(
                    formatter,
                    "default taxonomy \"{id}\" must also be available"
                )
            }
        }
    }
}

impl Error for ConfigurationError {}

/// Fingerprints a project's effective policy: its format version, default, and each taxonomy
/// in order. TOML layout and comments never reach the hash, so they cannot change it.
#[must_use]
pub fn fingerprint_project_configuration(configuration: &ProjectConfiguration) -> Fingerprint {
    let mut canonical = CanonicalHasher::new(b"gitserious.project-configuration.v1");
    canonical.u16(configuration.version());
    canonical.text(configuration.default_taxonomy().as_str());
    canonical.usize(configuration.taxonomies().len());
    for taxonomy in configuration.taxonomies() {
        canonical.text(&fingerprint_taxonomy(taxonomy).to_string());
    }
    canonical.finish()
}

/// Fingerprints every semantic field in a complete taxonomy.
#[must_use]
pub fn fingerprint_taxonomy(taxonomy: &Taxonomy) -> Fingerprint {
    let mut canonical = CanonicalHasher::new(b"gitserious.taxonomy.v1");
    canonical.text(taxonomy.id().as_str());
    canonical.u16(taxonomy.version().get());
    canonical.text(taxonomy.description().as_str());
    match taxonomy.derived_from() {
        Some(source) => {
            canonical.text(source.id().as_str());
            canonical.u16(source.version().get());
        }
        None => canonical.text("no-source"),
    }
    canonical.usize(taxonomy.commit_types().len());
    for definition in taxonomy.commit_types() {
        fingerprint_commit_type(&mut canonical, definition);
    }
    canonical.finish()
}

fn fingerprint_commit_type(canonical: &mut CanonicalHasher, definition: &CommitTypeDefinition) {
    canonical.u16(definition.schema_version().get());
    canonical.text(definition.id().as_str());
    canonical.text(definition.description());
    canonical.usize(definition.properties().len());
    for property in definition.properties() {
        canonical.text(property.key().as_str());
        canonical.text(property.description());
        match property.requirement() {
            PropertyRequirement::Required => canonical.text("required"),
            PropertyRequirement::Recommended => canonical.text("recommended"),
            PropertyRequirement::Optional => canonical.text("optional"),
            PropertyRequirement::Conditional(condition) => {
                canonical.text("conditional");
                canonical.text(condition.id().as_str());
                canonical.text(condition.rationale());
            }
        }
    }
}

struct CanonicalHasher(Sha256);

impl CanonicalHasher {
    fn new(domain: &[u8]) -> Self {
        let mut hasher = Self(Sha256::new());
        hasher.bytes(domain);
        hasher
    }

    fn bytes(&mut self, value: impl AsRef<[u8]>) {
        let value = value.as_ref();
        self.0
            .update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
        self.0.update(value);
    }

    fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn u16(&mut self, value: u16) {
        self.bytes(value.to_be_bytes());
    }

    fn usize(&mut self, value: usize) {
        self.bytes(u64::try_from(value).unwrap_or(u64::MAX).to_be_bytes());
    }

    fn finish(self) -> Fingerprint {
        Fingerprint::from_bytes(self.0.finalize().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taxonomies_without_examples_keep_their_historical_fingerprints() {
        let expected = [
            "sha256:b38521e5cdcf8217aeb7e7d715c6bc43b96ad166f92aae63cbc65ac702ebb277",
            "sha256:752a600d06daeff2728c6677c12410705628b0b642f4c994f9d8a71b23dcbf15",
            "sha256:b1f70a0e78ad40cf80aa653c0b7810349ecf9a989e876d411b64ed81234fc826",
        ];
        for (taxonomy, expected) in built_in_taxonomies().iter().zip(expected) {
            assert_eq!(fingerprint_taxonomy(taxonomy).to_string(), expected);
        }
    }

    #[test]
    fn project_fingerprints_are_pinned_and_order_sensitive() -> Result<(), ConfigurationError> {
        let conventional = TaxonomyId::new("conventional")
            .unwrap_or_else(|_| unreachable!("compiled taxonomy identity is valid"));
        let forward =
            ProjectConfiguration::new(1, conventional.clone(), built_in_taxonomies().to_vec())?;
        let mut reversed = built_in_taxonomies().to_vec();
        reversed.reverse();
        let backward = ProjectConfiguration::new(1, conventional, reversed)?;

        assert_eq!(
            fingerprint_project_configuration(&forward).to_string(),
            "sha256:5667cf608d966b85668185b258051bebae84ea3bda96443d52975131ea00b9ff",
            "the canonical project fingerprint changed; bump its domain version"
        );
        assert_ne!(
            fingerprint_project_configuration(&forward),
            fingerprint_project_configuration(&backward)
        );
        Ok(())
    }

    #[test]
    fn defaults_expose_all_built_ins_in_order() {
        let global = GlobalConfiguration::default();
        assert_eq!(global.default_taxonomy().as_str(), "conventional");
        assert_eq!(
            global
                .available_taxonomies()
                .iter()
                .map(TaxonomyId::as_str)
                .collect::<Vec<_>>(),
            ["conventional", "research", "infra-ops"]
        );
    }
}
