mod support;

use std::cell::RefCell;
use std::error::Error;
use std::fs;
use std::io;

use gitserious_app::{
    AuthoredCommit, CommitAuthoringContext, CommitAuthoringOutcome, CommitDraftAuthor,
    CommitDraftAuthorOutcome, create_commit_with_taxonomy,
};
use gitserious_core::{
    AuthoredProperty, CommitDraft, CommitSubject, CommitTypeDefinition, CommitTypeId,
    PropertyRequirement, PropertyValue, PropertyValues, TaxonomyId, built_in_taxonomies,
};
use gitserious_fs::{GitCommitWriter, GitRepositoryLocator, TomlProjectStateStore};
use support::{Home, git, head_exists, repository, stderr, stdout, with_stale_fingerprint};

#[test]
fn taxonomy_overrides_leave_project_files_unchanged_before_terminal_authoring()
-> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;
    home.init(repository.path())?;
    let config = fs::read(repository.path().join("gitserious.toml"))?;
    let lock = fs::read(repository.path().join("gitserious.lock"))?;
    let infra_type = built_in_taxonomies()[2].commit_types()[0].id().to_string();

    for arguments in [
        vec!["commit", "--taxonomy", "research", "--type", "fix"],
        vec!["commit", "--taxonomy", "infra-ops", "--type", &infra_type],
    ] {
        let output = home.run(repository.path(), &arguments)?;
        assert!(!output.status.success());
        assert_eq!(
            stderr(&output),
            "error: commit authoring requires an interactive terminal\n"
        );
        assert_eq!(fs::read(repository.path().join("gitserious.toml"))?, config);
        assert_eq!(fs::read(repository.path().join("gitserious.lock"))?, lock);
        assert!(!head_exists(repository.path())?);
    }
    Ok(())
}

#[derive(Default)]
struct ReviewedAuthor(RefCell<String>);

impl CommitDraftAuthor for ReviewedAuthor {
    type Error = io::Error;

    fn author(
        &self,
        _: &[CommitTypeDefinition],
        _: Option<&CommitTypeDefinition>,
    ) -> Result<CommitDraftAuthorOutcome, Self::Error> {
        Err(io::Error::other("expected taxonomy context"))
    }

    fn author_with_context(
        &self,
        context: &CommitAuthoringContext,
    ) -> Result<CommitAuthoringOutcome, Self::Error> {
        let taxonomy = context.initial_taxonomy();
        let definition = context
            .preselected_type()
            .ok_or_else(|| io::Error::other("expected preselected type"))?;
        let properties = definition
            .properties()
            .iter()
            .filter(|property| property.requirement() == &PropertyRequirement::Required)
            .map(|property| {
                PropertyValue::new(format!("reviewed {}", property.key()))
                    .map(|value| {
                        AuthoredProperty::new(property.key().clone(), PropertyValues::single(value))
                    })
                    .map_err(io::Error::other)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let draft = CommitDraft::new(
            definition.id().clone(),
            None,
            CommitSubject::new("preserve reviewed research context").map_err(io::Error::other)?,
            properties,
        )
        .map_err(io::Error::other)?;
        let message = taxonomy.render(&draft).map_err(io::Error::other)?;
        message.as_str().clone_into(&mut self.0.borrow_mut());
        Ok(CommitAuthoringOutcome::Authored(AuthoredCommit::reviewed(
            taxonomy.id().clone(),
            draft,
            message,
        )))
    }
}

#[test]
fn git_stores_the_exact_reviewed_taxonomy_message() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;
    home.init(repository.path())?;
    let config = fs::read(repository.path().join("gitserious.toml"))?;
    let lock = fs::read(repository.path().join("gitserious.lock"))?;
    fs::write(repository.path().join("change.txt"), "reviewed change\n")?;
    git(repository.path(), &["add", "change.txt"])?;
    let author = ReviewedAuthor::default();

    create_commit_with_taxonomy(
        &GitRepositoryLocator,
        &TomlProjectStateStore,
        &author,
        &GitCommitWriter,
        repository.path(),
        Some(&TaxonomyId::new("research")?),
        Some(&CommitTypeId::new("fix")?),
    )?;

    let object = stdout(&git(repository.path(), &["cat-file", "commit", "HEAD"])?);
    let (_, message) = object.split_once("\n\n").ok_or("missing commit body")?;
    assert_eq!(message, *author.0.borrow());
    assert!(message.contains("\nsymptom:\nreviewed symptom\n"));
    assert!(message.contains("Gitserious-Taxonomy: research@1\n"));
    assert_eq!(fs::read(repository.path().join("gitserious.toml"))?, config);
    assert_eq!(fs::read(repository.path().join("gitserious.lock"))?, lock);
    assert_eq!(
        stdout(&git(
            repository.path(),
            &["ls-tree", "--name-only", "HEAD"]
        )?),
        "change.txt\n"
    );
    Ok(())
}

#[test]
fn project_policy_is_resolved_before_terminal_authoring() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;

    let absent = home.run(repository.path(), &["commit", "--type", "feat"])?;
    assert_eq!(absent.status.code(), Some(1));
    assert!(stdout(&absent).is_empty());
    assert!(
        stderr(&absent).contains("not initialized"),
        "{}",
        stderr(&absent)
    );

    home.init(repository.path())?;
    let lock = repository.path().join("gitserious.lock");
    fs::write(&lock, with_stale_fingerprint(&fs::read_to_string(&lock)?)?)?;
    let stale = home.run(repository.path(), &["commit", "--type", "feat"])?;
    assert_eq!(stale.status.code(), Some(1));
    assert!(stdout(&stale).is_empty());
    assert!(stderr(&stale).contains("disagree"), "{}", stderr(&stale));
    assert!(!head_exists(repository.path())?);
    Ok(())
}

#[test]
fn unavailable_type_is_rejected_before_terminal_authoring() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;
    home.init(repository.path())?;

    let output = home.run(repository.path(), &["commit", "--type", "custom"])?;

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
    let error = stderr(&output);
    assert!(error.contains("custom"));
    assert!(error.contains("choose one of: feat, fix"), "{error}");
    assert!(error.contains("revert"));
    assert!(!error.contains("interactive terminal"));
    assert!(!head_exists(repository.path())?);
    Ok(())
}

#[test]
fn bare_and_typed_commits_reject_non_terminal_execution_without_calling_git()
-> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;
    home.init(repository.path())?;
    git(repository.path(), &["config", "core.editor", "false"])?;
    fs::write(repository.path().join("staged.txt"), "staged\n")?;
    git(repository.path(), &["add", "staged.txt"])?;

    for arguments in [&["commit"][..], &["commit", "--type", "feat"][..]] {
        let output = home.run(repository.path(), arguments)?;
        assert_eq!(output.status.code(), Some(1));
        assert!(stdout(&output).is_empty());
        assert_eq!(
            stderr(&output),
            "error: commit authoring requires an interactive terminal\n"
        );
        assert!(!head_exists(repository.path())?);
    }
    Ok(())
}
