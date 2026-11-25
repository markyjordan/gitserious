use std::path::PathBuf;
use std::process::Command;

use gitserious_app::{IndexWriter, RepositoryRoot};

/// Writes the index with `git add`, `git restore --staged`, and `git rm --cached`.
#[derive(Clone, Copy, Debug, Default)]
pub struct GitIndexWriter;

impl IndexWriter for GitIndexWriter {
    fn stage_tracked(&self, root: &RepositoryRoot) -> Result<(), String> {
        git(root, &["add", "--update"], &[])
    }

    fn stage(&self, root: &RepositoryRoot, paths: &[PathBuf]) -> Result<(), String> {
        // Without a pathspec `git add --all` would stage the whole worktree.
        if paths.is_empty() {
            return Ok(());
        }
        git(root, &["add", "--all"], paths)
    }

    fn unstage(&self, root: &RepositoryRoot, paths: &[PathBuf]) -> Result<(), String> {
        if paths.is_empty() {
            return Ok(());
        }
        if has_head(root)? {
            git(root, &["restore", "--staged"], paths)
        } else {
            git(
                root,
                &["rm", "--cached", "--quiet", "--ignore-unmatch"],
                paths,
            )
        }
    }
}

/// Whether `HEAD` names a commit, which an unborn branch lacks.
fn has_head(root: &RepositoryRoot) -> Result<bool, String> {
    Command::new("git")
        .arg("-C")
        .arg(root.as_path())
        .args(["rev-parse", "--quiet", "--verify", "HEAD^{commit}"])
        .output()
        .map(|output| output.status.success())
        .map_err(|error| format!("could not execute git rev-parse: {error}"))
}

/// Runs `arguments` on `paths`, read literally so a name holding `*` or `:` matches itself.
fn git(root: &RepositoryRoot, arguments: &[&str], paths: &[PathBuf]) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root.as_path())
        .arg("--literal-pathspecs")
        .args(arguments)
        .arg("--")
        .args(paths)
        .output()
        .map_err(|error| format!("could not execute git {}: {error}", arguments[0]))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}
