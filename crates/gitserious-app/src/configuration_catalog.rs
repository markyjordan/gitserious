use std::error::Error;
use std::fmt::{self, Display, Formatter};

use gitserious_core::{
    ResolveTaxonomyError, ResolvedTaxonomy, TaxonomyDefinition, TaxonomyId, TemplateDefinition,
    TemplateId, TypesetDefinition, TypesetId, built_in_configuration,
};

use crate::CustomConfiguration;

/// Built-in and custom definitions validated as one effective catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigurationCatalog {
    taxonomies: Vec<TaxonomyDefinition>,
    typesets: Vec<TypesetDefinition>,
    templates: Vec<TemplateDefinition>,
}

impl ConfigurationCatalog {
    /// Merges built-in and custom definitions without permitting shadowing.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationCatalogError`] for reserved identities, dangling
    /// references, incompatible typesets, or incomplete type coverage.
    pub fn new(custom: &CustomConfiguration) -> Result<Self, ConfigurationCatalogError> {
        let built_in = built_in_configuration();
        if let Some(taxonomy) = custom
            .taxonomies()
            .iter()
            .find(|value| built_in.find_taxonomy(value.id()).is_some())
        {
            return Err(ConfigurationCatalogError::ReservedTaxonomy(
                taxonomy.id().clone(),
            ));
        }
        if let Some(typeset) = custom.typesets().iter().find(|value| {
            built_in
                .find_typeset(value.taxonomy(), value.id())
                .is_some()
        }) {
            return Err(ConfigurationCatalogError::ReservedTypeset {
                taxonomy: typeset.taxonomy().clone(),
                typeset: typeset.id().clone(),
            });
        }
        if let Some(template) = custom
            .templates()
            .iter()
            .find(|value| built_in.find_template(value.id()).is_some())
        {
            return Err(ConfigurationCatalogError::ReservedTemplate(
                template.id().clone(),
            ));
        }

        let mut catalog = Self {
            taxonomies: built_in
                .taxonomies()
                .iter()
                .chain(custom.taxonomies())
                .cloned()
                .collect(),
            typesets: built_in
                .typesets()
                .iter()
                .chain(custom.typesets())
                .cloned()
                .collect(),
            templates: built_in
                .templates()
                .iter()
                .chain(custom.templates())
                .cloned()
                .collect(),
        };
        catalog
            .taxonomies
            .sort_by(|left, right| left.id().cmp(right.id()));
        catalog.typesets.sort_by(|left, right| {
            (left.taxonomy(), left.id()).cmp(&(right.taxonomy(), right.id()))
        });
        catalog
            .templates
            .sort_by(|left, right| left.id().cmp(right.id()));

        for typeset in &catalog.typesets {
            let Some(taxonomy) = catalog.find_taxonomy(typeset.taxonomy()) else {
                return Err(ConfigurationCatalogError::UnknownTypesetTaxonomy {
                    taxonomy: typeset.taxonomy().clone(),
                    typeset: typeset.id().clone(),
                });
            };
            validate_typeset(taxonomy, typeset)?;
        }
        for template in &catalog.templates {
            catalog.resolve(template.id())?;
        }
        Ok(catalog)
    }

    /// Returns all effective taxonomies in identifier order.
    #[must_use]
    pub fn taxonomies(&self) -> &[TaxonomyDefinition] {
        &self.taxonomies
    }

    /// Returns all effective typesets in taxonomy/identifier order.
    #[must_use]
    pub fn typesets(&self) -> &[TypesetDefinition] {
        &self.typesets
    }

    /// Returns all effective templates in identifier order.
    #[must_use]
    pub fn templates(&self) -> &[TemplateDefinition] {
        &self.templates
    }

    /// Finds one effective taxonomy.
    #[must_use]
    pub fn find_taxonomy(&self, id: &TaxonomyId) -> Option<&TaxonomyDefinition> {
        self.taxonomies.iter().find(|taxonomy| taxonomy.id() == id)
    }

    /// Finds one effective taxonomy-scoped typeset.
    #[must_use]
    pub fn find_typeset(
        &self,
        taxonomy: &TaxonomyId,
        typeset: &TypesetId,
    ) -> Option<&TypesetDefinition> {
        self.typesets
            .iter()
            .find(|definition| definition.taxonomy() == taxonomy && definition.id() == typeset)
    }

    /// Finds one effective template.
    #[must_use]
    pub fn find_template(&self, id: &TemplateId) -> Option<&TemplateDefinition> {
        self.templates.iter().find(|template| template.id() == id)
    }

    /// Resolves a template through the same path for built-in and custom data.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationCatalogError`] when the template or one of its
    /// selected definitions is unavailable or incompatible.
    pub fn resolve(&self, id: &TemplateId) -> Result<ResolvedTaxonomy, ConfigurationCatalogError> {
        let template = self
            .find_template(id)
            .ok_or_else(|| ConfigurationCatalogError::UnknownTemplate(id.clone()))?;
        let taxonomy = self.find_taxonomy(template.taxonomy()).ok_or_else(|| {
            ConfigurationCatalogError::UnknownTemplateTaxonomy {
                template: template.id().clone(),
                taxonomy: template.taxonomy().clone(),
            }
        })?;
        let typeset = self
            .find_typeset(template.taxonomy(), template.typeset())
            .ok_or_else(|| ConfigurationCatalogError::UnknownTemplateTypeset {
                template: template.id().clone(),
                taxonomy: template.taxonomy().clone(),
                typeset: template.typeset().clone(),
            })?;
        ResolvedTaxonomy::resolve(template, taxonomy, typeset)
            .map_err(ConfigurationCatalogError::Resolution)
    }
}

