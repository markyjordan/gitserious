use std::cell::RefCell;
use std::convert::Infallible;
use std::error::Error;
use std::path::Path;

use gitserious_app::{
    AuthoredCommit, CommitAuthoringContext, CommitAuthoringOutcome, CommitDraftAuthor,
    CommitDraftAuthorOutcome, CommitOutput, CommitWriter, ProjectConfig, ProjectLock, ProjectState,
    ProjectStateStore, RepositoryLocator, RepositoryRoot, create_commit_with_taxonomy,
    fingerprint_project_config,
};
use gitserious_core::{
    CommitDraft, CommitMessage, CommitSubject, CommitTypeDefinition, CommitTypeId, Description,
    SchemaVersion, Taxonomy, TaxonomyId, TaxonomyVersion,
};

struct Locator(RepositoryRoot);
impl RepositoryLocator for Locator {
    type Error = Infallible;
    fn locate(&self, _: &Path) -> Result<RepositoryRoot, Self::Error> {
        Ok(self.0.clone())
    }
}

struct Store(ProjectState);
impl ProjectStateStore for Store {
    type Error = Infallible;
    fn inspect(&self, _: &RepositoryRoot) -> Result<ProjectState, Self::Error> {
        Ok(self.0.clone())
    }
    fn ensure_local_state(&self, _: &RepositoryRoot) -> Result<(), Self::Error> {
        Ok(())
    }
    fn initialize(
        &self,
        _: &RepositoryRoot,
        _: &ProjectConfig,
        _: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn create_lock(&self, _: &RepositoryRoot, _: &ProjectLock) -> Result<(), Self::Error> {
        Ok(())
    }
    fn replace_lock(
        &self,
        _: &RepositoryRoot,
        _: &ProjectLock,
        _: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn compare_and_swap(
        &self,
        _: &RepositoryRoot,
        _: &ProjectConfig,
        _: &ProjectLock,
        _: &ProjectConfig,
        _: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Ok(())
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
        let message = selected.render(&draft).unwrap_or_else(|_| unreachable!());
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
fn commit_uses_only_the_portable_lock_snapshot() -> Result<(), Box<dyn Error>> {
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
    let config = ProjectConfig::new(
        1,
        Some(taxonomy.id().clone()),
        Some(vec![taxonomy.id().clone()]),
    )?;
    let lock = ProjectLock::new(
        1,
        fingerprint_project_config(&config),
        taxonomy.id().clone(),
        vec![taxonomy],
    )?;
    let root = RepositoryRoot::new(std::env::temp_dir())?;
    let writer = Writer::default();
    create_commit_with_taxonomy(
        &Locator(root),
        &Store(ProjectState::Initialized { config, lock }),
        &Author,
        &writer,
        Path::new("."),
        None,
        None,
    )?;
    let message = writer.0.borrow();
    let message = message.as_ref().ok_or("writer was not called")?;
    assert!(message.as_str().contains("Gitserious-Taxonomy: portable@1"));
    assert!(!message.as_str().contains("Gitserious-Template"));
    Ok(())
}
