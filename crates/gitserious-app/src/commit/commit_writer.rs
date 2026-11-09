use gitserious_core::CommitMessage;

use crate::RepositoryRoot;

/// Output emitted by the concrete commit operation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommitOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl CommitOutput {
    /// Records exact standard output and standard error bytes from Git.
    #[must_use]
    pub const fn new(stdout: Vec<u8>, stderr: Vec<u8>) -> Self {
        Self { stdout, stderr }
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
