use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

use gitserious_core::{
    CommitTypeDefinition, CommitTypeId, CommitValidationErrors, ResolvedChangeType, TemplateId,
};

use crate::{
    CommitAuthoringContext, CommitAuthoringOutcome, CommitDraftAuthor, CommitOutput, CommitWriter,
    ConfigurationCatalog, Fingerprint, ProjectState, ProjectStateStore, RepositoryLocator,
    ResolveProjectPolicyError, fingerprint_commit_type_definition, resolve_project_lock,
};

/// The result of an interactive commit workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitOutcome {
    /// Git created the commit and returned its exact process output.
    Created(CommitOutput),
    /// The user left authoring without a completed draft.
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
                "gitserious.lock exists without gitserious.toml; restore or remove the orphan lock",
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
pub enum CreateCommitError<LocatorError, StoreError, AuthorError, WriterError> {
    /// The author supplied no message that was reviewed with provenance.
    MissingReviewedMessage,
    /// The approved bytes differ from the selected schema and draft rendering.
    ReviewedMessageMismatch,
    /// The requested or returned template is outside project policy.
    UnknownTemplate {
        requested: TemplateId,
        available: Vec<TemplateId>,
    },
    /// A type preselection cannot be carried to another template implicitly.
    AuthoredTemplateMismatch {
        expected: TemplateId,
        actual: TemplateId,
    },
    /// Captured choices violate authoring-context invariants.
    InvalidContext(String),
    /// Repository discovery failed.
    Repository(LocatorError),
    /// Repository-local project state could not be read.
    Store(StoreError),
    /// Current project policy is absent, stale, or unavailable.
    Policy(CommitPolicyError),
    /// Structured draft authoring failed.
    Author(AuthorError),
    /// The requested or selected commit type is outside current policy.
    UnknownCommitType {
        /// Rejected open identifier.
        requested: CommitTypeId,
        /// Available identifiers in project-policy order.
        available: Vec<CommitTypeId>,
    },
    /// The author returned a different type than the CLI preselected.
    AuthoredTypeMismatch {
        /// Type pinned by the delivery request.
        expected: CommitTypeId,
        /// Type returned by the authoring adapter.
        actual: CommitTypeId,
    },
    /// An authored draft failed canonical validation or rendering.
    InvalidDraft(CommitValidationErrors),
    /// Git failed to create the commit.
    Writer(WriterError),
}

/// Result type for the four independently failing commit-workflow ports.
pub type CreateCommitResult<LocatorError, StoreError, AuthorError, WriterError> =
    Result<CommitOutcome, CreateCommitError<LocatorError, StoreError, AuthorError, WriterError>>;

impl<LocatorError, StoreError, AuthorError, WriterError> Display
    for CreateCommitError<LocatorError, StoreError, AuthorError, WriterError>
where
    LocatorError: Display,
    StoreError: Display,
    AuthorError: Display,
    WriterError: Display,
{
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingReviewedMessage => {
                formatter.write_str("commit author did not return a reviewed canonical message")
            }
            Self::ReviewedMessageMismatch => formatter.write_str(
                "reviewed commit message does not match the selected template and draft",
            ),
            Self::UnknownTemplate {
                requested,
                available,
            } => write!(
                formatter,
                "template {requested} is not available; choose one of: {}",
                available
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::AuthoredTemplateMismatch { expected, actual } => write!(
                formatter,
                "preselected type belongs to template {expected}, not {actual}"
            ),
            Self::InvalidContext(error) => formatter.write_str(error),
            Self::Repository(error) => Display::fmt(error, formatter),
            Self::Store(error) => Display::fmt(error, formatter),
            Self::Policy(error) => Display::fmt(error, formatter),
            Self::Author(error) => Display::fmt(error, formatter),
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
            Self::AuthoredTypeMismatch { expected, actual } => write!(
                formatter,
                "authored type {actual:?} does not match requested type {expected:?}"
            ),
            Self::InvalidDraft(error) => Display::fmt(error, formatter),
            Self::Writer(error) => Display::fmt(error, formatter),
        }
    }
}

impl<LocatorError, StoreError, AuthorError, WriterError> Error
    for CreateCommitError<LocatorError, StoreError, AuthorError, WriterError>
where
    LocatorError: Error + 'static,
    StoreError: Error + 'static,
    AuthorError: Error + 'static,
    WriterError: Error + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Repository(error) => Some(error),
            Self::Store(error) => Some(error),
            Self::Policy(error) => Some(error),
            Self::Author(error) => Some(error),
            Self::InvalidDraft(error) => Some(error),
            Self::Writer(error) => Some(error),
            Self::UnknownCommitType { .. }
            | Self::AuthoredTypeMismatch { .. }
            | Self::UnknownTemplate { .. }
            | Self::AuthoredTemplateMismatch { .. }
            | Self::InvalidContext(_)
            | Self::MissingReviewedMessage
            | Self::ReviewedMessageMismatch => None,
        }
    }
}

/// Authors and creates a commit under the repository's exact resolved policy.
///
/// # Errors
///
/// Returns [`CreateCommitError`] when repository or policy discovery,
/// authoring, validation, or Git commit creation fails.
pub fn create_commit<L, S, A, W>(
    locator: &L,
    store: &S,
    author: &A,
    writer: &W,
    start: &Path,
    requested_type: Option<&CommitTypeId>,
) -> CreateCommitResult<L::Error, S::Error, A::Error, W::Error>
where
    L: RepositoryLocator + ?Sized,
    S: ProjectStateStore + ?Sized,
    A: CommitDraftAuthor + ?Sized,
    W: CommitWriter + ?Sized,
{
    create_commit_with_template(locator, store, author, writer, start, None, requested_type)
}

