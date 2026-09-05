use std::fmt::Display;
use std::io::{self, Write};
use std::process::ExitCode;

use gitserious_app::resolve_global_paths;
use gitserious_cli::CommitAdapters;
use gitserious_fs::{
    GitCommitWriter, GitRepositoryLocator, SystemGlobalPathResolver, TomlGlobalConfigurationStore,
    TomlProjectStateStore,
};
use gitserious_tui::{RatatuiCommitDraftAuthor, RatatuiConfigurationEditor};

fn main() -> ExitCode {
    let mut stderr = io::stderr();
    let paths = match resolve_global_paths(&SystemGlobalPathResolver) {
        Ok(paths) => paths,
        Err(error) => return report(&mut stderr, &error),
    };
    let configuration = TomlGlobalConfigurationStore::new(paths.config().clone());
    let locator = GitRepositoryLocator;
    let store = TomlProjectStateStore;
    let author = RatatuiCommitDraftAuthor;
    let writer = GitCommitWriter;
    let editor = RatatuiConfigurationEditor;
    let commit = CommitAdapters::new(&author, &writer).with_configuration_editor(&editor);
    let mut stdout = io::stdout();

    gitserious_cli::run_with_commit(
        std::env::args_os(),
        &locator,
        &store,
        &configuration,
        &commit,
        &mut stdout,
        &mut stderr,
    )
}

fn report(stderr: &mut impl Write, error: impl Display) -> ExitCode {
    if writeln!(stderr, "error: {error}").is_err() {
        return ExitCode::FAILURE;
    }
    ExitCode::FAILURE
}
