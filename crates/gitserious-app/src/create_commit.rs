use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

use gitserious_core::{
    CommitTypeDefinition, CommitTypeId, CommitValidationErrors, annotate_commit_editor_document,
    commit_editor_document_is_empty, parse_commit_editor_document, render_commit_editor_document,
    render_commit_message,
};

use crate::{
    CommitDraftEditor, CommitOutput, CommitTypeCatalog, CommitTypeSelection, CommitTypeSelector,
    CommitWriter, Fingerprint, ProjectState, ProjectStateStore, RepositoryLocator,
    ResolveProjectPolicyError, fingerprint_commit_type_definition, resolve_project_lock,
};

/// The result of an interactive commit workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitOutcome {
    /// Git created the commit and returned its exact process output.
    Created(CommitOutput),
    /// The user left the selector or editor without an authored draft.
    Cancelled,
}

/// A project-policy condition that prevents safe commit authoring.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitPolicyError {
    /// The repository has no gitserious project configuration.
    NotInitialized,
    /// Authored configuration exists without its generated lock.
    MissingLock,
    /// A generated lock exists without authored configuration.
    OrphanLock,
    /// The generated lock does not match current authored configuration.
    StaleLock,
    /// Authored policy cannot be resolved by this release.
    Resolution(ResolveProjectPolicyError),
    /// A locked commit type is unavailable from the effective catalog.
    MissingDefinition(CommitTypeId),
    /// The available definition's schema version differs from the lock.
    SchemaVersionMismatch(CommitTypeId),
    /// The available definition's complete fingerprint differs from the lock.
    DefinitionFingerprintMismatch(CommitTypeId),
}

impl Display for CommitPolicyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInitialized => formatter.write_str(
                "gitserious is not initialized; run `gitserious init` before committing",
            ),
            Self::MissingLock => formatter
                .write_str("gitserious.lock is missing; run `gitserious init` before committing"),
            Self::OrphanLock => formatter.write_str(
                "gitserious.lock exists without config.toml; restore or remove the orphan lock",
            ),
            Self::StaleLock => formatter.write_str(
                "gitserious project policy is stale; run `gitserious init` before committing",
            ),
            Self::Resolution(error) => Display::fmt(error, formatter),
            Self::MissingDefinition(id) => {
                write!(formatter, "locked commit type {id:?} is not available")
            }
            Self::SchemaVersionMismatch(id) => write!(
                formatter,
                "available commit type {id:?} does not match its locked schema version"
            ),
            Self::DefinitionFingerprintMismatch(id) => write!(
                formatter,
                "available commit type {id:?} does not match its locked definition"
            ),
        }
    }
}

impl Error for CommitPolicyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Resolution(error) => Some(error),
            Self::NotInitialized
            | Self::MissingLock
            | Self::OrphanLock
            | Self::StaleLock
            | Self::MissingDefinition(_)
            | Self::SchemaVersionMismatch(_)
            | Self::DefinitionFingerprintMismatch(_) => None,
        }
    }
}

/// Failure to author or create an interactive commit.
#[derive(Debug)]
pub enum CreateCommitError<
    LocatorError,
    StoreError,
    CatalogError,
    SelectorError,
    EditorError,
    WriterError,
> {
    /// Repository discovery failed.
    Repository(LocatorError),
    /// Repository-local project state could not be read.
    Store(StoreError),
    /// Current project policy is absent, stale, or unavailable.
    Policy(CommitPolicyError),
    /// Effective commit-type catalog access failed.
    Catalog(CatalogError),
    /// Interactive commit-type selection failed.
    Selector(SelectorError),
    /// The requested or selected commit type is outside current policy.
    UnknownCommitType {
        /// Rejected open identifier.
        requested: CommitTypeId,
        /// Available identifiers in project-policy order.
        available: Vec<CommitTypeId>,
    },
    /// Editor document preparation or execution failed.
    Editor(EditorError),
    /// A parsed draft unexpectedly failed canonical rendering.
    InvalidDraft(CommitValidationErrors),
    /// Git failed to create the commit.
    Writer(WriterError),
}

/// Result type for the six independently failing commit-workflow ports.
pub type CreateCommitResult<
    LocatorError,
    StoreError,
    CatalogError,
    SelectorError,
    EditorError,
    WriterError,
> = Result<
    CommitOutcome,
    CreateCommitError<
        LocatorError,
        StoreError,
        CatalogError,
        SelectorError,
        EditorError,
        WriterError,
    >,
>;

impl<LocatorError, StoreError, CatalogError, SelectorError, EditorError, WriterError> Display
    for CreateCommitError<
        LocatorError,
        StoreError,
        CatalogError,
        SelectorError,
        EditorError,
        WriterError,
    >
where
    LocatorError: Display,
    StoreError: Display,
    CatalogError: Display,
    SelectorError: Display,
    EditorError: Display,
    WriterError: Display,
{
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repository(error) => Display::fmt(error, formatter),
            Self::Store(error) => Display::fmt(error, formatter),
            Self::Policy(error) => Display::fmt(error, formatter),
            Self::Catalog(error) => Display::fmt(error, formatter),
            Self::Selector(error) => Display::fmt(error, formatter),
            Self::UnknownCommitType {
                requested,
                available,
            } => {
                write!(
                    formatter,
                    "commit type {requested:?} is not available; choose one of: "
                )?;
                for (index, id) in available.iter().enumerate() {
                    if index > 0 {
                        formatter.write_str(", ")?;
                    }
                    Display::fmt(id, formatter)?;
                }
                Ok(())
            }
            Self::Editor(error) => Display::fmt(error, formatter),
            Self::InvalidDraft(error) => Display::fmt(error, formatter),
            Self::Writer(error) => Display::fmt(error, formatter),
        }
    }
}

