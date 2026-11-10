use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::Path;

use gitserious_core::{CommitTypeDefinition, CommitTypeId, CommitValidationErrors, TaxonomyId};

use crate::{CommitAuthoringContext, ProjectConfigurationStore, RepositoryLocator};

/// A project-policy condition that prevents safe authoring.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitPolicyError {
    /// The repository has no gitserious.toml.
    NotInitialized,
}

impl Display for CommitPolicyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInitialized => formatter.write_str(
                "gitserious is not initialized; run `gitserious init` before committing",
            ),
        }
    }
}

impl Error for CommitPolicyError {}

/// Failure to prepare a commit or to accept its reviewed message.
#[derive(Debug)]
pub enum CreateCommitError<LocatorError, StoreError> {
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
    UnknownCommitType {
        requested: CommitTypeId,
        available: Vec<CommitTypeId>,
    },
    AuthoredTypeMismatch {
        expected: CommitTypeId,
        actual: CommitTypeId,
    },
    InvalidDraft(CommitValidationErrors),
}

/// Policy snapshot preparation with the same typed errors as reviewed-message validation.
pub type CommitPreparationResult<L, S> =
    Result<(crate::RepositoryRoot, CommitAuthoringContext), CreateCommitError<L, S>>;

impl<L: Display, S: Display> Display for CreateCommitError<L, S> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReviewedMessageMismatch => {
                formatter.write_str("reviewed commit bytes do not match the selected taxonomy")
            }
            Self::UnknownTaxonomy {
                requested,
                available,
            } => write!(
                formatter,
                "taxonomy \"{requested}\" is unavailable; choose one of: {}",
                display_ids(available)
            ),
            Self::AuthoredTaxonomyMismatch { expected, actual } => write!(
                formatter,
                "authored taxonomy \"{actual}\" does not match requested taxonomy \"{expected}\""
            ),
            Self::InvalidContext(error) => formatter.write_str(error),
            Self::Repository(error) => Display::fmt(error, formatter),
            Self::Store(error) => Display::fmt(error, formatter),
            Self::Policy(error) => Display::fmt(error, formatter),
            Self::UnknownCommitType {
                requested,
                available,
            } => write!(
                formatter,
                "commit type \"{requested}\" is unavailable; choose one of: {}",
                display_ids(available)
            ),
            Self::AuthoredTypeMismatch { expected, actual } => write!(
                formatter,
                "authored type \"{actual}\" does not match requested type \"{expected}\""
            ),
            Self::InvalidDraft(error) => Display::fmt(error, formatter),
        }
    }
}

impl<L: Error + 'static, S: Error + 'static> Error for CreateCommitError<L, S> {}

fn display_ids<T: Display>(ids: &[T]) -> String {
    ids.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Loads persisted project policy without opening a UI or writing to Git.
pub fn prepare_commit_with_taxonomy<L, S>(
    locator: &L,
    store: &S,
    start: &Path,
) -> CommitPreparationResult<L::Error, S::Error>
where
    L: RepositoryLocator + ?Sized,
    S: ProjectConfigurationStore + ?Sized,
{
    let root = locator
        .locate(start)
        .map_err(CreateCommitError::Repository)?;
    let project = store
        .load(&root)
        .map_err(CreateCommitError::Store)?
        .ok_or(CreateCommitError::Policy(CommitPolicyError::NotInitialized))?
        .configuration;
    let context = CommitAuthoringContext::new(
        project.taxonomies().to_vec(),
        project.default_taxonomy(),
        None,
    )
    .map_err(CreateCommitError::InvalidContext)?;
    Ok((root, context))
}

/// Checks that the bytes approved in review exactly match the captured policy.
pub fn validate_reviewed_commit<L, S>(
    context: &CommitAuthoringContext,
    authored: &crate::AuthoredCommit,
) -> Result<gitserious_core::CommitMessage, CreateCommitError<L, S>> {
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
    find_definition::<L, S>(taxonomy.definitions(), authored.draft().commit_type())?;
    let message = taxonomy
        .validate(authored.draft())
        .into_message()
        .map_err(CreateCommitError::InvalidDraft)?;
    let reviewed = authored.reviewed_message();
    if reviewed != &message {
        return Err(CreateCommitError::ReviewedMessageMismatch);
    }
    Ok(reviewed.clone())
}

fn find_definition<'a, L, S>(
    definitions: &'a [CommitTypeDefinition],
    requested: &CommitTypeId,
) -> Result<&'a CommitTypeDefinition, CreateCommitError<L, S>> {
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
