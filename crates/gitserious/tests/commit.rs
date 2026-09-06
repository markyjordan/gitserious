use std::error::Error;

#[test]
fn template_overrides_leave_project_files_unchanged_before_terminal_authoring()
-> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    initialize(repository.path())?;
    let config = fs::read(repository.path().join("gitserious.toml"))?;
    let lock = fs::read(repository.path().join("gitserious.lock"))?;
    for arguments in [
        vec!["commit", "--template", "ml-research", "--type", "fix"],
        vec!["commit", "--template", "infra-ops", "--type", "deploy"],
    ] {
        let output = run(repository.path(), &arguments)?;
        assert!(!output.status.success());
        assert!(
            stderr(&output).contains("interactive terminal"),
            "{}",
            stderr(&output)
        );
        assert_eq!(fs::read(repository.path().join("gitserious.toml"))?, config);
        assert_eq!(fs::read(repository.path().join("gitserious.lock"))?, lock);
        assert!(!head_exists(repository.path())?);
    }
    Ok(())
}
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

#[derive(Default)]
struct ReviewedAuthor(std::cell::RefCell<String>);

impl gitserious_app::CommitDraftAuthor for ReviewedAuthor {
    type Error = std::io::Error;

    fn author(
        &self,
        _: &[gitserious_core::CommitTypeDefinition],
        _: Option<&gitserious_core::CommitTypeDefinition>,
    ) -> Result<gitserious_app::CommitDraftAuthorOutcome, Self::Error> {
        Err(std::io::Error::other("expected template context"))
    }

    fn author_with_context(
        &self,
        context: &gitserious_app::CommitAuthoringContext,
    ) -> Result<gitserious_app::CommitAuthoringOutcome, Self::Error> {
        use gitserious_core::{
            AuthoredProperty, CommitDraft, CommitSubject, PropertyRequirement, PropertyValue,
            PropertyValues,
        };
        let template = context.initial_template();
        let definition = context
            .preselected_type()
            .ok_or_else(|| std::io::Error::other("expected preselected type"))?;
        let properties = definition
            .properties()
            .iter()
            .filter(|property| property.requirement() == &PropertyRequirement::Required)
            .map(|property| {
                PropertyValue::new(format!("reviewed {}", property.key()))
                    .map(|value| {
                        AuthoredProperty::new(property.key().clone(), PropertyValues::single(value))
                    })
                    .map_err(std::io::Error::other)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let draft = CommitDraft::new(
            definition.id().clone(),
            None,
            CommitSubject::new("preserve reviewed research context")
                .map_err(std::io::Error::other)?,
            properties,
        )
        .map_err(std::io::Error::other)?;
        let message = template.render(&draft).map_err(std::io::Error::other)?;
        message.as_str().clone_into(&mut self.0.borrow_mut());
        Ok(gitserious_app::CommitAuthoringOutcome::Authored(
            gitserious_app::AuthoredCommit::reviewed(template.id().clone(), draft, message),
        ))
    }
}

#[test]
fn git_stores_the_exact_reviewed_template_message() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    git_success(repository.path(), &["config", "commit.gpgsign", "false"])?;
    initialize(repository.path())?;
    let config = fs::read(repository.path().join("gitserious.toml"))?;
    let lock = fs::read(repository.path().join("gitserious.lock"))?;
    fs::write(repository.path().join("change.txt"), "reviewed change\n")?;
    git_success(repository.path(), &["add", "change.txt"])?;
    let author = ReviewedAuthor::default();
    gitserious_app::create_commit_with_template(
        &gitserious_fs::GitRepositoryLocator,
        &gitserious_fs::TomlProjectStateStore,
        &author,
        &gitserious_fs::GitCommitWriter,
        repository.path(),
        Some(&gitserious_core::TemplateId::new("ml-research")?),
        Some(&gitserious_core::CommitTypeId::new("fix")?),
    )?;
    let object = stdout(&git_success(
        repository.path(),
        &["cat-file", "commit", "HEAD"],
    )?);
    let (_, message) = object.split_once("\n\n").ok_or("missing commit body")?;
    assert_eq!(message, *author.0.borrow());
    assert!(message.contains("Gitserious-Template: ml-research@1\n"));
    assert_eq!(fs::read(repository.path().join("gitserious.toml"))?, config);
    assert_eq!(fs::read(repository.path().join("gitserious.lock"))?, lock);
    assert_eq!(
        stdout(&git_success(
            repository.path(),
            &["ls-tree", "--name-only", "HEAD"]
        )?),
        "change.txt\n"
    );
    Ok(())
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_gitserious")
}

fn repository() -> Result<TempDir, Box<dyn Error>> {
    let directory = tempfile::Builder::new()
        .prefix("gitserious commit ")
        .tempdir()?;
    git_success(directory.path(), &["init", "-q"])?;
    git_success(directory.path(), &["config", "user.name", "Git Serious"])?;
    git_success(
        directory.path(),
        &["config", "user.email", "gitserious@example.com"],
    )?;
    Ok(directory)
}

fn run(directory: &Path, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
    Ok(Command::new(binary())
        .args(arguments)
        .current_dir(directory)
        .output()?)
}

fn git(directory: &Path, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
    Ok(Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .output()?)
}

fn git_success(directory: &Path, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
    let output = git(directory, arguments)?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!(
            "git {} failed: {}{}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn initialize(directory: &Path) -> Result<(), Box<dyn Error>> {
    let output = run(directory, &["init"])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(stderr(&output).into())
    }
}

fn head_exists(directory: &Path) -> Result<bool, Box<dyn Error>> {
    Ok(git(directory, &["rev-parse", "--verify", "HEAD"])?
        .status
        .success())
}

#[test]
fn project_policy_is_resolved_before_terminal_authoring() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;

    let absent = run(repository.path(), &["commit", "--type", "feat"])?;
    assert_eq!(absent.status.code(), Some(1));
    assert!(stdout(&absent).is_empty());
    assert!(stderr(&absent).contains("not initialized"));

    initialize(repository.path())?;
    let lock = repository.path().join("gitserious.lock");
    let stale = fs::read_to_string(&lock)?.replace(
        "config-fingerprint = \"sha256:e",
        "config-fingerprint = \"sha256:a",
    );
    fs::write(lock, stale)?;
    let stale = run(repository.path(), &["commit", "--type", "feat"])?;
    assert_eq!(stale.status.code(), Some(1));
    assert!(stdout(&stale).is_empty());
    assert!(stderr(&stale).contains("stale"));
    assert!(!head_exists(repository.path())?);
    Ok(())
}

#[test]
fn unavailable_type_is_rejected_before_terminal_authoring() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    initialize(repository.path())?;

    let output = run(repository.path(), &["commit", "--type", "custom"])?;

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
    let error = stderr(&output);
    assert!(error.contains("custom"));
    assert!(error.contains("choose one of: feat, fix"));
    assert!(error.contains("revert"));
    assert!(!error.contains("interactive terminal"));
    assert!(!head_exists(repository.path())?);
    Ok(())
}

#[test]
fn bare_and_typed_commits_reject_non_terminal_execution_without_calling_git()
-> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    initialize(repository.path())?;
    git_success(repository.path(), &["config", "core.editor", "false"])?;
    fs::write(repository.path().join("staged.txt"), "staged\n")?;
    git_success(repository.path(), &["add", "staged.txt"])?;

    for arguments in [&["commit"][..], &["commit", "--type", "feat"][..]] {
        let output = run(repository.path(), arguments)?;
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
