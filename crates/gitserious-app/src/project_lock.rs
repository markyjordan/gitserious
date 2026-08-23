use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use gitserious_core::{
    CommitMessageTemplateDefinition, CommitTypeDefinition, CommitTypeId, PropertyMultiplicity,
    PropertyRequirement, ResolvedTaxonomy, SchemaVersion, TemplateId, TemplateVersion,
    default_commit_message_template,
};
use sha2::{Digest, Sha256};

use crate::{
    ConfigurationCatalog, ConfigurationCatalogError, CustomConfiguration, Fingerprint,
    ProjectConfig,
};

/// The only generated project-lock format understood by this release.
pub const PROJECT_LOCK_VERSION: u16 = 1;

/// One resolved commit-type schema recorded in project policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedCommitType {
    id: CommitTypeId,
    schema_version: SchemaVersion,
    definition_fingerprint: Fingerprint,
}

impl ResolvedCommitType {
    /// Creates one resolved commit-type lock entry.
    #[must_use]
    pub const fn new(
        id: CommitTypeId,
        schema_version: SchemaVersion,
        definition_fingerprint: Fingerprint,
    ) -> Self {
        Self {
            id,
            schema_version,
            definition_fingerprint,
        }
    }

    /// Returns the commit-type identifier.
    #[must_use]
    pub const fn id(&self) -> &CommitTypeId {
        &self.id
    }

    /// Returns the resolved schema version.
    #[must_use]
    pub const fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    /// Returns the complete definition fingerprint.
    #[must_use]
    pub const fn definition_fingerprint(&self) -> Fingerprint {
        self.definition_fingerprint
    }
}

/// The concrete template selected by authored project configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedTemplate {
    id: TemplateId,
    version: TemplateVersion,
    fingerprint: Fingerprint,
    commit_types: Vec<ResolvedCommitType>,
}

impl ResolvedTemplate {
    /// Creates a structurally valid resolved template.
    ///
    /// # Errors
    ///
    /// Returns [`ResolvedTemplateError`] when no commit types are supplied or
    /// a commit-type identifier is repeated.
    pub fn new(
        id: TemplateId,
        version: TemplateVersion,
        fingerprint: Fingerprint,
        commit_types: Vec<ResolvedCommitType>,
    ) -> Result<Self, ResolvedTemplateError> {
        if commit_types.is_empty() {
            return Err(ResolvedTemplateError::EmptyCommitTypes);
        }
        let mut ids = BTreeSet::new();
        for commit_type in &commit_types {
            if !ids.insert(commit_type.id()) {
                return Err(ResolvedTemplateError::DuplicateCommitType(
                    commit_type.id().clone(),
                ));
            }
        }
        Ok(Self {
            id,
            version,
            fingerprint,
            commit_types,
        })
    }

    /// Returns the concrete template identifier.
    #[must_use]
    pub const fn id(&self) -> &TemplateId {
        &self.id
    }

    /// Returns the concrete template version.
    #[must_use]
    pub const fn version(&self) -> TemplateVersion {
        self.version
    }

    /// Returns the ordered template fingerprint.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    /// Returns resolved commit types in template order.
    #[must_use]
    pub fn commit_types(&self) -> &[ResolvedCommitType] {
        &self.commit_types
    }
}

/// A structural invariant violation in a resolved template.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolvedTemplateError {
    /// The resolved template contains no commit types.
    EmptyCommitTypes,
    /// Two resolved definitions use the same identifier.
    DuplicateCommitType(CommitTypeId),
}

impl Display for ResolvedTemplateError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCommitTypes => {
                formatter.write_str("resolved template must contain at least one commit type")
            }
            Self::DuplicateCommitType(id) => {
                write!(formatter, "resolved template repeats commit type {id:?}")
            }
        }
    }
}

impl Error for ResolvedTemplateError {}

/// Generated, reproducible repository-local policy state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectLock {
    version: u16,
    config_fingerprint: Fingerprint,
    template_reference: TemplateId,
    resolved_template: ResolvedTemplate,
}

impl ProjectLock {
    /// Rehydrates a supported project lock.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectLockError`] when the lock version is unsupported.
    pub fn new(
        version: u16,
        config_fingerprint: Fingerprint,
        template_reference: TemplateId,
        resolved_template: ResolvedTemplate,
    ) -> Result<Self, ProjectLockError> {
        if version != PROJECT_LOCK_VERSION {
            return Err(ProjectLockError::UnsupportedVersion(version));
        }
        Ok(Self {
            version,
            config_fingerprint,
            template_reference,
            resolved_template,
        })
    }

