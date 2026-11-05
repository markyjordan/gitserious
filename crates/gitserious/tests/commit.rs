mod support;

use std::cell::RefCell;
use std::error::Error;
use std::fs;
use std::io;

use gitserious_app::{
    AuthoredCommit, CommitAuthoringContext, CommitAuthoringOutcome, CommitDraftAuthor,
    CommitDraftAuthorOutcome, CommitOutcome, create_commit_with_taxonomy,
};
use gitserious_core::{
    AuthoredProperty, CommitDraft, CommitSubject, CommitTypeDefinition, PropertyRequirement,
    PropertyValue, TaxonomyId,
};
use gitserious_fs::{GitCommitWriter, GitRepositoryLocator, TomlProjectStore};
use support::{Home, git, head_exists, repository, stderr, stdout};

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
        let research = TaxonomyId::new("research").map_err(io::Error::other)?;
        let taxonomy = context
            .find_taxonomy(&research)
            .ok_or_else(|| io::Error::other("expected research in project policy"))?;
        let definition = taxonomy
            .definitions()
            .iter()
            .find(|definition| definition.id().as_str() == "fix")
            .ok_or_else(|| io::Error::other("expected research fix"))?;
        let properties = definition
            .properties()
            .iter()
            .filter(|property| property.requirement() == &PropertyRequirement::Required)
            .map(|property| {
                PropertyValue::new(format!("reviewed {}", property.key()))
                    .map(|value| AuthoredProperty::new(property.key().clone(), value))
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
        let message = taxonomy
            .validate(&draft)
            .into_message()
            .map_err(io::Error::other)?;
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
    fs::write(repository.path().join("change.txt"), "reviewed change\n")?;
    git(repository.path(), &["add", "change.txt"])?;
    let author = ReviewedAuthor::default();

    create_commit_with_taxonomy(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &author,
        &GitCommitWriter,
        repository.path(),
    )?;

    let object = stdout(&git(repository.path(), &["cat-file", "commit", "HEAD"])?);
    let (_, message) = object.split_once("\n\n").ok_or("missing commit body")?;
    assert_eq!(message, *author.0.borrow());
    assert!(message.contains("\nsymptom:\nreviewed symptom\n"));
    assert!(message.contains("Gitserious-Taxonomy: research-v1\n"));
    assert_eq!(fs::read(repository.path().join("gitserious.toml"))?, config);
    assert!(!repository.path().join("gitserious.lock").exists());
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

    let absent = home.run(repository.path(), &["commit"])?;
    assert_eq!(absent.status.code(), Some(1));
    assert!(stdout(&absent).is_empty());
    assert!(
        stderr(&absent).contains("not initialized"),
        "{}",
        stderr(&absent)
    );

    home.init(repository.path())?;
    let config = repository.path().join("gitserious.toml");
    let valid = fs::read_to_string(&config)?;
    fs::write(
        &config,
        valid.replace(
            "default-taxonomy = \"conventional\"",
            "default-taxonomy = \"missing\"",
        ),
    )?;
    let invalid = home.run(repository.path(), &["commit"])?;
    assert_eq!(invalid.status.code(), Some(1));
    assert!(stdout(&invalid).is_empty());
    assert!(
        stderr(&invalid).contains("invalid project configuration")
            && stderr(&invalid).contains("default taxonomy \"missing\""),
        "{}",
        stderr(&invalid)
    );
    assert!(!head_exists(repository.path())?);
    Ok(())
}

#[test]
fn a_cloned_project_authors_without_any_usable_library() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let author = Home::new()?;
    author.init(repository.path())?;
    let contributor = Home::new()?;
    let library = contributor.library_path();
    fs::create_dir_all(library.parent().ok_or("library has a parent")?)?;
    fs::write(&library, "not = [toml\n")?;

    let output = contributor.run(repository.path(), &["commit"])?;

    assert_eq!(
        stderr(&output),
        "error: commit authoring requires an interactive terminal\n"
    );
    assert_eq!(fs::read_to_string(&library)?, "not = [toml\n");
    Ok(())
}

#[derive(Default)]
struct OfferedTypes(RefCell<Vec<String>>);

impl CommitDraftAuthor for OfferedTypes {
    type Error = io::Error;

    fn author(
        &self,
        definitions: &[CommitTypeDefinition],
        _: Option<&CommitTypeDefinition>,
    ) -> Result<CommitDraftAuthorOutcome, Self::Error> {
        self.0.borrow_mut().extend(
            definitions
                .iter()
                .map(|definition| definition.id().to_string()),
        );
        Ok(CommitDraftAuthorOutcome::Cancelled)
    }
}

#[test]
fn project_owned_conventional_replaces_the_built_in_definition() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    fs::write(
        repository.path().join("gitserious.toml"),
        r#"config-version = 1
default-taxonomy = "conventional"

[[taxonomies]]
id = "conventional"
version = 7
description = "A project-owned conventional taxonomy."

[[taxonomies.commit-types]]
id = "ship"
schema-version = 1
description = "Ships a change."
properties = []
"#,
    )?;

    let author = OfferedTypes::default();

    let outcome = create_commit_with_taxonomy(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &author,
        &GitCommitWriter,
        repository.path(),
    )?;

    assert_eq!(outcome, CommitOutcome::Cancelled);
    assert_eq!(*author.0.borrow(), ["ship"]);
    Ok(())
}

#[test]
fn commit_rejects_non_terminal_execution_without_touching_policy_or_git()
-> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;
    home.init(repository.path())?;
    let config = fs::read(repository.path().join("gitserious.toml"))?;
    git(repository.path(), &["config", "core.editor", "false"])?;
    fs::write(repository.path().join("staged.txt"), "staged\n")?;
    git(repository.path(), &["add", "staged.txt"])?;

    let output = home.run(repository.path(), &["commit"])?;

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
    assert_eq!(
        stderr(&output),
        "error: commit authoring requires an interactive terminal\n"
    );
    assert_eq!(fs::read(repository.path().join("gitserious.toml"))?, config);
    assert!(!repository.path().join("gitserious.lock").exists());
    assert!(!head_exists(repository.path())?);
    Ok(())
}
