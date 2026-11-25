use std::error::Error;
#[cfg(unix)]
use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use gitserious_app::{
    MissingWorkTree, RepositoryInitializer, RepositoryLocator, RepositoryRoot, RepositoryRootError,
};

/// Git-process adapter for locating the enclosing worktree.
#[derive(Clone, Copy, Debug, Default)]
pub struct GitRepositoryLocator;

/// Runs `git init`, which honors the user's `init.defaultBranch`.
#[derive(Clone, Copy, Debug, Default)]
pub struct GitRepositoryInitializer;

impl RepositoryInitializer for GitRepositoryInitializer {
    fn initialize(&self, directory: &Path) -> Result<(), String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(directory)
            .args(["init", "--quiet"])
            .output()
            .map_err(|error| format!("could not execute git init: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
        }
    }
}

impl RepositoryLocator for GitRepositoryLocator {
    type Error = GitRepositoryError;

    fn locate(&self, start: &Path) -> Result<RepositoryRoot, Self::Error> {
        let output = Command::new("git")
            .args(["-c", "core.quotePath=false", "-C"])
            .arg(start)
            .args(["rev-parse", "--show-toplevel"])
            .env("GIT_OPTIONAL_LOCKS", "0")
            // Untranslated diagnostics let `is_not_repository` read them.
            .env("LC_ALL", "C")
            .output()
            .map_err(GitRepositoryError::GitUnavailable)?;

        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(GitRepositoryError::NotWorkTree {
                missing: missing_work_tree(start, &detail),
                start: start.to_path_buf(),
                detail,
            });
        }

        let path = path_from_stdout(output.stdout)?;
        RepositoryRoot::new(path).map_err(GitRepositoryError::InvalidRoot)
    }

    fn missing_work_tree(error: &Self::Error) -> Option<MissingWorkTree> {
        match error {
            GitRepositoryError::NotWorkTree { missing, .. } => missing.clone(),
            _ => None,
        }
    }
}

/// Reads why `start` has no work tree from Git's `detail`, asking Git what `start` is when the
/// message alone cannot tell. A `.git` file that names nothing stays unclassified.
fn missing_work_tree(start: &Path, detail: &str) -> Option<MissingWorkTree> {
    if detail.contains("not a git repository") {
        return Some(if is_home_or_root(start) {
            MissingWorkTree::HomeOrRoot
        } else {
            MissingWorkTree::NotRepository
        });
    }
    if detail.contains("dubious ownership") {
        return dubious_repository(detail)
            .map(|repository| MissingWorkTree::DubiousOwnership { repository });
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(start)
        .args([
            "rev-parse",
            "--is-bare-repository",
            "--is-inside-git-dir",
            "--absolute-git-dir",
        ])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();
    let (bare, inside, git_dir) = (lines.next()?, lines.next()?, lines.next()?);
    if bare == "true" {
        return Some(MissingWorkTree::Bare);
    }
    (inside == "true").then(|| {
        let git_dir = Path::new(git_dir);
        MissingWorkTree::InsideGitDir {
            worktree: (git_dir.file_name() == Some(".git".as_ref()))
                .then(|| git_dir.parent().map(Path::to_path_buf))
                .flatten(),
        }
    })
}

/// The repository Git names in "detected dubious ownership in repository at '<path>'".
fn dubious_repository(detail: &str) -> Option<PathBuf> {
    let (_, rest) = detail.split_once("repository at '")?;
    let (path, _) = rest.split_once('\'')?;
    Some(PathBuf::from(path))
}

/// Whether `directory` is the user's home directory or a filesystem root, where `git init`
/// would put everything below under version control.
fn is_home_or_root(directory: &Path) -> bool {
    let Ok(directory) = directory.canonicalize() else {
        return false;
    };
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .and_then(|home| PathBuf::from(home).canonicalize().ok());
    directory.parent().is_none() || home.is_some_and(|home| home == directory)
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
        /// Why no work tree contains the directory, when Git's answer tells.
        missing: Option<MissingWorkTree>,
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
            Self::NotWorkTree {
                start,
                missing: Some(MissingWorkTree::HomeOrRoot),
                ..
            } => write!(
                formatter,
                "{} is a home directory or a filesystem root; open a project folder to \
                 initialize git and gitserious",
                start.display()
            ),
            Self::NotWorkTree { start, detail, .. } if detail.is_empty() => {
                write!(
                    formatter,
                    "directory is not inside a Git worktree: {}",
                    start.display()
                )
            }
            Self::NotWorkTree { start, detail, .. } => write!(
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