    /// Returns the lock format version.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// Returns the canonical authored-configuration fingerprint.
    #[must_use]
    pub const fn config_fingerprint(&self) -> Fingerprint {
        self.config_fingerprint
    }

    /// Returns the authored template channel or identifier.
    #[must_use]
    pub const fn template_reference(&self) -> &TemplateId {
        &self.template_reference
    }

    /// Returns the concrete resolved policy template.
    #[must_use]
    pub const fn resolved_template(&self) -> &ResolvedTemplate {
        &self.resolved_template
    }
}

/// An unsupported generated project-lock format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectLockError {
    /// The file declares a version this binary cannot interpret.
    UnsupportedVersion(u16),
}

impl Display for ProjectLockError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported project lock version {version}")
            }
        }
    }
}

impl Error for ProjectLockError {}

/// Failure to resolve an authored project policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolveProjectPolicyError {
    /// No installed template matches the authored reference.
    UnknownTemplate(TemplateId),
    /// The installed template violated resolved-policy invariants.
    InvalidResolvedTemplate(ResolvedTemplateError),
    /// The effective catalog containing the authored template is invalid.
    Catalog(ConfigurationCatalogError),
}

impl Display for ResolveProjectPolicyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTemplate(id) => {
                write!(formatter, "template {id:?} is not installed")
            }
            Self::InvalidResolvedTemplate(error) => Display::fmt(error, formatter),
            Self::Catalog(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for ResolveProjectPolicyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidResolvedTemplate(error) => Some(error),
            Self::Catalog(error) => Some(error),
            Self::UnknownTemplate(_) => None,
        }
    }
}

/// Resolves authored configuration into an exact, reproducible project lock.
///
/// The authored template reference is resolved through one effective catalog,
/// so built-in and custom templates follow the identical path. The
/// recorded fingerprints cover every semantic field of the joined policy.
///
/// The built-in moving channel keeps its historical lock identity: selecting
/// it records the compiled-in commit-message template rather than the channel
/// placeholder, preserving previously generated locks byte for byte.
///
/// # Errors
///
/// Returns [`ResolveProjectPolicyError`] when the requested template is not
/// installed, the catalog is invalid, or resolved template invariants fail.
pub fn resolve_project_lock(
    config: &ProjectConfig,
) -> Result<ProjectLock, ResolveProjectPolicyError> {
    let catalog =
        ConfigurationCatalog::new(config.custom()).map_err(ResolveProjectPolicyError::Catalog)?;
    let reference = config.active_template();
    let resolved = catalog.resolve(reference).map_err(|error| match error {
        ConfigurationCatalogError::UnknownTemplate(id) => {
            ResolveProjectPolicyError::UnknownTemplate(id)
        }
        other => ResolveProjectPolicyError::Catalog(other),
    })?;
    let Some(template) = catalog.find_template(reference) else {
        return Err(ResolveProjectPolicyError::Catalog(
            ConfigurationCatalogError::UnknownTemplate(reference.clone()),
        ));
    };
    let commit_types = resolved
        .change_types()
        .iter()
        .map(|change_type| {
            let definition = change_type.commit_type_definition();
            ResolvedCommitType::new(
                definition.id().clone(),
                definition.schema_version(),
                fingerprint_commit_type_definition(&definition),
            )
        })
        .collect::<Vec<_>>();
    let built_in = gitserious_core::built_in_configuration();
    let (identity_id, identity_version) = if built_in.template().id() == reference {
        let message_template = default_commit_message_template();
        (message_template.id().clone(), message_template.version())
    } else {
        (template.id().clone(), template.version())
    };
    let template_fingerprint =
        fingerprint_resolved_template(&identity_id, identity_version, &commit_types);
    let resolved_template = ResolvedTemplate::new(
        identity_id,
        identity_version,
        template_fingerprint,
        commit_types,
    )
    .map_err(ResolveProjectPolicyError::InvalidResolvedTemplate)?;

    Ok(ProjectLock {
        version: PROJECT_LOCK_VERSION,
        config_fingerprint: fingerprint_project_config(config),
        template_reference: reference.clone(),
        resolved_template,
    })
}

/// Fingerprints normalized authored configuration independently of TOML layout.
#[must_use]
pub fn fingerprint_project_config(config: &ProjectConfig) -> Fingerprint {
    let mut canonical = CanonicalHasher::new(b"gitserious.project-config.v1");
    canonical.u16(config.version());
    canonical.text(config.active_template().as_str());
    fingerprint_custom_configuration(&mut canonical, config.custom());
    canonical.finish()
}

