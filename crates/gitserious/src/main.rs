use std::io;
use std::process::ExitCode;

use gitserious_cli::CommitAdapters;
use gitserious_fs::{
    GitCommitDraftEditor, GitCommitWriter, GitRepositoryLocator, TomlProjectStateStore,
};
use gitserious_tui::RatatuiCommitTypeSelector;

use crate::built_in_catalog::BuiltInCommitTypeCatalog;

mod built_in_catalog;

fn main() -> ExitCode {
    let locator = GitRepositoryLocator;
    let store = TomlProjectStateStore;
    let catalog = BuiltInCommitTypeCatalog;
    let selector = RatatuiCommitTypeSelector;
    let editor = GitCommitDraftEditor;
    let writer = GitCommitWriter;
    let commit = CommitAdapters::new(&catalog, &selector, &editor, &writer);
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();

    gitserious_cli::run_with_commit(
        std::env::args_os(),
        &locator,
        &store,
        &commit,
        &mut stdout,
        &mut stderr,
    )
}
