use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use gitserious_app::{CommitDraftEditor, CommitWriter, RepositoryRoot};
use gitserious_core::{
    CommitMessage, built_in_commit_types, parse_commit_editor_document, render_commit_message,
};
use gitserious_fs::{
    GitCommitDraftEditor, GitCommitDraftEditorError, GitCommitError, GitCommitWriter,
};
use tempfile::TempDir;

fn git(directory: &Path, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
    Ok(Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .output()?)
}

fn git_ok(directory: &Path, arguments: &[&str]) -> Result<(), Box<dyn Error>> {
    let output = git(directory, arguments)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

fn repository() -> Result<TempDir, Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    git_ok(directory.path(), &["init", "-q"])?;
    git_ok(directory.path(), &["config", "user.name", "Test User"])?;
    git_ok(
        directory.path(),
        &["config", "user.email", "test@example.com"],
    )?;
    Ok(directory)
}

fn root(path: &Path) -> Result<RepositoryRoot, Box<dyn Error>> {
    Ok(RepositoryRoot::new(path.to_path_buf())?)
}

fn feat_message() -> Result<CommitMessage, Box<dyn Error>> {
    let definition = &built_in_commit_types()[0];
    let document = "feat(fs): create commit\n\nintent:\n  exercise Git\n\nbehavior:\n  record the staged index\n";
    let draft = parse_commit_editor_document(definition, document)?;
    Ok(render_commit_message(definition, &draft)?)
}

fn shell_quote(path: &Path) -> String {
    let value = path.to_string_lossy();
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn write_editor_script(directory: &Path, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let script = directory.join("editor script.sh");
    fs::write(&script, format!("#!/bin/sh\nset -eu\n{body}\n"))?;
    Ok(script)
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
fn configured_editor_with_arguments_and_spaced_paths_replaces_document()
-> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let fixture_directory = repository.path().join("editor fixtures");
    fs::create_dir(&fixture_directory)?;
    let replacement = fixture_directory.join("replacement document.txt");
    let expected =
        "feat(fs): edited subject\n\nintent:\n  edited intent\n\nbehavior:\n  edited behavior\n";
    fs::write(&replacement, expected)?;
    let script = write_editor_script(&fixture_directory, "cp \"$1\" \"$2\"")?;
    let command = format!("sh {} {}", shell_quote(&script), shell_quote(&replacement));
    git_ok(repository.path(), &["config", "core.editor", &command])?;

    let edited = GitCommitDraftEditor.edit(&root(repository.path())?, "initial document\n")?;
    assert_eq!(edited, expected);
    Ok(())
}

#[test]
fn unsuccessful_editor_aborts_without_returning_a_document() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let script = write_editor_script(repository.path(), "exit 23")?;
    let command = format!("sh {}", shell_quote(&script));
    git_ok(repository.path(), &["config", "core.editor", &command])?;

    let error = GitCommitDraftEditor
        .edit(&root(repository.path())?, "initial\n")
        .err()
        .ok_or("failed editor unexpectedly succeeded")?;
    assert!(matches!(error, GitCommitDraftEditorError::EditorFailed(_)));
    assert!(error.to_string().contains("configured Git editor exited"));
    assert!(error.source().is_none());
    Ok(())
}

#[cfg(unix)]
#[test]
fn non_utf8_editor_output_is_rejected() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let script = write_editor_script(repository.path(), "printf '\\377' > \"$1\"")?;
    let command = format!("sh {}", shell_quote(&script));
    git_ok(repository.path(), &["config", "core.editor", &command])?;

    assert!(matches!(
        GitCommitDraftEditor.edit(&root(repository.path())?, "initial\n"),
        Err(GitCommitDraftEditorError::InvalidDocumentEncoding(_))
    ));
    Ok(())
}

#[test]
fn writer_commits_exact_message_and_only_the_staged_index() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    fs::write(repository.path().join("staged.txt"), "staged\n")?;
    fs::write(repository.path().join("untracked.txt"), "untracked\n")?;
    git_ok(repository.path(), &["add", "staged.txt"])?;
    let message = feat_message()?;

    let output = GitCommitWriter.commit(&root(repository.path())?, &message)?;
    assert!(String::from_utf8_lossy(output.stdout()).contains("root-commit"));
    let object = git(repository.path(), &["cat-file", "commit", "HEAD"])?;
    assert!(object.status.success());
    let object = String::from_utf8(object.stdout)?;
    let actual_message = object
        .split_once("\n\n")
        .map(|(_, message)| message)
        .ok_or("commit object has no message separator")?;
    assert_eq!(actual_message, message.as_str());

    let tree = git(repository.path(), &["ls-tree", "--name-only", "HEAD"])?;
    assert_eq!(String::from_utf8(tree.stdout)?, "staged.txt\n");
    assert!(repository.path().join("untracked.txt").exists());
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
    git_ok(
        repository.path(),
        &["config", "core.hooksPath", hooks.to_string_lossy().as_ref()],
    )?;
    fs::write(repository.path().join("change.txt"), "change\n")?;
    git_ok(repository.path(), &["add", "change.txt"])?;

    GitCommitWriter.commit(&root(repository.path())?, &feat_message()?)?;
    assert_eq!(fs::read_to_string(marker)?, "ran");
    Ok(())
}

#[test]
fn hook_rejection_and_empty_index_preserve_git_diagnostics() -> Result<(), Box<dyn Error>> {
    let rejected_repository = repository()?;
    let hooks = rejected_repository.path().join("hooks");
    fs::create_dir(&hooks)?;
    let pre_commit = hooks.join("pre-commit");
    fs::write(
        &pre_commit,
        "#!/bin/sh\necho 'policy rejected commit' >&2\nexit 9\n",
    )?;
    make_executable(&pre_commit)?;
    git_ok(
        rejected_repository.path(),
        &["config", "core.hooksPath", hooks.to_string_lossy().as_ref()],
    )?;
    fs::write(rejected_repository.path().join("change.txt"), "change\n")?;
    git_ok(rejected_repository.path(), &["add", "change.txt"])?;
    let error = GitCommitWriter
        .commit(&root(rejected_repository.path())?, &feat_message()?)
        .err()
        .ok_or("rejecting hook unexpectedly committed")?;
    assert!(matches!(error, GitCommitError::Rejected { .. }));
    assert!(error.to_string().contains("policy rejected commit"));
    assert!(
        !git(
            rejected_repository.path(),
            &["rev-parse", "--verify", "HEAD"]
        )?
        .status
        .success()
    );

    let empty = repository()?;
    let error = GitCommitWriter
        .commit(&root(empty.path())?, &feat_message()?)
        .err()
        .ok_or("empty index unexpectedly committed")?;
    assert!(matches!(error, GitCommitError::Rejected { .. }));
    assert!(!error.to_string().is_empty());
    Ok(())
}

#[test]
fn repository_paths_with_spaces_are_supported() -> Result<(), Box<dyn Error>> {
    let parent = tempfile::tempdir()?;
    let repository = parent.path().join("repository with spaces");
    fs::create_dir(&repository)?;
    git_ok(&repository, &["init", "-q"])?;
    git_ok(&repository, &["config", "user.name", "Test User"])?;
    git_ok(&repository, &["config", "user.email", "test@example.com"])?;
    fs::write(repository.join("change.txt"), "change\n")?;
    git_ok(&repository, &["add", "change.txt"])?;

    GitCommitWriter.commit(&root(&repository)?, &feat_message()?)?;
    assert!(
        git(&repository, &["rev-parse", "--verify", "HEAD"])?
            .status
            .success()
    );
    Ok(())
}