fn fingerprint_custom_configuration(canonical: &mut CanonicalHasher, custom: &CustomConfiguration) {
    canonical.usize(custom.taxonomies().len());
    for taxonomy in custom.taxonomies() {
        canonical.text(taxonomy.id().as_str());
        canonical.u16(taxonomy.version().get());
        canonical.text(taxonomy.description().as_str());
        canonical.usize(taxonomy.change_types().len());
        for change_type in taxonomy.change_types() {
            canonical.text(change_type.id().as_str());
            canonical.text(change_type.description().as_str());
        }
    }

    canonical.usize(custom.typesets().len());
    for typeset in custom.typesets() {
        canonical.text(typeset.taxonomy().as_str());
        canonical.text(typeset.id().as_str());
        canonical.u16(typeset.version().get());
        canonical.text(typeset.description().as_str());
        canonical.usize(typeset.schemas().len());
        for schema in typeset.schemas() {
            canonical.text(schema.change_type().as_str());
            canonical.usize(schema.properties().len());
            for property in schema.properties() {
                fingerprint_property(canonical, property);
            }
        }
    }

    canonical.usize(custom.templates().len());
    for template in custom.templates() {
        canonical.text(template.id().as_str());
        canonical.u16(template.version().get());
        canonical.text(template.description().as_str());
        canonical.text(template.taxonomy().as_str());
        canonical.text(template.typeset().as_str());
    }
}

fn fingerprint_property(
    canonical: &mut CanonicalHasher,
    property: &gitserious_core::PropertyDefinition,
) {
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

/// Fingerprints every semantic field in one ordered commit-type definition.
#[must_use]
pub fn fingerprint_commit_type_definition(definition: &CommitTypeDefinition) -> Fingerprint {
    let mut canonical = CanonicalHasher::new(b"gitserious.commit-type-definition.v1");
    canonical.u16(definition.schema_version().get());
    canonical.text(definition.id().as_str());
    canonical.text(definition.description());
    canonical.usize(definition.properties().len());
    for property in definition.properties() {
        fingerprint_property(&mut canonical, property);
    }
    canonical.finish()
}

/// Fingerprints a template's identity, version, and ordered definitions.
#[must_use]
pub fn fingerprint_commit_message_template(
    template: &CommitMessageTemplateDefinition,
) -> Fingerprint {
    let commit_types = template
        .commit_types()
        .iter()
        .map(|definition| {
            ResolvedCommitType::new(
                definition.id().clone(),
                definition.schema_version(),
                fingerprint_commit_type_definition(definition),
            )
        })
        .collect::<Vec<_>>();
    fingerprint_template(template, &commit_types)
}

/// Fingerprints every semantic field in a fully joined resolved taxonomy.
#[must_use]
pub fn fingerprint_resolved_taxonomy(resolved: &ResolvedTaxonomy) -> Fingerprint {
    let mut canonical = CanonicalHasher::new(b"gitserious.resolved-taxonomy.v1");
    canonical.text(resolved.template_id().as_str());
    canonical.u16(resolved.template_version().get());
    canonical.text(resolved.template_description().as_str());
    canonical.text(resolved.taxonomy_id().as_str());
    canonical.u16(resolved.taxonomy_version().get());
    canonical.text(resolved.taxonomy_description().as_str());
    canonical.text(resolved.typeset_id().as_str());
    canonical.u16(resolved.typeset_version().get());
    canonical.text(resolved.typeset_description().as_str());
    canonical.usize(resolved.change_types().len());
    for change_type in resolved.change_types() {
        canonical.text(change_type.id().as_str());
        canonical.text(change_type.description().as_str());
        canonical.usize(change_type.properties().len());
        for property in change_type.properties() {
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
    canonical.finish()
}

fn fingerprint_template(
    template: &CommitMessageTemplateDefinition,
    commit_types: &[ResolvedCommitType],
) -> Fingerprint {
    let mut canonical = CanonicalHasher::new(b"gitserious.commit-message-template.v1");
    canonical.text(template.id().as_str());
    canonical.u16(template.version().get());
    canonical.usize(commit_types.len());
    for commit_type in commit_types {
        canonical.text(commit_type.id().as_str());
        canonical.bytes(commit_type.definition_fingerprint().as_bytes());
    }
    canonical.finish()
}

fn fingerprint_resolved_template(
    identity_id: &TemplateId,
    identity_version: TemplateVersion,
    commit_types: &[ResolvedCommitType],
) -> Fingerprint {
    let mut canonical = CanonicalHasher::new(b"gitserious.commit-message-template.v1");
    canonical.text(identity_id.as_str());
    canonical.u16(identity_version.get());
    canonical.usize(commit_types.len());
    for commit_type in commit_types {
        canonical.text(commit_type.id().as_str());
        canonical.bytes(commit_type.definition_fingerprint().as_bytes());
    }
    canonical.finish()
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
