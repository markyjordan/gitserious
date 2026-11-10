mod support;

use std::error::Error;
use std::fs;

use gitserious_app::{AuthoredCommit, CommitAuthoringContext, CommitServices, CommitWorkspace};
use gitserious_core::{
    AuthoredProperty, CommitDraft, CommitSubject, PropertyRequirement, PropertyValue, TaxonomyId,
};
use gitserious_fs::{
    GitCommitWriter, GitRepositoryLocator, GitRepositoryStatusReader, TomlProjectStore,
};
use support::{Home, git, head_exists, repository, stderr, stdout};

const PROJECT_OWNED_CONVENTIONAL: &str = r#"config-version = 1
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
"#;

#[test]
fn commit_without_a_terminal_stops_before_the_commit_page() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;

    let absent = home.run(repository.path(), &["commit"])?;
    assert_eq!(absent.status.code(), Some(1));
    assert!(stdout(&absent).is_empty());
    assert_eq!(
        stderr(&absent),
        "error: commit requires an interactive terminal\n"
    );
    assert!(!repository.path().join("gitserious.toml").exists());

    home.init(repository.path())?;
    let config = repository.path().join("gitserious.toml");
    let valid = fs::read_to_string(&config)?;
    let broken = valid.replace(
        "default-taxonomy = \"conventional\"",
        "default-taxonomy = \"missing\"",
    );
    fs::write(&config, &broken)?;
    let invalid = home.run(repository.path(), &["commit"])?;
    assert_eq!(invalid.status.code(), Some(1));
    assert!(stdout(&invalid).is_empty());
    assert_eq!(
        stderr(&invalid),
        "error: commit requires an interactive terminal\n"
    );
    assert_eq!(fs::read_to_string(&config)?, broken);
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
        "error: commit requires an interactive terminal\n"
    );
    assert_eq!(fs::read_to_string(&library)?, "not = [toml\n");
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
        "error: commit requires an interactive terminal\n"
    );
    assert_eq!(fs::read(repository.path().join("gitserious.toml"))?, config);
    assert!(!repository.path().join("gitserious.lock").exists());
    assert!(!head_exists(repository.path())?);
    Ok(())
}

fn reviewed_research_fix(
    context: &CommitAuthoringContext,
) -> Result<AuthoredCommit, Box<dyn Error>> {
    let taxonomy = context
        .find_taxonomy(&TaxonomyId::new("research")?)
        .ok_or("expected research in project policy")?;
    let definition = taxonomy
        .definitions()
        .iter()
        .find(|definition| definition.id().as_str() == "fix")
        .ok_or("expected research fix")?;
    let properties = definition
        .properties()
        .iter()
        .filter(|property| property.requirement() == &PropertyRequirement::Required)
        .map(|property| {
            Ok(AuthoredProperty::new(
                property.key().clone(),
                PropertyValue::new(format!("reviewed {}", property.key()))?,
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    let draft = CommitDraft::new(
        definition.id().clone(),
        None,
        CommitSubject::new("preserve reviewed research context")?,
        properties,
    )?;
    let message = taxonomy.validate(&draft).into_message()?;
    Ok(AuthoredCommit::reviewed(
        taxonomy.id().clone(),
        draft,
        message,
    ))
}

#[test]
fn git_stores_the_exact_reviewed_taxonomy_message() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;
    home.init(repository.path())?;
    let config = fs::read(repository.path().join("gitserious.toml"))?;
    fs::write(repository.path().join("change.txt"), "reviewed change\n")?;
    git(repository.path(), &["add", "change.txt"])?;
    let commit = CommitServices::new(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &GitRepositoryStatusReader,
        &GitCommitWriter,
        repository.path(),
    );
    let authored = reviewed_research_fix(&commit.prepare_commit()?)?;

    commit.submit_commit(&authored)?;

    let object = stdout(&git(repository.path(), &["cat-file", "commit", "HEAD"])?);
    let (_, message) = object.split_once("\n\n").ok_or("missing commit body")?;
    assert_eq!(
        message,
        authored
            .reviewed_message()
            .ok_or("no approved bytes")?
            .as_str()
    );
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
fn project_owned_conventional_replaces_the_built_in_definition() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    fs::write(
        repository.path().join("gitserious.toml"),
        PROJECT_OWNED_CONVENTIONAL,
    )?;
    fs::write(repository.path().join("change.txt"), "staged\n")?;
    git(repository.path(), &["add", "change.txt"])?;

    let context = CommitServices::new(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &GitRepositoryStatusReader,
        &GitCommitWriter,
        repository.path(),
    )
    .prepare_commit()?;

    assert_eq!(
        context
            .initial_taxonomy()
            .definitions()
            .iter()
            .map(|definition| definition.id().as_str())
            .collect::<Vec<_>>(),
        ["ship"]
    );
    assert!(!head_exists(repository.path())?);
    Ok(())
}
