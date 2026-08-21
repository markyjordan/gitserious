use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::{
    ChangeTypeId, Description, PropertyDefinition, TaxonomyDefinition, TaxonomyId, TaxonomyVersion,
    TemplateDefinition, TemplateId, TemplateVersion, TypesetDefinition, TypesetId, TypesetVersion,
};

/// One fully joined change type ready for authoring and validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedChangeType {
    id: ChangeTypeId,
    description: Description,
    properties: Vec<PropertyDefinition>,
}

impl ResolvedChangeType {
    /// Returns the change-type identifier.
    #[must_use]
    pub const fn id(&self) -> &ChangeTypeId {
        &self.id
    }

    /// Returns the taxonomy-owned semantic description.
    #[must_use]
    pub const fn description(&self) -> &Description {
        &self.description
    }

    /// Returns typeset-owned properties in canonical order.
    #[must_use]
    pub fn properties(&self) -> &[PropertyDefinition] {
        &self.properties
    }
}

/// A fully resolved template containing all data needed by future consumers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedTaxonomy {
    template_id: TemplateId,
    template_version: TemplateVersion,
    template_description: Description,
    taxonomy_id: TaxonomyId,
    taxonomy_version: TaxonomyVersion,
    taxonomy_description: Description,
    typeset_id: TypesetId,
    typeset_version: TypesetVersion,
    typeset_description: Description,
    change_types: Vec<ResolvedChangeType>,
}

impl ResolvedTaxonomy {
    /// Joins one template, taxonomy, and typeset into an effective model.
    ///
    /// # Errors
    ///
    /// Returns [`ResolveTaxonomyError`] when references disagree or the
    /// typeset does not explicitly cover exactly the taxonomy's change types.
    pub fn resolve(
        template: &TemplateDefinition,
        taxonomy: &TaxonomyDefinition,
        typeset: &TypesetDefinition,
    ) -> Result<Self, ResolveTaxonomyError> {
        if template.taxonomy() != taxonomy.id() {
            return Err(ResolveTaxonomyError::TemplateTaxonomyMismatch {
                expected: template.taxonomy().clone(),
                actual: taxonomy.id().clone(),
            });
        }
        if typeset.taxonomy() != taxonomy.id() {
            return Err(ResolveTaxonomyError::TypesetTaxonomyMismatch {
                expected: taxonomy.id().clone(),
                actual: typeset.taxonomy().clone(),
            });
        }
        if template.typeset() != typeset.id() {
            return Err(ResolveTaxonomyError::TemplateTypesetMismatch {
                expected: template.typeset().clone(),
                actual: typeset.id().clone(),
            });
        }

        let taxonomy_ids = taxonomy
            .change_types()
            .iter()
            .map(crate::ChangeTypeDefinition::id)
            .collect::<BTreeSet<_>>();
        for schema in typeset.schemas() {
            if !taxonomy_ids.contains(schema.change_type()) {
                return Err(ResolveTaxonomyError::UnknownTypesetChangeType(
                    schema.change_type().clone(),
                ));
            }
        }

        let mut change_types = Vec::with_capacity(taxonomy.change_types().len());
        for change_type in taxonomy.change_types() {
            let Some(schema) = typeset
                .schemas()
                .iter()
                .find(|schema| schema.change_type() == change_type.id())
            else {
                return Err(ResolveTaxonomyError::MissingTypesetChangeType(
                    change_type.id().clone(),
                ));
            };
            change_types.push(ResolvedChangeType {
                id: change_type.id().clone(),
                description: change_type.description().clone(),
                properties: schema.properties().to_vec(),
            });
        }

        Ok(Self {
            template_id: template.id().clone(),
            template_version: template.version(),
            template_description: template.description().clone(),
            taxonomy_id: taxonomy.id().clone(),
            taxonomy_version: taxonomy.version(),
            taxonomy_description: taxonomy.description().clone(),
            typeset_id: typeset.id().clone(),
            typeset_version: typeset.version(),
            typeset_description: typeset.description().clone(),
            change_types,
        })
    }

    /// Returns the selected template identifier.
    #[must_use]
    pub const fn template_id(&self) -> &TemplateId {
        &self.template_id
    }

    /// Returns the selected template version.
    #[must_use]
    pub const fn template_version(&self) -> TemplateVersion {
        self.template_version
    }

    /// Returns the selected template description.
    #[must_use]
    pub const fn template_description(&self) -> &Description {
        &self.template_description
    }

    /// Returns the taxonomy identifier.
    #[must_use]
    pub const fn taxonomy_id(&self) -> &TaxonomyId {
        &self.taxonomy_id
    }

    /// Returns the taxonomy version.
    #[must_use]
    pub const fn taxonomy_version(&self) -> TaxonomyVersion {
        self.taxonomy_version
    }

    /// Returns the taxonomy description.
    #[must_use]
    pub const fn taxonomy_description(&self) -> &Description {
        &self.taxonomy_description
    }

    /// Returns the taxonomy-scoped typeset identifier.
    #[must_use]
    pub const fn typeset_id(&self) -> &TypesetId {
        &self.typeset_id
    }

    /// Returns the selected typeset version.
    #[must_use]
    pub const fn typeset_version(&self) -> TypesetVersion {
        self.typeset_version
    }

    /// Returns the selected typeset description.
    #[must_use]
    pub const fn typeset_description(&self) -> &Description {
        &self.typeset_description
    }

    /// Returns fully joined change types in taxonomy order.
    #[must_use]
    pub fn change_types(&self) -> &[ResolvedChangeType] {
        &self.change_types
    }
}

/// A template, taxonomy, or typeset compatibility failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolveTaxonomyError {
    /// The supplied taxonomy is not the template selection.
    TemplateTaxonomyMismatch {
        /// Selected taxonomy.
        expected: TaxonomyId,
        /// Supplied taxonomy.
        actual: TaxonomyId,
    },
    /// The supplied typeset belongs to another taxonomy.
    TypesetTaxonomyMismatch {
        /// Supplied taxonomy identity.
        expected: TaxonomyId,
        /// Typeset taxonomy identity.
        actual: TaxonomyId,
    },
    /// The supplied typeset is not the template selection.
    TemplateTypesetMismatch {
        /// Selected typeset.
        expected: TypesetId,
        /// Supplied typeset.
        actual: TypesetId,
    },
    /// The typeset omits a taxonomy change type.
    MissingTypesetChangeType(ChangeTypeId),
    /// The typeset contains a change type absent from the taxonomy.
    UnknownTypesetChangeType(ChangeTypeId),
}

impl Display for ResolveTaxonomyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::TemplateTaxonomyMismatch { expected, actual } => write!(
                formatter,
                "template selects taxonomy {expected:?}, not {actual:?}"
            ),
            Self::TypesetTaxonomyMismatch { expected, actual } => write!(
                formatter,
                "typeset belongs to taxonomy {actual:?}, not {expected:?}"
            ),
            Self::TemplateTypesetMismatch { expected, actual } => write!(
                formatter,
                "template selects typeset {expected:?}, not {actual:?}"
            ),
            Self::MissingTypesetChangeType(id) => {
                write!(formatter, "typeset does not define change type {id:?}")
            }
            Self::UnknownTypesetChangeType(id) => {
                write!(formatter, "typeset defines unknown change type {id:?}")
            }
        }
    }
}

impl Error for ResolveTaxonomyError {}
