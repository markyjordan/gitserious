use crate::{Description, TaxonomyId, TemplateId, TemplateVersion, TypesetId};

/// A reusable selection of one taxonomy and one compatible typeset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateDefinition {
    id: TemplateId,
    version: TemplateVersion,
    description: Description,
    taxonomy: TaxonomyId,
    typeset: TypesetId,
}

impl TemplateDefinition {
    /// Creates a reusable configuration template.
    #[must_use]
    pub const fn new(
        id: TemplateId,
        version: TemplateVersion,
        description: Description,
        taxonomy: TaxonomyId,
        typeset: TypesetId,
    ) -> Self {
        Self {
            id,
            version,
            description,
            taxonomy,
            typeset,
        }
    }

    /// Returns the globally unique template identifier.
    #[must_use]
    pub const fn id(&self) -> &TemplateId {
        &self.id
    }

    /// Returns the template's semantic version.
    #[must_use]
    pub const fn version(&self) -> TemplateVersion {
        self.version
    }

    /// Returns the template's purpose.
    #[must_use]
    pub const fn description(&self) -> &Description {
        &self.description
    }

    /// Returns the selected taxonomy.
    #[must_use]
    pub const fn taxonomy(&self) -> &TaxonomyId {
        &self.taxonomy
    }

    /// Returns the selected taxonomy-scoped typeset.
    #[must_use]
    pub const fn typeset(&self) -> &TypesetId {
        &self.typeset
    }
}
