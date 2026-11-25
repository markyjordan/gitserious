// Git runs the editor through sh, which only Unix runners are sure to have on PATH.
#![cfg(unix)]

use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;

use gitserious_app::{FileEditor, RepositoryRoot};
use gitserious_fs::GitEditor;

fn git(directory: &Path, arguments: &[&str]) -> Result<(), Box<dyn Error>> {
    let status = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .status()?;
    if !status.success() {
        return Err(format!("git {arguments:?} failed with {status}").into());
    }
    Ok(())
}

#[test]
fn the_editor_git_names_edits_the_file_and_its_failure_is_reported() -> Result<(), Box<dyn Error>> {
    // An editor in the environment outranks core.editor, so the test cannot choose one.
    if ["GIT_EDITOR", "VISUAL", "EDITOR"]
        .iter()
        .any(|name| std::env::var_os(name).is_some())
    {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    git(directory.path(), &["init", "-q"])?;
    let root = RepositoryRoot::new(directory.path().to_path_buf())?;
    fs::write(directory.path().join("gitserious.toml"), "broken\n")?;

    git(
        directory.path(),
        &["config", "core.editor", "printf 'edited\\n' >"],
    )?;
    GitEditor.edit(&root, Path::new("gitserious.toml"))?;
    assert_eq!(
        fs::read_to_string(directory.path().join("gitserious.toml"))?,
        "edited\n"
    );

    git(directory.path(), &["config", "core.editor", "false"])?;
    let failure = GitEditor
        .edit(&root, Path::new("gitserious.toml"))
        .err()
        .ok_or("a failing editor succeeded")?;
    assert!(failure.contains("the editor false exited"), "{failure}");
    Ok(())
}
