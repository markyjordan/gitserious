use std::error::Error;
#[cfg(unix)]
use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use gitserious_app::{RepositoryLocator, RepositoryRoot, RepositoryRootError};

/// Git-process adapter for locating the enclosing worktree.
#[derive(Clone, Copy, Debug, Default)]
pub struct GitRepositoryLocator;

impl RepositoryLocator for GitRepositoryLocator {
    type Error = GitRepositoryError;

    fn locate(&self, start: &Path) -> Result<RepositoryRoot, Self::Error> {
        let output = Command::new("git")
            .args(["-c", "core.quotePath=false", "-C"])
            .arg(start)
            .args(["rev-parse", "--show-toplevel"])
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .map_err(GitRepositoryError::GitUnavailable)?;

        if !output.status.success() {
            return Err(GitRepositoryError::NotWorkTree {
                start: start.to_path_buf(),
                detail: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }

        let path = path_from_stdout(output.stdout)?;
        RepositoryRoot::new(path).map_err(GitRepositoryError::InvalidRoot)
    }
}

fn path_from_stdout(mut bytes: Vec<u8>) -> Result<PathBuf, GitRepositoryError> {
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    if bytes.is_empty() {
        return Err(GitRepositoryError::EmptyOutput);
    }

    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStringExt;

        PathBuf::from(OsString::from_vec(bytes))
    };

    #[cfg(not(unix))]
    let path = PathBuf::from(
        String::from_utf8(bytes).map_err(|error| GitRepositoryError::InvalidEncoding(error))?,
    );

    Ok(path)
}

/// Failure to discover a usable Git worktree root.
#[derive(Debug)]
pub enum GitRepositoryError {
    /// The Git executable could not be started.
    GitUnavailable(io::Error),
    /// Git rejected the starting directory as a worktree.
    NotWorkTree {
        /// The invocation directory supplied to Git.
        start: PathBuf,
        /// Git's trimmed diagnostic output.
        detail: String,
    },
    /// Git succeeded without returning a worktree path.
    EmptyOutput,
    /// A non-Unix Git process returned a path that was not UTF-8.
    #[cfg(not(unix))]
    InvalidEncoding(std::string::FromUtf8Error),
    /// Git returned a relative path instead of an absolute worktree root.
    InvalidRoot(RepositoryRootError),
}

impl Display for GitRepositoryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitUnavailable(_) => formatter.write_str("could not execute git"),
            Self::NotWorkTree { start, detail } if detail.is_empty() => {
                write!(
                    formatter,
                    "directory is not inside a Git worktree: {}",
                    start.display()
                )
            }
            Self::NotWorkTree { start, detail } => write!(
                formatter,
                "directory is not inside a Git worktree: {} ({detail})",
                start.display()
            ),
            Self::EmptyOutput => formatter.write_str("git returned an empty worktree root"),
            #[cfg(not(unix))]
            Self::InvalidEncoding(_) => {
                formatter.write_str("git returned a worktree root that is not valid UTF-8")
            }
            Self::InvalidRoot(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for GitRepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::GitUnavailable(error) => Some(error),
            Self::NotWorkTree { .. } | Self::EmptyOutput => None,
            #[cfg(not(unix))]
            Self::InvalidEncoding(error) => Some(error),
            Self::InvalidRoot(error) => Some(error),
        }
    }
}
