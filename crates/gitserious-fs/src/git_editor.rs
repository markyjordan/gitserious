use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::io::{self, Write};
use std::process::{Command, ExitStatus};
use std::string::FromUtf8Error;

use gitserious_app::{CommitDraftEditor, RepositoryRoot};

const EDITOR_ALIAS: &str = "alias.gitserious-edit=!eval \"$GITSERIOUS_EDITOR_COMMAND\"";

/// Git-process implementation of the configured commit-draft editor.
#[derive(Clone, Copy, Debug, Default)]
pub struct GitCommitDraftEditor;

impl CommitDraftEditor for GitCommitDraftEditor {
    type Error = GitCommitDraftEditorError;

    fn edit(&self, root: &RepositoryRoot, document: &str) -> Result<String, Self::Error> {
        let mut temporary =
            tempfile::NamedTempFile::new().map_err(GitCommitDraftEditorError::CreateDocument)?;
        temporary
            .write_all(document.as_bytes())
            .map_err(GitCommitDraftEditorError::WriteDocument)?;
        temporary
            .flush()
            .map_err(GitCommitDraftEditorError::FlushDocument)?;

        let editor = resolve_editor(root)?;
        let path = temporary
            .path()
            .to_str()
            .ok_or(GitCommitDraftEditorError::NonUtf8DocumentPath)?;
        let command = format!("{editor} {}", shell_quote(path));
        let status = Command::new("git")
            .arg("-C")
            .arg(root.as_path())
            .args(["-c", EDITOR_ALIAS, "gitserious-edit"])
            .env("GITSERIOUS_EDITOR_COMMAND", command)
            .status()
            .map_err(GitCommitDraftEditorError::GitUnavailable)?;
        if !status.success() {
            return Err(GitCommitDraftEditorError::EditorFailed(status));
        }

        let bytes = fs::read(temporary.path()).map_err(GitCommitDraftEditorError::ReadDocument)?;
        String::from_utf8(bytes).map_err(GitCommitDraftEditorError::InvalidDocumentEncoding)
    }
}

fn resolve_editor(root: &RepositoryRoot) -> Result<String, GitCommitDraftEditorError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root.as_path())
        .args(["var", "GIT_EDITOR"])
        .output()
        .map_err(GitCommitDraftEditorError::GitUnavailable)?;
    if !output.status.success() {
        return Err(GitCommitDraftEditorError::EditorUnavailable(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    let bytes = trim_line_ending(output.stdout);
    if bytes.is_empty() {
        return Err(GitCommitDraftEditorError::EditorUnavailable(String::new()));
    }
    String::from_utf8(bytes).map_err(GitCommitDraftEditorError::InvalidEditorEncoding)
}

fn trim_line_ending(mut bytes: Vec<u8>) -> Vec<u8> {
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    bytes
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Failure to prepare, open, or read a Git commit editor document.
#[derive(Debug)]
pub enum GitCommitDraftEditorError {
    /// A private temporary editor document could not be created.
    CreateDocument(io::Error),
    /// The initial editor document could not be written.
    WriteDocument(io::Error),
    /// The initial editor document could not be flushed.
    FlushDocument(io::Error),
    /// The Git executable could not be started.
    GitUnavailable(io::Error),
    /// Git could not resolve an editor command.
    EditorUnavailable(String),
    /// Git returned an editor command that was not valid UTF-8.
    InvalidEditorEncoding(FromUtf8Error),
    /// The temporary document path could not be passed through Git's shell.
    NonUtf8DocumentPath,
    /// The configured editor exited unsuccessfully.
    EditorFailed(ExitStatus),
    /// The saved editor document could not be read.
    ReadDocument(io::Error),
    /// The saved editor document was not valid UTF-8.
    InvalidDocumentEncoding(FromUtf8Error),
}

impl Display for GitCommitDraftEditorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateDocument(_) => {
                formatter.write_str("could not create the commit editor document")
            }
            Self::WriteDocument(_) | Self::FlushDocument(_) => {
                formatter.write_str("could not prepare the commit editor document")
            }
            Self::GitUnavailable(_) => formatter.write_str("could not execute git"),
            Self::EditorUnavailable(detail) if detail.is_empty() => {
                formatter.write_str("Git has no configured editor")
            }
            Self::EditorUnavailable(detail) => {
                write!(
                    formatter,
                    "Git could not resolve its configured editor ({detail})"
                )
            }
            Self::InvalidEditorEncoding(_) => {
                formatter.write_str("Git returned an editor command that is not valid UTF-8")
            }
            Self::NonUtf8DocumentPath => formatter
                .write_str("temporary commit editor path cannot be represented for Git's shell"),
            Self::EditorFailed(status) => {
                write!(formatter, "configured Git editor exited with {status}")
            }
            Self::ReadDocument(_) => {
                formatter.write_str("could not read the saved commit editor document")
            }
            Self::InvalidDocumentEncoding(_) => {
                formatter.write_str("saved commit editor document is not valid UTF-8")
            }
        }
    }
}

impl Error for GitCommitDraftEditorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::CreateDocument(error)
            | Self::WriteDocument(error)
            | Self::FlushDocument(error)
            | Self::GitUnavailable(error)
            | Self::ReadDocument(error) => Some(error),
            Self::InvalidEditorEncoding(error) | Self::InvalidDocumentEncoding(error) => {
                Some(error)
            }
            Self::EditorUnavailable(_) | Self::NonUtf8DocumentPath | Self::EditorFailed(_) => None,
        }
    }
}
