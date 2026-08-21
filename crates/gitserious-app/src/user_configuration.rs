use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use gitserious_core::{
    TaxonomyDefinition, TaxonomyId, TemplateDefinition, TemplateId, TypesetDefinition, TypesetId,
};

/// The global user-owned configuration snapshot persisted as one aggregate.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UserConfiguration {
    taxonomies: Vec<TaxonomyDefinition>,
    typesets: Vec<TypesetDefinition>,
    templates: Vec<TemplateDefinition>,
}

impl UserConfiguration {
    /// Creates a structurally unique user configuration snapshot.
    ///
    /// Cross-definition references are checked by the effective catalog.
    ///
    /// # Errors
    ///
    /// Returns [`UserConfigurationError`] when a top-level identity is
    /// repeated.
    pub fn new(
        taxonomies: Vec<TaxonomyDefinition>,
        typesets: Vec<TypesetDefinition>,
        templates: Vec<TemplateDefinition>,
    ) -> Result<Self, UserConfigurationError> {
        let mut taxonomy_ids = BTreeSet::new();
        for taxonomy in &taxonomies {
            if !taxonomy_ids.insert(taxonomy.id()) {
                return Err(UserConfigurationError::DuplicateTaxonomy(
                    taxonomy.id().clone(),
                ));
            }
        }

        let mut typeset_ids = BTreeSet::new();
        for typeset in &typesets {
            let key = (typeset.taxonomy(), typeset.id());
            if !typeset_ids.insert(key) {
                return Err(UserConfigurationError::DuplicateTypeset {
                    taxonomy: typeset.taxonomy().clone(),
                    typeset: typeset.id().clone(),
                });
            }
        }

        let mut template_ids = BTreeSet::new();
        for template in &templates {
            if !template_ids.insert(template.id()) {
                return Err(UserConfigurationError::DuplicateTemplate(
                    template.id().clone(),
                ));
            }
        }

        Ok(Self {
            taxonomies,
            typesets,
            templates,
        })
    }

    /// Returns user-defined taxonomies in snapshot order.
    #[must_use]
    pub fn taxonomies(&self) -> &[TaxonomyDefinition] {
        &self.taxonomies
    }

    /// Returns user-defined typesets in snapshot order.
    #[must_use]
    pub fn typesets(&self) -> &[TypesetDefinition] {
        &self.typesets
    }

    /// Returns user-defined templates in snapshot order.
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

/// A duplicate top-level definition in user configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UserConfigurationError {
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

impl Display for UserConfigurationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateTaxonomy(id) => {
                write!(formatter, "user configuration repeats taxonomy {id:?}")
            }
            Self::DuplicateTypeset { taxonomy, typeset } => write!(
                formatter,
                "user configuration repeats typeset {taxonomy:?}/{typeset:?}"
            ),
            Self::DuplicateTemplate(id) => {
                write!(formatter, "user configuration repeats template {id:?}")
            }
        }
    }
}

impl Error for UserConfigurationError {}
