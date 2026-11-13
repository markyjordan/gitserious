use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

use gitserious_app::{RepositoryRoot, WorktreePaths};

/// Lists cite paths with `git ls-files`.
#[derive(Clone, Copy, Debug, Default)]
pub struct GitWorktreePaths;

impl WorktreePaths for GitWorktreePaths {
    fn paths(&self, root: &RepositoryRoot) -> Result<Vec<String>, String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(root.as_path())
            .args([
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
            ])
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .map_err(|error| format!("could not execute git ls-files: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
        }
        Ok(catalog(files(&output.stdout)?))
    }
}

fn files(bytes: &[u8]) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    for record in bytes.split(|byte| *byte == 0) {
        if record.is_empty() {
            continue;
        }
        let text = path(record)?;
        let Some(text) = text.to_str() else {
            continue;
        };
        if !text.is_empty() {
            files.push(text.to_owned());
        }
    }
    Ok(files)
}

/// Directories keep a trailing slash, so `@src/net/` reads as a directory.
fn catalog(files: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut paths = BTreeSet::new();
    for file in files {
        let mut directory = String::new();
        let mut parts = file.split('/');
        let Some(mut current) = parts.next() else {
            continue;
        };
        for next in parts {
            if current.is_empty() {
                current = next;
                continue;
            }
            directory.push_str(current);
            directory.push('/');
            paths.insert(directory.clone());
            current = next;
        }
        if !file.is_empty() {
            paths.insert(file);
        }
    }
    paths.into_iter().collect()
}

// Unix paths are infallible; the same interface reports decoding errors on Windows.
#[cfg_attr(unix, allow(clippy::unnecessary_wraps))]
fn path(bytes: &[u8]) -> Result<PathBuf, String> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(PathBuf::from(std::ffi::OsString::from_vec(bytes.to_vec())))
    }
    #[cfg(not(unix))]
    {
        String::from_utf8(bytes.to_vec())
            .map(PathBuf::from)
            .map_err(|error| format!("invalid path encoding: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::catalog;

    #[test]
    fn directories_are_added_for_every_parent() {
        let paths = catalog([
            "src/net/retry.rs".into(),
            "src/net/retry.rs".into(),
            "README.md".into(),
        ]);
        assert_eq!(
            paths,
            ["README.md", "src/", "src/net/", "src/net/retry.rs",]
        );
    }
}