/// The only product-level ownership distinction for configuration definitions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigurationOrigin {
    /// Immutable definitions compiled into gitserious.
    BuiltIn,
    /// Editable definitions stored in global or project configuration.
    Custom,
}

impl ConfigurationOrigin {
    /// Returns the stable presentation label for this origin.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BuiltIn => "built-in",
            Self::Custom => "custom",
        }
    }
}

/// Classifies one taxonomy identifier as built-in or custom.
#[must_use]
pub fn taxonomy_origin(id: &TaxonomyId) -> ConfigurationOrigin {
    if built_in_configuration().find_taxonomy(id).is_some() {
        ConfigurationOrigin::BuiltIn
    } else {
        ConfigurationOrigin::Custom
    }
}

/// Classifies one taxonomy-scoped typeset identifier as built-in or custom.
#[must_use]
pub fn typeset_origin(taxonomy: &TaxonomyId, typeset: &TypesetId) -> ConfigurationOrigin {
    if built_in_configuration()
        .find_typeset(taxonomy, typeset)
        .is_some()
    {
        ConfigurationOrigin::BuiltIn
    } else {
        ConfigurationOrigin::Custom
    }
}

/// Classifies one template identifier as built-in or custom.
#[must_use]
pub fn template_origin(id: &TemplateId) -> ConfigurationOrigin {
    if built_in_configuration().find_template(id).is_some() {
        ConfigurationOrigin::BuiltIn
    } else {
        ConfigurationOrigin::Custom
    }
}

/// Returns the effective catalog containing only built-in definitions.
///
/// # Errors
///
/// Returns [`ConfigurationCatalogError`] when the compiled-in built-in
/// definitions violate catalog invariants, which indicates a release defect.
pub fn built_in_effective_catalog() -> Result<ConfigurationCatalog, ConfigurationCatalogError> {
    ConfigurationCatalog::new(&CustomConfiguration::default())
}

fn validate_typeset(
    taxonomy: &TaxonomyDefinition,
    typeset: &TypesetDefinition,
) -> Result<(), ConfigurationCatalogError> {
    for change_type in taxonomy.change_types() {
        if !typeset
            .schemas()
            .iter()
            .any(|schema| schema.change_type() == change_type.id())
        {
            return Err(ConfigurationCatalogError::Resolution(
                ResolveTaxonomyError::MissingTypesetChangeType(change_type.id().clone()),
            ));
        }
    }
    for schema in typeset.schemas() {
        if !taxonomy
            .change_types()
            .iter()
            .any(|change_type| change_type.id() == schema.change_type())
        {
            return Err(ConfigurationCatalogError::Resolution(
                ResolveTaxonomyError::UnknownTypesetChangeType(schema.change_type().clone()),
            ));
        }
    }
    Ok(())
}

/// An invalid effective configuration catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigurationCatalogError {
    /// A custom taxonomy shadows a built-in identity.
    ReservedTaxonomy(TaxonomyId),
    /// A custom typeset shadows a built-in taxonomy-scoped identity.
    ReservedTypeset {
        /// Containing taxonomy.
        taxonomy: TaxonomyId,
        /// Reserved typeset.
        typeset: TypesetId,
    },
    /// A custom template shadows a built-in identity.
    ReservedTemplate(TemplateId),
    /// A typeset references an unavailable taxonomy.
    UnknownTypesetTaxonomy {
        /// Missing taxonomy.
        taxonomy: TaxonomyId,
        /// Referencing typeset.
        typeset: TypesetId,
    },
    /// A template references an unavailable taxonomy.
    UnknownTemplateTaxonomy {
        /// Referencing template.
        template: TemplateId,
        /// Missing taxonomy.
        taxonomy: TaxonomyId,
    },
    /// A template references an unavailable compatible typeset.
    UnknownTemplateTypeset {
        /// Referencing template.
        template: TemplateId,
        /// Selected taxonomy.
        taxonomy: TaxonomyId,
        /// Missing typeset.
        typeset: TypesetId,
    },
    /// The requested template does not exist.
    UnknownTemplate(TemplateId),
    /// Joined definitions violate taxonomy/typeset invariants.
    Resolution(ResolveTaxonomyError),
}

impl Display for ConfigurationCatalogError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReservedTaxonomy(id) => {
                write!(formatter, "taxonomy {id:?} is reserved by gitserious")
            }
            Self::ReservedTypeset { taxonomy, typeset } => write!(
                formatter,
                "typeset {taxonomy:?}/{typeset:?} is reserved by gitserious"
            ),
            Self::ReservedTemplate(id) => {
                write!(formatter, "template {id:?} is reserved by gitserious")
            }
            Self::UnknownTypesetTaxonomy { taxonomy, typeset } => write!(
                formatter,
                "typeset {typeset:?} references unavailable taxonomy {taxonomy:?}"
            ),
            Self::UnknownTemplateTaxonomy { template, taxonomy } => write!(
                formatter,
                "template {template:?} references unavailable taxonomy {taxonomy:?}"
            ),
            Self::UnknownTemplateTypeset {
                template,
                taxonomy,
                typeset,
            } => write!(
                formatter,
                "template {template:?} references unavailable typeset {taxonomy:?}/{typeset:?}"
            ),
            Self::UnknownTemplate(id) => write!(formatter, "template {id:?} is unavailable"),
            Self::Resolution(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for ConfigurationCatalogError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Resolution(error) => Some(error),
            Self::ReservedTaxonomy(_)
            | Self::ReservedTypeset { .. }
            | Self::ReservedTemplate(_)
            | Self::UnknownTypesetTaxonomy { .. }
            | Self::UnknownTemplateTaxonomy { .. }
            | Self::UnknownTemplateTypeset { .. }
            | Self::UnknownTemplate(_) => None,
        }
    }
}