/// Authors with an explicit template override or the active project default.
///
/// # Errors
/// Returns policy, selection, authoring, validation, or Git creation errors.
pub fn create_commit_with_template<L, S, A, W>(
    locator: &L,
    store: &S,
    author: &A,
    writer: &W,
    start: &Path,
    requested_template: Option<&TemplateId>,
    requested_type: Option<&CommitTypeId>,
) -> CreateCommitResult<L::Error, S::Error, A::Error, W::Error>
where
    L: RepositoryLocator + ?Sized,
    S: ProjectStateStore + ?Sized,
    A: CommitDraftAuthor + ?Sized,
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

    let catalog = ConfigurationCatalog::new(config.custom())
        .map_err(ResolveProjectPolicyError::Catalog)
        .map_err(CommitPolicyError::Resolution)
        .map_err(CreateCommitError::Policy)?;
    let expected_lock = resolve_project_lock(&config)
        .map_err(CommitPolicyError::Resolution)
        .map_err(CreateCommitError::Policy)?;
    if lock != expected_lock {
        return Err(CreateCommitError::Policy(CommitPolicyError::StaleLock));
    }

    let reference = requested_template.unwrap_or(config.active_template());
    if catalog.find_template(reference).is_none() {
        return Err(CreateCommitError::UnknownTemplate {
            requested: reference.clone(),
            available: catalog
                .templates()
                .iter()
                .map(|template| template.id().clone())
                .collect(),
        });
    }
    let available =
        locked_definitions(&lock, &catalog, reference).map_err(CreateCommitError::Policy)?;
    if let Some(requested) = requested_type {
        find_definition(&available, requested)?;
    }
    let schemas = catalog
        .templates()
        .iter()
        .map(|template| catalog.resolve(template.id()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(ResolveProjectPolicyError::Catalog)
        .map_err(CommitPolicyError::Resolution)
        .map_err(CreateCommitError::Policy)?;
    let context = CommitAuthoringContext::new(schemas, reference, requested_type)
        .map_err(CreateCommitError::InvalidContext)?;
    author_and_write(author, writer, &root, &context)
}

fn author_and_write<L, S, A, W>(
    author: &A,
    writer: &W,
    root: &crate::RepositoryRoot,
    context: &CommitAuthoringContext,
) -> CreateCommitResult<L, S, A::Error, W::Error>
where
    A: CommitDraftAuthor + ?Sized,
    W: CommitWriter + ?Sized,
{
    let authored = match author
        .author_with_context(context)
        .map_err(CreateCommitError::Author)?
    {
        CommitAuthoringOutcome::Authored(authored) => authored,
        CommitAuthoringOutcome::Cancelled => return Ok(CommitOutcome::Cancelled),
    };
    let template = context.find_template(authored.template()).ok_or_else(|| {
        CreateCommitError::UnknownTemplate {
            requested: authored.template().clone(),
            available: context
                .templates()
                .iter()
                .map(|template| template.id().clone())
                .collect(),
        }
    })?;
    if let Some(expected) = context.preselected_type() {
        if template.id() != context.initial_template().id() {
            return Err(CreateCommitError::AuthoredTemplateMismatch {
                expected: context.initial_template().id().clone(),
                actual: template.id().clone(),
            });
        }
        if expected.id() != authored.draft().commit_type() {
            return Err(CreateCommitError::AuthoredTypeMismatch {
                expected: expected.id().clone(),
                actual: authored.draft().commit_type().clone(),
            });
        }
    }
    find_definition(template.definitions(), authored.draft().commit_type())?;
    let message = template
        .render(authored.draft())
        .map_err(CreateCommitError::InvalidDraft)?;
    let reviewed = authored
        .reviewed_message()
        .ok_or(CreateCommitError::MissingReviewedMessage)?;
    if reviewed != &message {
        return Err(CreateCommitError::ReviewedMessageMismatch);
    }
    let output = writer
        .commit(root, reviewed)
        .map_err(CreateCommitError::Writer)?;
    Ok(CommitOutcome::Created(output))
}

fn find_definition<'a, LocatorError, StoreError, AuthorError, WriterError>(
    definitions: &'a [CommitTypeDefinition],
    requested: &CommitTypeId,
) -> Result<
    &'a CommitTypeDefinition,
    CreateCommitError<LocatorError, StoreError, AuthorError, WriterError>,
> {
    definitions
        .iter()
        .find(|definition| definition.id() == requested)
        .ok_or_else(|| CreateCommitError::UnknownCommitType {
            requested: requested.clone(),
            available: definitions
                .iter()
                .map(|definition| definition.id().clone())
                .collect(),
        })
}

fn locked_definitions(
    lock: &crate::ProjectLock,
    catalog: &ConfigurationCatalog,
    reference: &TemplateId,
) -> Result<Vec<CommitTypeDefinition>, CommitPolicyError> {
    let resolved = catalog.resolve(reference).map_err(|error| {
        CommitPolicyError::Resolution(ResolveProjectPolicyError::Catalog(error))
    })?;
    let available = resolved
        .change_types()
        .iter()
        .map(ResolvedChangeType::commit_type_definition)
        .collect::<Vec<_>>();
    let locked = lock
        .resolved_templates()
        .iter()
        .find(|template| template.id() == reference)
        .ok_or_else(|| {
            CommitPolicyError::Resolution(ResolveProjectPolicyError::UnknownTemplate(
                reference.clone(),
            ))
        })?;
    resolve_locked_definitions(locked, &available)
}

fn resolve_locked_definitions(
    lock: &crate::ResolvedTemplate,
    definitions: &[CommitTypeDefinition],
) -> Result<Vec<CommitTypeDefinition>, CommitPolicyError> {
    lock.commit_types()
        .iter()
        .map(|locked| {
            let Some(definition) = definitions
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
