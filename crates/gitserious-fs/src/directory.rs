use std::fs::DirBuilder;
use std::io;

use gitserious_app::{DirectoryCreator, StorageDirectory};

/// Local-filesystem adapter for creating selected storage directories.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalDirectoryCreator;

impl DirectoryCreator for LocalDirectoryCreator {
    type Error = io::Error;

    fn ensure(&self, directory: &StorageDirectory) -> Result<(), Self::Error> {
        let mut builder = DirBuilder::new();
        builder.recursive(true);

        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;

            builder.mode(0o700);
        }

        builder.create(directory.as_path())
    }
}
