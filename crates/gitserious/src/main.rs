use std::fmt::Display;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use gitserious_app::{HelpPage, resolve_global_paths, run_interactive};
use gitserious_cli::{CommitAdapters, ShellLauncher, ShellStart};
use gitserious_fs::{
    GitCommitWriter, GitRepositoryInitializer, GitRepositoryLocator, GitRepositoryStatusReader,
    SystemGlobalPathResolver, TomlGlobalConfigurationStore, TomlProjectStore,
};
use gitserious_tui::{InitialPage, RatatuiInteractiveUi};

fn main() -> ExitCode {
    let mut stderr = io::stderr();
    let paths = match resolve_global_paths(&SystemGlobalPathResolver) {
        Ok(paths) => paths,
        Err(error) => return report(&mut stderr, &error),
    };
    let configuration = TomlGlobalConfigurationStore::new(paths.config().clone());
    let locator = GitRepositoryLocator;
    let store = TomlProjectStore;
    let writer = GitCommitWriter;
    let status = GitRepositoryStatusReader;
    let initializer = GitRepositoryInitializer;
    let shell = Application {
        locator: &locator,
        store: &store,
        configuration: &configuration,
        status: &status,
        initializer: &initializer,
        writer: &writer,
    };
    let commit = CommitAdapters::new().with_shell(&shell);
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

struct Application<'a> {
    locator: &'a GitRepositoryLocator,
    store: &'a TomlProjectStore,
    configuration: &'a TomlGlobalConfigurationStore,
    status: &'a GitRepositoryStatusReader,
    initializer: &'a GitRepositoryInitializer,
    writer: &'a GitCommitWriter,
}

impl ShellLauncher for Application<'_> {
    fn open(&self, start: ShellStart, directory: &Path, help: &[HelpPage]) -> Result<(), String> {
        let page = match start {
            ShellStart::Home => InitialPage::Home,
            ShellStart::Commit => InitialPage::Commit,
            ShellStart::Config => InitialPage::Config,
            ShellStart::Help { command } => InitialPage::Help { command },
        };
        run_interactive(
            self.locator,
            self.store,
            self.configuration,
            self.status,
            self.initializer,
            self.writer,
            directory,
            &RatatuiInteractiveUi::at(page),
            help,
        )
    }
}

fn report(stderr: &mut impl Write, error: impl Display) -> ExitCode {
    if writeln!(stderr, "error: {error}").is_err() {
        return ExitCode::FAILURE;
    }
    ExitCode::FAILURE
}
