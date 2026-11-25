use gitserious_core::CommitMessage;

use crate::RepositoryRoot;

/// The commit Git created, as Git abbreviates its name and stores its first line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatedCommit {
    pub abbreviated_oid: String,
    pub summary: String,
}

/// Output emitted by the concrete commit operation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommitOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    created: Option<CreatedCommit>,
}

impl CommitOutput {
    /// Records exact standard output and standard error bytes from Git.
    #[must_use]
    pub const fn new(stdout: Vec<u8>, stderr: Vec<u8>) -> Self {
        Self {
            stdout,
            stderr,
            created: None,
        }
    }

    /// Records the commit Git reports at `HEAD` after creating it.
    #[must_use]
    pub fn with_created(mut self, created: CreatedCommit) -> Self {
        self.created = Some(created);
        self
    }

    /// The created commit, absent when Git could not name it after a successful commit.
    #[must_use]
    pub const fn created(&self) -> Option<&CreatedCommit> {
        self.created.as_ref()
    }

    /// Returns exact standard output bytes.
    #[must_use]
    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }

    /// Returns exact standard error bytes.
    #[must_use]
    pub fn stderr(&self) -> &[u8] {
        &self.stderr
    }
}

/// Creates a repository commit from a validated canonical message.
pub trait CommitWriter {
    /// The adapter-specific Git operation failure.
    type Error;

    /// Commits the current index with `message` and captures Git's output.
    ///
    /// # Errors
    ///
    /// Returns the adapter's [`Self::Error`] when the commit cannot be created.
    fn commit(
        &self,
        root: &RepositoryRoot,
        message: &CommitMessage,
    ) -> Result<CommitOutput, Self::Error>;
}
