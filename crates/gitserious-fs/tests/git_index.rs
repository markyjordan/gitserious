mod support;

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use gitserious_app::{IndexWriter, RepositoryRoot};
use gitserious_fs::GitIndexWriter;
use support::git;
use tempfile::TempDir;

fn repository() -> Result<(TempDir, RepositoryRoot), Box<dyn Error>> {
    let temp = support::repository()?;
    let root = RepositoryRoot::new(temp.path().to_path_buf())?;
    Ok((temp, root))
}

/// Short status, staged column first, one line per path, without untracked files.
fn staged(directory: &Path) -> Result<String, Box<dyn Error>> {
    let output = git(directory, &["status", "--short", "--untracked-files=no"])?;
    Ok(String::from_utf8(output.stdout)?)
}

fn committed(directory: &Path, files: &[(&str, &str)]) -> Result<(), Box<dyn Error>> {
    for (path, text) in files {
        fs::write(directory.join(path), text)?;
    }
    git(directory, &["add", "."])?;
    git(directory, &["commit", "-q", "-m", "base"])?;
    Ok(())
}

#[test]
fn staging_tracked_changes_takes_edits_and_deletions_but_not_untracked_files()
-> Result<(), Box<dyn Error>> {
    let (temp, root) = repository()?;
    committed(temp.path(), &[("edited", "a\n"), ("deleted", "b\n")])?;
    fs::write(temp.path().join("edited"), "changed\n")?;
    fs::remove_file(temp.path().join("deleted"))?;
    fs::write(temp.path().join("untracked"), "new\n")?;

    GitIndexWriter.stage_tracked(&root)?;

    assert_eq!(staged(temp.path())?, "D  deleted\nM  edited\n");
    let untracked = git(temp.path(), &["ls-files", "--others", "--exclude-standard"])?;
    assert_eq!(String::from_utf8(untracked.stdout)?, "untracked\n");
    Ok(())
}

#[test]
fn an_untracked_file_and_a_deletion_stage_and_unstage_one_path_at_a_time()
-> Result<(), Box<dyn Error>> {
    let (temp, root) = repository()?;
    committed(temp.path(), &[("deleted", "b\n"), ("kept", "c\n")])?;
    fs::remove_file(temp.path().join("deleted"))?;
    // As a glob, `new[x]` would also match `newx`.
    fs::write(temp.path().join("new[x]"), "new\n")?;
    fs::write(temp.path().join("newx"), "other\n")?;

    GitIndexWriter.stage(&root, &["new[x]".into()])?;
    GitIndexWriter.stage(&root, &["deleted".into()])?;
    assert_eq!(staged(temp.path())?, "D  deleted\nA  new[x]\n");

    GitIndexWriter.unstage(&root, &["new[x]".into()])?;
    GitIndexWriter.unstage(&root, &["deleted".into()])?;
    assert_eq!(staged(temp.path())?, " D deleted\n");
    assert!(
        temp.path().join("new[x]").exists(),
        "unstaging keeps the file"
    );

    GitIndexWriter.stage(&root, &[])?;
    assert_eq!(
        staged(temp.path())?,
        " D deleted\n",
        "no paths stages nothing"
    );
    Ok(())
}

#[test]
fn a_rename_unstages_by_naming_both_paths() -> Result<(), Box<dyn Error>> {
    let (temp, root) = repository()?;
    committed(temp.path(), &[("old", "same content\n")])?;
    git(temp.path(), &["mv", "old", "new"])?;
    assert_eq!(staged(temp.path())?, "R  old -> new\n");

    GitIndexWriter.unstage(&root, &[PathBuf::from("new"), PathBuf::from("old")])?;

    assert_eq!(staged(temp.path())?, " D old\n");
    assert!(temp.path().join("new").exists());
    Ok(())
}

#[test]
fn an_unborn_branch_unstages_by_dropping_the_path_from_the_index() -> Result<(), Box<dyn Error>> {
    let (temp, root) = repository()?;
    fs::write(temp.path().join("first"), "first\n")?;
    fs::write(temp.path().join("second"), "second\n")?;

    GitIndexWriter.stage(&root, &["first".into(), "second".into()])?;
    assert_eq!(staged(temp.path())?, "A  first\nA  second\n");
    GitIndexWriter.unstage(&root, &["first".into()])?;

    assert_eq!(staged(temp.path())?, "A  second\n");
    assert!(
        temp.path().join("first").exists(),
        "unstaging keeps the file"
    );
    Ok(())
}
