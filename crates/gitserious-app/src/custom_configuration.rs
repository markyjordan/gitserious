use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use gitserious_core::{
    TaxonomyDefinition, TaxonomyId, TemplateDefinition, TemplateId, TypesetDefinition, TypesetId,
};

/// The only custom-configuration format understood by this release.
pub const CUSTOM_CONFIGURATION_VERSION: u16 = 1;

/// Editable taxonomy, typeset, and template definitions for one scope.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CustomConfiguration {
    taxonomies: Vec<TaxonomyDefinition>,
    typesets: Vec<TypesetDefinition>,
    templates: Vec<TemplateDefinition>,
}

impl CustomConfiguration {
    /// Creates a structurally unique custom configuration snapshot.
    ///
    /// Cross-definition references are checked by the effective catalog.
    ///
    /// # Errors
    ///
    /// Returns [`CustomConfigurationError`] when a top-level identity is
    /// repeated.
    pub fn new(
        taxonomies: Vec<TaxonomyDefinition>,
        typesets: Vec<TypesetDefinition>,
        templates: Vec<TemplateDefinition>,
    ) -> Result<Self, CustomConfigurationError> {
        let mut taxonomy_ids = BTreeSet::new();
        for taxonomy in &taxonomies {
            if !taxonomy_ids.insert(taxonomy.id()) {
                return Err(CustomConfigurationError::DuplicateTaxonomy(
                    taxonomy.id().clone(),
                ));
            }
        }

        let mut typeset_ids = BTreeSet::new();
        for typeset in &typesets {
            let key = (typeset.taxonomy(), typeset.id());
            if !typeset_ids.insert(key) {
                return Err(CustomConfigurationError::DuplicateTypeset {
                    taxonomy: typeset.taxonomy().clone(),
                    typeset: typeset.id().clone(),
                });
            }
        }

        let mut template_ids = BTreeSet::new();
        for template in &templates {
            if !template_ids.insert(template.id()) {
                return Err(CustomConfigurationError::DuplicateTemplate(
                    template.id().clone(),
                ));
            }
        }

        let mut configuration = Self {
            taxonomies,
            typesets,
            templates,
        };
        configuration.sort();
        Ok(configuration)
    }

    /// Returns custom taxonomies in snapshot order.
    #[must_use]
    pub fn taxonomies(&self) -> &[TaxonomyDefinition] {
        &self.taxonomies
    }

    /// Returns custom typesets in snapshot order.
    #[must_use]
    pub fn typesets(&self) -> &[TypesetDefinition] {
        &self.typesets
    }

    /// Returns custom templates in snapshot order.
    #[must_use]
    pub fn templates(&self) -> &[TemplateDefinition] {
        &self.templates
    }

    pub(crate) fn taxonomies_mut(&mut self) -> &mut Vec<TaxonomyDefinition> {
        &mut self.taxonomies
    }

    pub(crate) fn typesets_mut(&mut self) -> &mut Vec<TypesetDefinition> {
        &mut self.typesets
    }

    pub(crate) fn templates_mut(&mut self) -> &mut Vec<TemplateDefinition> {
        &mut self.templates
    }

    pub(crate) fn sort(&mut self) {
        self.taxonomies
            .sort_by(|left, right| left.id().cmp(right.id()));
        self.typesets.sort_by(|left, right| {
            (left.taxonomy(), left.id()).cmp(&(right.taxonomy(), right.id()))
        });
        self.templates
            .sort_by(|left, right| left.id().cmp(right.id()));
    }
}

/// A duplicate top-level definition in custom configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CustomConfigurationError {
    /// A taxonomy identity is repeated.
    DuplicateTaxonomy(TaxonomyId),
    /// A taxonomy-scoped typeset identity is repeated.
    DuplicateTypeset {
        /// Containing taxonomy.
        taxonomy: TaxonomyId,
        /// Repeated typeset.
        typeset: TypesetId,
    },
    /// A template identity is repeated.
    DuplicateTemplate(TemplateId),
}

impl Display for CustomConfigurationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateTaxonomy(id) => {
                write!(formatter, "custom configuration repeats taxonomy {id:?}")
            }
            Self::DuplicateTypeset { taxonomy, typeset } => write!(
                formatter,
                "custom configuration repeats typeset {taxonomy:?}/{typeset:?}"
            ),
            Self::DuplicateTemplate(id) => {
                write!(formatter, "custom configuration repeats template {id:?}")
            }
        }
    }
}

impl Error for CustomConfigurationError {}
