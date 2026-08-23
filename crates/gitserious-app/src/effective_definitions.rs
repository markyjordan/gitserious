use gitserious_core::{CommitTypeDefinition, TemplateId};

/// Read access to the commit-type definitions of one installed template.
///
/// Adapters own how templates resolve into concrete definitions. Application
/// use cases see only owned domain definitions and the adapter's error type.
pub trait EffectiveDefinitions {
    /// The adapter-specific failure returned by definition resolution.
    type Error;

    /// Returns one template's commit-type definitions in canonical order.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when the template's definitions
    /// cannot be resolved. A successful resolution can still be empty.
    fn for_template(
        &self,
        template: &TemplateId,
    ) -> Result<Vec<CommitTypeDefinition>, Self::Error>;
}
