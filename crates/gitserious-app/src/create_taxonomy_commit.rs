use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

use gitserious_core::{CommitTypeDefinition, CommitTypeId, CommitValidationErrors, TaxonomyId};

use crate::{
    CommitAuthoringContext, CommitAuthoringOutcome, CommitDraftAuthor, CommitOutput, CommitWriter,
    ProjectState, ProjectStateStore, RepositoryLocator,
};

/// Result of an interactive commit workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitOutcome {
    /// Git created the commit.
    Created(CommitOutput),
    /// The user cancelled authoring.
    Cancelled,
}

/// A project-policy condition that prevents safe authoring.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitPolicyError {
    /// No project artifacts exist.
    NotInitialized,
    /// Authored configuration exists without its lock.
    MissingLock,
    /// A lock exists without authored configuration.
    OrphanLock,
    /// Authored project configuration and lock disagree.
    StaleLock,
}

impl Display for CommitPolicyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInitialized => formatter.write_str(
                "gitserious is not initialized; run `gitserious init` before committing",
            ),
            Self::MissingLock => formatter.write_str(
                "gitserious.lock is missing; run `gitserious config`, open Project, and review Apply/Refresh",
            ),
            Self::OrphanLock => formatter.write_str(
                "gitserious.lock exists without gitserious.toml; restore or remove the orphan lock",
            ),
            Self::StaleLock => formatter.write_str(
                "gitserious.toml and gitserious.lock disagree; run `gitserious config`, open Project, and review Apply/Refresh",
            ),
        }
    }
}

impl Error for CommitPolicyError {}

/// Failure to author or create a commit.
#[derive(Debug)]
pub enum CreateCommitError<LocatorError, StoreError, AuthorError, WriterError> {
    MissingReviewedMessage,
    ReviewedMessageMismatch,
    UnknownTaxonomy {
        requested: TaxonomyId,
        available: Vec<TaxonomyId>,
    },
    AuthoredTaxonomyMismatch {
        expected: TaxonomyId,
        actual: TaxonomyId,
    },
    InvalidContext(String),
    Repository(LocatorError),
    Store(StoreError),
    Policy(CommitPolicyError),
    Author(AuthorError),
    UnknownCommitType {
        requested: CommitTypeId,
        available: Vec<CommitTypeId>,
    },
    AuthoredTypeMismatch {
        expected: CommitTypeId,
        actual: CommitTypeId,
    },
    InvalidDraft(CommitValidationErrors),
    Writer(WriterError),
}

pub type CreateCommitResult<LocatorError, StoreError, AuthorError, WriterError> =
    Result<CommitOutcome, CreateCommitError<LocatorError, StoreError, AuthorError, WriterError>>;

impl<L: Display, S: Display, A: Display, W: Display> Display for CreateCommitError<L, S, A, W> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingReviewedMessage => {
                formatter.write_str("author did not return reviewed commit bytes")
            }
            Self::ReviewedMessageMismatch => {
                formatter.write_str("reviewed commit bytes do not match the selected taxonomy")
            }
            Self::UnknownTaxonomy {
                requested,
                available,
            } => write!(
                formatter,
                "taxonomy {requested:?} is unavailable; choose one of: {}",
                display_ids(available)
            ),
            Self::AuthoredTaxonomyMismatch { expected, actual } => write!(
                formatter,
                "authored taxonomy {actual:?} does not match requested taxonomy {expected:?}"
            ),
            Self::InvalidContext(error) => formatter.write_str(error),
            Self::Repository(error) => Display::fmt(error, formatter),
            Self::Store(error) => Display::fmt(error, formatter),
            Self::Policy(error) => Display::fmt(error, formatter),
            Self::Author(error) => Display::fmt(error, formatter),
            Self::UnknownCommitType {
                requested,
                available,
            } => write!(
                formatter,
                "commit type {requested:?} is unavailable; choose one of: {}",
                display_ids(available)
            ),
            Self::AuthoredTypeMismatch { expected, actual } => write!(
                formatter,
                "authored type {actual:?} does not match requested type {expected:?}"
            ),
            Self::InvalidDraft(error) => Display::fmt(error, formatter),
            Self::Writer(error) => Display::fmt(error, formatter),
        }
    }
}

