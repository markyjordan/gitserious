use gitserious_core::{
    CommitDraft, CommitTypeDefinition, CommitTypeId, ResolvedChangeType, ResolvedTaxonomy,
    TemplateId,
};
use std::collections::BTreeSet;

/// A resolved template and its schema-ordered authoring definitions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitTemplate {
    schema: ResolvedTaxonomy,
    definitions: Vec<CommitTypeDefinition>,
}

impl CommitTemplate {
    /// Captures immutable template meaning for one authoring interaction.
    #[must_use]
    pub fn new(schema: ResolvedTaxonomy) -> Self {
        let definitions = schema
            .change_types()
            .iter()
            .map(ResolvedChangeType::commit_type_definition)
            .collect();
        Self {
            schema,
            definitions,
        }
    }
    /// Returns the actual template identity, including `default` when selected.
    #[must_use]
    pub const fn id(&self) -> &TemplateId {
        self.schema.template_id()
    }
    /// Returns the complete resolved schema snapshot.
    #[must_use]
    pub const fn schema(&self) -> &ResolvedTaxonomy {
        &self.schema
    }
    /// Returns change types in taxonomy order with their typeset properties.
    #[must_use]
    pub fn definitions(&self) -> &[CommitTypeDefinition] {
        &self.definitions
    }
}

/// Available project templates and the initial CLI/project selection.
#[derive(Clone, Debug)]
pub struct CommitAuthoringContext {
    templates: Vec<CommitTemplate>,
    initial: usize,
    requested_type: Option<CommitTypeId>,
}

impl CommitAuthoringContext {
    /// Creates an authoring snapshot with an optional type preselection.
    ///
    /// # Errors
    /// Returns an error for duplicate template identities, a missing initial
    /// template, or a type absent from that initial template.
    pub fn new(
        schemas: Vec<ResolvedTaxonomy>,
        initial: &TemplateId,
        requested_type: Option<&CommitTypeId>,
    ) -> Result<Self, String> {
        let mut identities = BTreeSet::new();
        for schema in &schemas {
            if !identities.insert(schema.template_id()) {
                return Err(format!(
                    "duplicate authoring template {}",
                    schema.template_id()
                ));
            }
        }
        let templates: Vec<_> = schemas.into_iter().map(CommitTemplate::new).collect();
        let initial = templates
            .iter()
            .position(|template| template.id() == initial)
            .ok_or_else(|| format!("initial template {initial} is unavailable"))?;
        if let Some(id) = requested_type
            && !templates[initial]
                .definitions()
                .iter()
                .any(|definition| definition.id() == id)
        {
            return Err(format!(
                "type {id} is unavailable in template {}",
                templates[initial].id()
            ));
        }
        Ok(Self {
            templates,
            initial,
            requested_type: requested_type.cloned(),
        })
    }
    /// Returns the immutable choices available for this commit.
    #[must_use]
    pub fn templates(&self) -> &[CommitTemplate] {
        &self.templates
    }
    /// Returns the explicit CLI selection or active project default.
    #[must_use]
    pub fn initial_template(&self) -> &CommitTemplate {
        &self.templates[self.initial]
    }
    /// Finds a template within this captured project snapshot.
    #[must_use]
    pub fn find_template(&self, id: &TemplateId) -> Option<&CommitTemplate> {
        self.templates.iter().find(|template| template.id() == id)
    }
    /// Returns a type preselected within the initial template only.
    #[must_use]
    pub fn preselected_type(&self) -> Option<&CommitTypeDefinition> {
        self.requested_type.as_ref().and_then(|id| {
            self.initial_template()
                .definitions()
                .iter()
                .find(|definition| definition.id() == id)
        })
    }
}

/// An authored draft bound to the template chosen for this commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthoredCommit {
    template: TemplateId,
    draft: CommitDraft,
}

impl AuthoredCommit {
    /// Records an adapter's selected identity and authored draft for validation.
    #[must_use]
    pub const fn new(template: TemplateId, draft: CommitDraft) -> Self {
        Self { template, draft }
    }
    /// Returns the adapter's selected template identity.
    #[must_use]
    pub const fn template(&self) -> &TemplateId {
        &self.template
    }
    /// Returns the authored draft.
    #[must_use]
    pub const fn draft(&self) -> &CommitDraft {
        &self.draft
    }
}

/// Result of a template-aware authoring interaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitAuthoringOutcome {
    /// Continue with the selected template and its draft.
    Authored(AuthoredCommit),
    /// Leave the repository unchanged.
    Cancelled,
}
