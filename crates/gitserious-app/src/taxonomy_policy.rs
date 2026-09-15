use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use gitserious_core::{
    CommitTypeDefinition, Fingerprint, PropertyMultiplicity, PropertyRequirement, Taxonomy,
    TaxonomyId, built_in_taxonomies,
};
use sha2::{Digest, Sha256};

/// The canonical unreleased global-configuration format.
pub const GLOBAL_CONFIG_VERSION: u16 = 1;
/// The canonical unreleased project-configuration format.
pub const PROJECT_CONFIG_VERSION: u16 = 1;
/// The canonical unreleased portable-lock format.
pub const PROJECT_LOCK_VERSION: u16 = 1;

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
    custom_taxonomies: Vec<Taxonomy>,
}

impl GlobalConfiguration {
    /// Creates and validates a global configuration snapshot.
    pub fn new(
        version: u16,
        default_taxonomy: TaxonomyId,
        available_taxonomies: Vec<TaxonomyId>,
        custom_taxonomies: Vec<Taxonomy>,
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
        validate_selection(&catalog, &self.default_taxonomy, &self.available_taxonomies)
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
    pub fn custom_taxonomies(&self) -> &[Taxonomy] {
        &self.custom_taxonomies
    }

    /// Returns the merged taxonomy catalog.
    pub fn catalog(&self) -> Result<TaxonomyCatalog, ConfigurationError> {
        TaxonomyCatalog::new(&self.custom_taxonomies)
    }
}

impl Default for GlobalConfiguration {
    fn default() -> Self {
        Self::built_in_default()
    }
}

/// Optional repository-local overrides of the global commit environment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectConfig {
    version: u16,
    default_taxonomy: Option<TaxonomyId>,
    available_taxonomies: Option<Vec<TaxonomyId>>,
}

impl ProjectConfig {
    /// Creates a project configuration. References are resolved with global state later.
    pub fn new(
        version: u16,
        default_taxonomy: Option<TaxonomyId>,
        available_taxonomies: Option<Vec<TaxonomyId>>,
    ) -> Result<Self, ConfigurationError> {
        if version != PROJECT_CONFIG_VERSION {
            return Err(ConfigurationError::UnsupportedProjectVersion(version));
        }
        if let Some(available) = &available_taxonomies {
            validate_ids(available)?;
        }
        Ok(Self {
            version,
            default_taxonomy,
            available_taxonomies,
        })
    }

    /// Creates a project that inherits both global fields.
    #[must_use]
    pub const fn inherited() -> Self {
        Self {
            version: PROJECT_CONFIG_VERSION,
            default_taxonomy: None,
            available_taxonomies: None,
        }
    }

    /// Returns the format version.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// Returns the optional project default override.
    #[must_use]
    pub const fn default_taxonomy(&self) -> Option<&TaxonomyId> {
        self.default_taxonomy.as_ref()
    }

    /// Returns the optional project available-list override.
    #[must_use]
    pub fn available_taxonomies(&self) -> Option<&[TaxonomyId]> {
        self.available_taxonomies.as_deref()
    }
}

/// Resolved selection and complete taxonomy definitions ready to lock.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectiveConfiguration {
    default_taxonomy: TaxonomyId,
    taxonomies: Vec<Taxonomy>,
}

impl EffectiveConfiguration {
    /// Returns the effective default taxonomy.
    #[must_use]
    pub const fn default_taxonomy(&self) -> &TaxonomyId {
        &self.default_taxonomy
    }

    /// Returns available taxonomies in picker order.
    #[must_use]
    pub fn taxonomies(&self) -> &[Taxonomy] {
        &self.taxonomies
    }
}

