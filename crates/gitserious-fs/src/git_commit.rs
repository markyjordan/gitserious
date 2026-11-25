use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::{self, Write};
use std::process::{Command, ExitStatus, Stdio};

use gitserious_app::{CommitOutput, CommitWriter, CreatedCommit, RepositoryRoot};
use gitserious_core::CommitMessage;

/// Git-process implementation that commits the current staged index.
#[derive(Clone, Copy, Debug, Default)]
pub struct GitCommitWriter;

impl CommitWriter for GitCommitWriter {
    type Error = GitCommitError;

    fn commit(
        &self,
        root: &RepositoryRoot,
        message: &CommitMessage,
    ) -> Result<CommitOutput, Self::Error> {
        let mut temporary =
            tempfile::NamedTempFile::new().map_err(GitCommitError::CreateMessage)?;
        temporary
            .write_all(message.as_str().as_bytes())
            .map_err(GitCommitError::WriteMessage)?;
        temporary.flush().map_err(GitCommitError::FlushMessage)?;

        let output = Command::new("git")
            .arg("-C")
            .arg(root.as_path())
            .arg("commit")
            .arg("--file")
            .arg(temporary.path())
            .arg("--cleanup=verbatim")
            .stdin(Stdio::inherit())
            .output()
            .map_err(GitCommitError::GitUnavailable)?;

        if output.status.success() {
            let written = CommitOutput::new(output.stdout, output.stderr);
            // The commit exists by now, so a failed read of HEAD must not report a failure.
            Ok(match head(root) {
                Some(created) => written.with_created(created),
                None => written,
            })
        } else {
            Err(GitCommitError::Rejected {
                status: output.status,
                stdout: output.stdout,
                stderr: output.stderr,
            })
        }
    }
}

/// `HEAD`'s abbreviated name and first line, as Git stores them.
fn head(root: &RepositoryRoot) -> Option<CreatedCommit> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root.as_path())
        .args([
            "-c",
            "log.showSignature=false",
            "log",
            "-1",
            "--format=%h%x00%s",
        ])
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let text = String::from_utf8_lossy(&output.stdout);
    let (abbreviated_oid, summary) = text.trim_end_matches('\n').split_once('\0')?;
    Some(CreatedCommit {
        abbreviated_oid: abbreviated_oid.to_owned(),
        summary: summary.to_owned(),
    })
}

/// Failure to prepare a message file or create a Git commit.
#[derive(Debug)]
pub enum GitCommitError {
    /// A private temporary canonical message file could not be created.
    CreateMessage(io::Error),
    /// The canonical message could not be written.
    WriteMessage(io::Error),
    /// The canonical message file could not be flushed.
    FlushMessage(io::Error),
    /// The Git executable could not be started.
    GitUnavailable(io::Error),
    /// Git ran but refused or failed to create the commit.
    Rejected {
        /// Git process status.
        status: ExitStatus,
        /// Exact standard output bytes.
        stdout: Vec<u8>,
        /// Exact standard error bytes.
        stderr: Vec<u8>,
    },
}

impl Display for GitCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateMessage(_) => {
                formatter.write_str("could not create the canonical commit message file")
            }
            Self::WriteMessage(_) | Self::FlushMessage(_) => {
                formatter.write_str("could not prepare the canonical commit message file")
            }
            Self::GitUnavailable(_) => formatter.write_str("could not execute git"),
            Self::Rejected {
                status,
                stdout,
                stderr,
            } => {
                let detail = if stderr.is_empty() { stdout } else { stderr };
                let detail = String::from_utf8_lossy(detail);
                let detail = detail.trim();
                if detail.is_empty() {
                    write!(formatter, "git commit exited with {status}")
                } else {
                    formatter.write_str(detail)
                }
            }
        }
    }
}

impl Error for GitCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::CreateMessage(error)
            | Self::WriteMessage(error)
            | Self::FlushMessage(error)
            | Self::GitUnavailable(error) => Some(error),
            Self::Rejected { .. } => None,
        }
    }
}
