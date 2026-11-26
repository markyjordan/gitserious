// Git runs the editor through sh, which only Unix runners are sure to have on PATH.
#![cfg(unix)]

mod support;

use std::error::Error;
use std::fs;
use std::path::Path;

use gitserious_app::{FileEditor, RepositoryRoot};
use gitserious_fs::GitEditor;
use support::{git, repository};

#[test]
fn the_editor_git_names_edits_the_file_and_its_failure_is_reported() -> Result<(), Box<dyn Error>> {
    // An editor in the environment outranks core.editor, so the test cannot choose one.
    if ["GIT_EDITOR", "VISUAL", "EDITOR"]
        .iter()
        .any(|name| std::env::var_os(name).is_some())
    {
        return Ok(());
    }
    let directory = repository()?;
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
