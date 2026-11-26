mod support;

use std::error::Error;
use std::fs;
use std::path::Path;

use gitserious_app::{CommitWriter, RepositoryRoot};
use gitserious_core::{
    AuthoredProperty, CommitDraft, CommitMessage, CommitScope, CommitSubject, PropertyKey,
    PropertyValue, built_in_commit_types, validate_commit_draft,
};
use gitserious_fs::{GitCommitError, GitCommitWriter};
use support::{git, git_command, init, repository};

fn root(path: &Path) -> Result<RepositoryRoot, Box<dyn Error>> {
    Ok(RepositoryRoot::new(path.to_path_buf())?)
}

fn property(key: &str, text: &str) -> Result<AuthoredProperty, Box<dyn Error>> {
    Ok(AuthoredProperty::new(
        PropertyKey::new(key)?,
        PropertyValue::new(text)?,
    ))
}

fn feat_message() -> Result<CommitMessage, Box<dyn Error>> {
    feat_message_with_intent("exercise Git")
}

fn feat_message_with_intent(intent: &str) -> Result<CommitMessage, Box<dyn Error>> {
    let definition = &built_in_commit_types()[0];
    let draft = CommitDraft::new(
        definition.id().clone(),
        Some(CommitScope::new("fs")?),
        CommitSubject::new("create commit")?,
        vec![
            property("intent", intent)?,
            property("decision", "record the staged index")?,
        ],
    )?;
    Ok(validate_commit_draft(definition, &draft).into_message()?)
}

fn committed_message(directory: &Path) -> Result<String, Box<dyn Error>> {
    let object = String::from_utf8(git(directory, &["cat-file", "commit", "HEAD"])?.stdout)?;
    object
        .split_once("\n\n")
        .map(|(_, message)| message.to_owned())
        .ok_or_else(|| "commit object has no message separator".into())
}

fn shell_quote(path: &Path) -> String {
    let value = path.to_string_lossy();
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), Box<dyn Error>> {
    Ok(())
}

#[test]
fn writer_commits_exact_message_and_only_the_staged_index() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    fs::write(repository.path().join("staged.txt"), "staged\n")?;
    fs::write(repository.path().join("untracked.txt"), "untracked\n")?;
    git(repository.path(), &["add", "staged.txt"])?;
    let message = feat_message()?;

    let output = GitCommitWriter.commit(&root(repository.path())?, &message)?;
    assert!(String::from_utf8_lossy(output.stdout()).contains("root-commit"));
    assert_eq!(committed_message(repository.path())?, message.as_str());

    let tree = git(repository.path(), &["ls-tree", "--name-only", "HEAD"])?;
    assert_eq!(String::from_utf8(tree.stdout)?, "staged.txt\n");
    assert!(repository.path().join("untracked.txt").exists());
    Ok(())
}

#[test]
fn writer_names_the_commit_it_created_as_git_stores_it() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    fs::write(repository.path().join("staged.txt"), "staged\n")?;
    git(repository.path(), &["add", "staged.txt"])?;

    let output = GitCommitWriter.commit(&root(repository.path())?, &feat_message()?)?;

    let abbreviated = git(repository.path(), &["rev-parse", "--short", "HEAD"])?;
    let created = output.created().ok_or("the created commit is unnamed")?;
    assert_eq!(
        created.abbreviated_oid,
        String::from_utf8(abbreviated.stdout)?.trim()
    );
    assert_eq!(created.summary, "feat(fs): create commit");
    Ok(())
}

#[test]
fn writer_preserves_the_exact_wrapped_canonical_message() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    fs::write(repository.path().join("staged.txt"), "staged\n")?;
    git(repository.path(), &["add", "staged.txt"])?;
    let intent = format!("{} durable", "x".repeat(75));
    let message = feat_message_with_intent(&intent)?;

    GitCommitWriter.commit(&root(repository.path())?, &message)?;

    assert_eq!(
        message.as_str(),
        format!(
            "feat(fs): create commit\n\nintent:\n{}\ndurable\n\ndecision:\nrecord the staged index\n",
            "x".repeat(75),
        )
    );
    assert_eq!(committed_message(repository.path())?, message.as_str());
    Ok(())
}

#[test]
fn normal_pre_commit_and_commit_msg_hooks_run() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let hooks = repository.path().join("hooks");
    fs::create_dir(&hooks)?;
    let marker = repository.path().join("hook-ran");
    let pre_commit = hooks.join("pre-commit");
    fs::write(
        &pre_commit,
        format!("#!/bin/sh\nprintf ran > {}\n", shell_quote(&marker)),
    )?;
    make_executable(&pre_commit)?;
    let commit_msg = hooks.join("commit-msg");
    fs::write(
        &commit_msg,
        "#!/bin/sh\ngrep -q '^feat(fs): create commit$' \"$1\"\n",
    )?;
    make_executable(&commit_msg)?;
    git(
        repository.path(),
        &["config", "core.hooksPath", hooks.to_string_lossy().as_ref()],
    )?;
    fs::write(repository.path().join("change.txt"), "change\n")?;
    git(repository.path(), &["add", "change.txt"])?;

    GitCommitWriter.commit(&root(repository.path())?, &feat_message()?)?;
    assert_eq!(fs::read_to_string(marker)?, "ran");
    Ok(())
}

#[test]
fn hook_rejection_and_empty_index_preserve_exact_git_diagnostics() -> Result<(), Box<dyn Error>> {
    let rejected_repository = repository()?;
    let hooks = rejected_repository.path().join("hooks");
    fs::create_dir(&hooks)?;
    let pre_commit = hooks.join("pre-commit");
    fs::write(
        &pre_commit,
        "#!/bin/sh\necho 'policy rejected commit' >&2\nexit 9\n",
    )?;
    make_executable(&pre_commit)?;
    git(
        rejected_repository.path(),
        &["config", "core.hooksPath", hooks.to_string_lossy().as_ref()],
    )?;
    fs::write(rejected_repository.path().join("change.txt"), "change\n")?;
    git(rejected_repository.path(), &["add", "change.txt"])?;
    let error = GitCommitWriter
        .commit(&root(rejected_repository.path())?, &feat_message()?)
        .err()
        .ok_or("rejecting hook unexpectedly committed")?;
    let error_text = error.to_string();
    assert!(matches!(error, GitCommitError::Rejected { .. }));
    assert!(error_text.contains("policy rejected commit"));
    assert!(
        !git_command(rejected_repository.path())
            .args(["rev-parse", "--verify", "HEAD"])
            .output()?
            .status
            .success()
    );

    let empty = repository()?;
    let error = GitCommitWriter
        .commit(&root(empty.path())?, &feat_message()?)
        .err()
        .ok_or("empty index unexpectedly committed")?;
    let error_text = error.to_string();
    assert!(matches!(error, GitCommitError::Rejected { .. }));
    assert!(!error_text.is_empty());
    Ok(())
}

#[test]
fn repository_paths_with_spaces_are_supported() -> Result<(), Box<dyn Error>> {
    let parent = tempfile::tempdir()?;
    let repository = parent.path().join("repository with spaces");
    fs::create_dir(&repository)?;
    init(&repository)?;
    fs::write(repository.join("change.txt"), "change\n")?;
    git(&repository, &["add", "change.txt"])?;

    GitCommitWriter.commit(&root(&repository)?, &feat_message()?)?;
    assert!(
        git_command(&repository)
            .args(["rev-parse", "--verify", "HEAD"])
            .output()?
            .status
            .success()
    );
    Ok(())
}