impl<LocatorError, StoreError, CatalogError, SelectorError, EditorError, WriterError> Error
    for CreateCommitError<
        LocatorError,
        StoreError,
        CatalogError,
        SelectorError,
        EditorError,
        WriterError,
    >
where
    LocatorError: Error + 'static,
    StoreError: Error + 'static,
    CatalogError: Error + 'static,
    SelectorError: Error + 'static,
    EditorError: Error + 'static,
    WriterError: Error + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Repository(error) => Some(error),
            Self::Store(error) => Some(error),
            Self::Policy(error) => Some(error),
            Self::Catalog(error) => Some(error),
            Self::Selector(error) => Some(error),
            Self::Editor(error) => Some(error),
            Self::InvalidDraft(error) => Some(error),
            Self::Writer(error) => Some(error),
            Self::UnknownCommitType { .. } => None,
        }
    }
}

/// Authors and creates a commit under the repository's exact resolved policy.
///
/// # Errors
///
/// Returns [`CreateCommitError`] when repository or policy discovery, catalog
/// resolution, selection, editing, validation, or Git commit creation fails.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn create_commit<L, S, C, T, E, W>(
    locator: &L,
    store: &S,
    catalog: &C,
    selector: &T,
    editor: &E,
    writer: &W,
    start: &Path,
    requested_type: Option<&CommitTypeId>,
) -> CreateCommitResult<L::Error, S::Error, C::Error, T::Error, E::Error, W::Error>
where
    L: RepositoryLocator + ?Sized,
    S: ProjectStateStore + ?Sized,
    C: CommitTypeCatalog + ?Sized,
    T: CommitTypeSelector + ?Sized,
    E: CommitDraftEditor + ?Sized,
    W: CommitWriter + ?Sized,
{
    let root = locator
        .locate(start)
        .map_err(CreateCommitError::Repository)?;
    let state = store.inspect(&root).map_err(CreateCommitError::Store)?;
    let (config, lock) = match state {
        ProjectState::Absent => {
            return Err(CreateCommitError::Policy(CommitPolicyError::NotInitialized));
        }
        ProjectState::ConfigOnly(_) => {
            return Err(CreateCommitError::Policy(CommitPolicyError::MissingLock));
        }
        ProjectState::LockOnly => {
            return Err(CreateCommitError::Policy(CommitPolicyError::OrphanLock));
        }
        ProjectState::Initialized { config, lock } => (config, lock),
    };

    let expected_lock = resolve_project_lock(&config)
        .map_err(CommitPolicyError::Resolution)
        .map_err(CreateCommitError::Policy)?;
    if lock != expected_lock {
        return Err(CreateCommitError::Policy(CommitPolicyError::StaleLock));
    }

    let catalog_definitions = catalog.list().map_err(CreateCommitError::Catalog)?;
    let definitions = resolve_locked_definitions(&lock, &catalog_definitions)
        .map_err(CreateCommitError::Policy)?;
    let selected = match requested_type {
        Some(requested) => requested.clone(),
        None => match selector
            .select(&definitions)
            .map_err(CreateCommitError::Selector)?
        {
            CommitTypeSelection::Selected(selected) => selected,
            CommitTypeSelection::Cancelled => return Ok(CommitOutcome::Cancelled),
        },
    };
    let Some(definition) = definitions
        .iter()
        .find(|definition| definition.id() == &selected)
    else {
        return Err(CreateCommitError::UnknownCommitType {
            requested: selected,
            available: definitions
                .iter()
                .map(|definition| definition.id().clone())
                .collect(),
        });
    };

    let mut document = render_commit_editor_document(definition);
    loop {
        let edited = editor
            .edit(&root, &document)
            .map_err(CreateCommitError::Editor)?;
        if edited == document || commit_editor_document_is_empty(&edited) {
            return Ok(CommitOutcome::Cancelled);
        }
        match parse_commit_editor_document(definition, &edited) {
            Ok(draft) => {
                let message = render_commit_message(definition, &draft)
                    .map_err(CreateCommitError::InvalidDraft)?;
                let output = writer
                    .commit(&root, &message)
                    .map_err(CreateCommitError::Writer)?;
                return Ok(CommitOutcome::Created(output));
            }
            Err(errors) => {
                document = annotate_commit_editor_document(&edited, &errors);
            }
        }
    }
}

fn resolve_locked_definitions(
    lock: &crate::ProjectLock,
    catalog: &[CommitTypeDefinition],
) -> Result<Vec<CommitTypeDefinition>, CommitPolicyError> {
    lock.resolved_template()
        .commit_types()
        .iter()
        .map(|locked| {
            let Some(definition) = catalog
                .iter()
                .find(|definition| definition.id() == locked.id())
            else {
                return Err(CommitPolicyError::MissingDefinition(locked.id().clone()));
            };
            if definition.schema_version() != locked.schema_version() {
                return Err(CommitPolicyError::SchemaVersionMismatch(
                    locked.id().clone(),
                ));
            }
            let fingerprint: Fingerprint = fingerprint_commit_type_definition(definition);
            if fingerprint != locked.definition_fingerprint() {
                return Err(CommitPolicyError::DefinitionFingerprintMismatch(
                    locked.id().clone(),
                ));
            }
            Ok(definition.clone())
        })
        .collect()
}
