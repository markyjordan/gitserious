use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

use gitserious_app::{CitedCommit, RepositoryRoot, WorktreePaths};

const COMMIT_CITE_LIMIT: usize = 200;

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

    fn commits(&self, root: &RepositoryRoot) -> Result<Vec<CitedCommit>, String> {
        let limit = COMMIT_CITE_LIMIT.to_string();
        let output = Command::new("git")
            .arg("-C")
            .arg(root.as_path())
            .args(["log", "-n", limit.as_str(), "--pretty=format:%h%x1f%s"])
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .map_err(|error| format!("could not execute git log: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
        }
        Ok(parse_commits(&String::from_utf8_lossy(&output.stdout)))
    }
}

fn parse_commits(text: &str) -> Vec<CitedCommit> {
    text.lines()
        .filter_map(|line| {
            let (sha, subject) = line.split_once('\u{1f}')?;
            if sha.is_empty() {
                None
            } else {
                Some(CitedCommit {
                    sha: sha.to_owned(),
                    subject: subject.to_owned(),
                })
            }
        })
        .collect()
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
    use super::{catalog, parse_commits};

    #[test]
    fn commit_rows_keep_the_short_sha_and_subject() {
        let commits = parse_commits("abc1234\u{1f}back off\ndef5678\u{1f}\n");
        assert_eq!(commits[0].sha, "abc1234");
        assert_eq!(commits[0].subject, "back off");
        assert_eq!(commits[1].subject, "");
        assert_eq!(parse_commits("not a row\n").len(), 0);
    }

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