/// Resolves project overrides field-by-field against global configuration.
pub fn resolve_effective_configuration(
    global: &GlobalConfiguration,
    project: &ProjectConfig,
) -> Result<EffectiveConfiguration, ConfigurationError> {
    let catalog = global.catalog()?;
    let default = project
        .default_taxonomy()
        .unwrap_or_else(|| global.default_taxonomy())
        .clone();
    let available = project
        .available_taxonomies()
        .unwrap_or_else(|| global.available_taxonomies());
    validate_selection(&catalog, &default, available)?;
    let taxonomies = available
        .iter()
        .map(|id| {
            catalog
                .find(id)
                .cloned()
                .ok_or_else(|| ConfigurationError::UnknownTaxonomy(id.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(EffectiveConfiguration {
        default_taxonomy: default,
        taxonomies,
    })
}

/// Portable repository policy used directly by commit authoring.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectLock {
    version: u16,
    config_fingerprint: Fingerprint,
    default_taxonomy: TaxonomyId,
    taxonomies: Vec<Taxonomy>,
}

impl ProjectLock {
    /// Creates a validated portable policy snapshot.
    pub fn new(
        version: u16,
        config_fingerprint: Fingerprint,
        default_taxonomy: TaxonomyId,
        taxonomies: Vec<Taxonomy>,
    ) -> Result<Self, ConfigurationError> {
        if version != PROJECT_LOCK_VERSION {
            return Err(ConfigurationError::UnsupportedLockVersion(version));
        }
        let mut ids = BTreeSet::new();
        for taxonomy in &taxonomies {
            if !ids.insert(taxonomy.id()) {
                return Err(ConfigurationError::DuplicateAvailableTaxonomy(
                    taxonomy.id().clone(),
                ));
            }
        }
        if taxonomies.is_empty() {
            return Err(ConfigurationError::EmptyAvailableTaxonomies);
        }
        if !taxonomies
            .iter()
            .any(|taxonomy| taxonomy.id() == &default_taxonomy)
        {
            return Err(ConfigurationError::DefaultNotAvailable(default_taxonomy));
        }
        Ok(Self {
            version,
            config_fingerprint,
            default_taxonomy,
            taxonomies,
        })
    }

    /// Captures effective configuration for one project.
    pub fn capture(
        project: &ProjectConfig,
        effective: &EffectiveConfiguration,
    ) -> Result<Self, ConfigurationError> {
        Self::new(
            PROJECT_LOCK_VERSION,
            fingerprint_project_config(project),
            effective.default_taxonomy.clone(),
            effective.taxonomies.clone(),
        )
    }

    /// Returns the lock format version.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// Returns the authored-project fingerprint.
    #[must_use]
    pub const fn config_fingerprint(&self) -> Fingerprint {
        self.config_fingerprint
    }

    /// Returns the pinned default taxonomy.
    #[must_use]
    pub const fn default_taxonomy(&self) -> &TaxonomyId {
        &self.default_taxonomy
    }

    /// Returns complete pinned taxonomies in picker order.
    #[must_use]
    pub fn taxonomies(&self) -> &[Taxonomy] {
        &self.taxonomies
    }

    /// Returns whether this lock belongs to the supplied authored configuration.
    #[must_use]
    pub fn matches(&self, config: &ProjectConfig) -> bool {
        self.config_fingerprint == fingerprint_project_config(config)
    }
}

/// Repository-local state observed before initialization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectState {
    /// Neither project artifact exists.
    Absent,
    /// Authored configuration exists without its lock.
    ConfigOnly(ProjectConfig),
    /// Both project artifacts exist.
    Initialized {
        /// Authored configuration.
        config: ProjectConfig,
        /// Portable resolved policy.
        lock: ProjectLock,
    },
    /// A lock exists without authored configuration.
    LockOnly,
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

fn validate_selection(
    catalog: &TaxonomyCatalog,
    default: &TaxonomyId,
    available: &[TaxonomyId],
) -> Result<(), ConfigurationError> {
    validate_ids(available)?;
    for id in available {
        if catalog.find(id).is_none() {
            return Err(ConfigurationError::UnknownTaxonomy(id.clone()));
        }
    }
    if !available.contains(default) {
        return Err(ConfigurationError::DefaultNotAvailable(default.clone()));
    }
    Ok(())
}

/// A taxonomy configuration or portable-lock failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigurationError {
    /// A global format version is unsupported.
    UnsupportedGlobalVersion(u16),
    /// A project format version is unsupported.
    UnsupportedProjectVersion(u16),
    /// A lock format version is unsupported.
    UnsupportedLockVersion(u16),
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
            Self::UnsupportedLockVersion(version) => {
                write!(formatter, "unsupported project lock version {version}")
            }
            Self::DuplicateOrReservedTaxonomy(id) => {
                write!(
                    formatter,
                    "taxonomy {id:?} repeats or shadows an existing identity"
                )
            }
            Self::EmptyAvailableTaxonomies => {
                formatter.write_str("at least one taxonomy must be available")
            }
            Self::DuplicateAvailableTaxonomy(id) => {
                write!(formatter, "available taxonomies repeat {id:?}")
            }
            Self::UnknownTaxonomy(id) => write!(formatter, "taxonomy {id:?} is not installed"),
            Self::DefaultNotAvailable(id) => {
                write!(formatter, "default taxonomy {id:?} must also be available")
            }
        }
    }
}

impl Error for ConfigurationError {}

/// Fingerprints authored project overrides independently of TOML layout.
#[must_use]
pub fn fingerprint_project_config(config: &ProjectConfig) -> Fingerprint {
    let mut canonical = CanonicalHasher::new(b"gitserious.project-config.taxonomy.v1");
    canonical.u16(config.version());
    match config.default_taxonomy() {
        Some(id) => {
            canonical.text("override");
            canonical.text(id.as_str());
        }
        None => canonical.text("inherit"),
    }
    match config.available_taxonomies() {
        Some(ids) => {
            canonical.text("override");
            canonical.usize(ids.len());
            for id in ids {
                canonical.text(id.as_str());
            }
        }
        None => canonical.text("inherit"),
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
        canonical.text(match property.multiplicity() {
            PropertyMultiplicity::Single => "single",
            PropertyMultiplicity::Multiple => "multiple",
        });
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

    #[test]
    fn project_fields_inherit_independently() {
        let global = GlobalConfiguration::default();
        let project = ProjectConfig::new(
            1,
            Some(TaxonomyId::new("research").unwrap_or_else(|_| unreachable!())),
            None,
        )
        .unwrap_or_else(|_| unreachable!());
        let effective =
            resolve_effective_configuration(&global, &project).unwrap_or_else(|_| unreachable!());
        assert_eq!(effective.default_taxonomy().as_str(), "research");
        assert_eq!(effective.taxonomies().len(), 3);
    }
}
