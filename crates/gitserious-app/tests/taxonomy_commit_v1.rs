use std::cell::RefCell;
use std::convert::Infallible;
use std::error::Error;
use std::path::Path;

use gitserious_app::{
    AuthoredCommit, CommitAuthoringContext, CommitAuthoringOutcome, CommitDraftAuthor,
    CommitDraftAuthorOutcome, CommitOutput, CommitWriter, ProjectConfiguration,
    ProjectConfigurationStore, ProjectRevision, RepositoryLocator, RepositoryRoot, StoredProject,
    create_commit_with_taxonomy,
};
use gitserious_core::{
    CommitDraft, CommitMessage, CommitSubject, CommitTypeDefinition, CommitTypeId, Description,
    SchemaVersion, Taxonomy, TaxonomyId, TaxonomyVersion,
};

struct Locator(RepositoryRoot);
impl RepositoryLocator for Locator {
    type Error = Infallible;

    fn is_not_repository(_: &Self::Error) -> bool {
        false
    }
    fn locate(&self, _: &Path) -> Result<RepositoryRoot, Self::Error> {
        Ok(self.0.clone())
    }
}

struct Store(ProjectConfiguration);
impl ProjectConfigurationStore for Store {
    type Error = Infallible;
    fn load(&self, _: &RepositoryRoot) -> Result<Option<StoredProject>, Self::Error> {
        Ok(Some(StoredProject {
            configuration: self.0.clone(),
            revision: ProjectRevision::of_bytes(b"project"),
        }))
    }
    fn create(
        &self,
        _: &RepositoryRoot,
        _: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error> {
        unreachable!("commit never writes project configuration")
    }
    fn replace(
        &self,
        _: &RepositoryRoot,
        _: ProjectRevision,
        _: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error> {
        unreachable!("commit never writes project configuration")
    }
    fn ensure_local_state(&self, _: &RepositoryRoot) -> Result<(), Self::Error> {
        unreachable!("commit never writes local state")
    }
}

struct Author;
impl CommitDraftAuthor for Author {
    type Error = Infallible;
    fn author(
        &self,
        _: &[CommitTypeDefinition],
        _: Option<&CommitTypeDefinition>,
    ) -> Result<CommitDraftAuthorOutcome, Self::Error> {
        Ok(CommitDraftAuthorOutcome::Cancelled)
    }
    fn author_with_context(
        &self,
        context: &CommitAuthoringContext,
    ) -> Result<CommitAuthoringOutcome, Self::Error> {
        let selected = context.initial_taxonomy();
        let draft = CommitDraft::new(
            CommitTypeId::new("note").unwrap_or_else(|_| unreachable!()),
            None,
            CommitSubject::new("portable policy").unwrap_or_else(|_| unreachable!()),
            Vec::new(),
        )
        .unwrap_or_else(|_| unreachable!());
        let message = selected
            .validate(&draft)
            .into_message()
            .unwrap_or_else(|_| unreachable!());
        Ok(CommitAuthoringOutcome::Authored(AuthoredCommit::reviewed(
            selected.id().clone(),
            draft,
            message,
        )))
    }
}

#[derive(Default)]
struct Writer(RefCell<Option<CommitMessage>>);
impl CommitWriter for Writer {
    type Error = Infallible;
    fn commit(
        &self,
        _: &RepositoryRoot,
        message: &CommitMessage,
    ) -> Result<CommitOutput, Self::Error> {
        *self.0.borrow_mut() = Some(message.clone());
        Ok(CommitOutput::default())
    }
}

#[test]
fn commit_uses_only_the_embedded_project_taxonomy() -> Result<(), Box<dyn Error>> {
    let taxonomy = Taxonomy::new(
        TaxonomyId::new("portable")?,
        TaxonomyVersion::V1,
        Description::new("Portable test policy.")?,
        None,
        vec![CommitTypeDefinition::new(
            SchemaVersion::V1,
            CommitTypeId::new("note")?,
            "Record a note.",
            Vec::new(),
        )?],
    )?;
    let project = ProjectConfiguration::new(1, taxonomy.id().clone(), vec![taxonomy])?;
    let root = RepositoryRoot::new(std::env::temp_dir())?;
    let writer = Writer::default();
    create_commit_with_taxonomy(
        &Locator(root),
        &Store(project),
        &Author,
        &writer,
        Path::new("."),
    )?;
    let message = writer.0.borrow();
    let message = message.as_ref().ok_or("writer was not called")?;
    assert!(
        message
            .as_str()
            .contains("Gitserious-Taxonomy: portable-v1")
    );
    assert!(!message.as_str().contains("Gitserious-Template"));
    Ok(())
}