impl<L: Error + 'static, S: Error + 'static, A: Error + 'static, W: Error + 'static> Error
    for CreateCommitError<L, S, A, W>
{
}

fn display_ids<T: Display>(ids: &[T]) -> String {
    ids.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Authors with the pinned default taxonomy.
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
    create_commit_with_taxonomy(locator, store, author, writer, start, None, requested_type)
}

/// Authors with an explicit taxonomy or the pinned project default.
pub fn create_commit_with_taxonomy<L, S, A, W>(
    locator: &L,
    store: &S,
    author: &A,
    writer: &W,
    start: &Path,
    requested_taxonomy: Option<&TaxonomyId>,
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
    if !lock.matches(&config) {
        return Err(CreateCommitError::Policy(CommitPolicyError::StaleLock));
    }
    let selected = requested_taxonomy.unwrap_or_else(|| lock.default_taxonomy());
    let taxonomy = lock
        .taxonomies()
        .iter()
        .find(|taxonomy| taxonomy.id() == selected)
        .ok_or_else(|| CreateCommitError::UnknownTaxonomy {
            requested: selected.clone(),
            available: lock
                .taxonomies()
                .iter()
                .map(|taxonomy| taxonomy.id().clone())
                .collect(),
        })?;
    if let Some(requested) = requested_type {
        find_definition(taxonomy.commit_types(), requested)?;
    }
    let context = CommitAuthoringContext::new(lock.taxonomies().to_vec(), selected, requested_type)
        .map_err(CreateCommitError::InvalidContext)?;
    let authored = match author
        .author_with_context(&context)
        .map_err(CreateCommitError::Author)?
    {
        CommitAuthoringOutcome::Authored(authored) => authored,
        CommitAuthoringOutcome::Cancelled => return Ok(CommitOutcome::Cancelled),
    };
    let taxonomy = context.find_taxonomy(authored.taxonomy()).ok_or_else(|| {
        CreateCommitError::UnknownTaxonomy {
            requested: authored.taxonomy().clone(),
            available: context
                .taxonomies()
                .iter()
                .map(|taxonomy| taxonomy.id().clone())
                .collect(),
        }
    })?;
    if let Some(expected) = context.preselected_type() {
        if taxonomy.id() != context.initial_taxonomy().id() {
            return Err(CreateCommitError::AuthoredTaxonomyMismatch {
                expected: context.initial_taxonomy().id().clone(),
                actual: taxonomy.id().clone(),
            });
        }
        if expected.id() != authored.draft().commit_type() {
            return Err(CreateCommitError::AuthoredTypeMismatch {
                expected: expected.id().clone(),
                actual: authored.draft().commit_type().clone(),
            });
        }
    }
    find_definition(taxonomy.definitions(), authored.draft().commit_type())?;
    let message = taxonomy
        .render(authored.draft())
        .map_err(CreateCommitError::InvalidDraft)?;
    let reviewed = authored
        .reviewed_message()
        .ok_or(CreateCommitError::MissingReviewedMessage)?;
    if reviewed != &message {
        return Err(CreateCommitError::ReviewedMessageMismatch);
    }
    let output = writer
        .commit(&root, reviewed)
        .map_err(CreateCommitError::Writer)?;
    Ok(CommitOutcome::Created(output))
}

fn find_definition<'a, L, S, A, W>(
    definitions: &'a [CommitTypeDefinition],
    requested: &CommitTypeId,
) -> Result<&'a CommitTypeDefinition, CreateCommitError<L, S, A, W>> {
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
