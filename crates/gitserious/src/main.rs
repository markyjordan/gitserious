use std::io::{self, IsTerminal};
use std::process::ExitCode;

use gitserious_app::{
    GlobalConfigurationStore, initialize_project, resolve_global_paths, run_interactive,
};
use gitserious_cli::{
    Invocation, ShellStart, help_topics, parse, styled_help, write_answer, write_error,
    write_init_outcome,
};
use gitserious_fs::{
    GitCommitWriter, GitRepositoryInitializer, GitRepositoryLocator, GitRepositoryStatusReader,
    SystemGlobalPathResolver, TomlGlobalConfigurationStore, TomlProjectStore,
};
use gitserious_tui::{HelpPage, InitialPage, RatatuiInteractiveUi};

fn main() -> ExitCode {
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    let paths = match resolve_global_paths(&SystemGlobalPathResolver) {
        Ok(paths) => paths,
        Err(error) => return write_error(&mut stderr, error),
    };
    let directory = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => return write_error(&mut stderr, error),
    };
    let configuration = TomlGlobalConfigurationStore::new(paths.config().clone());
    let terminal = io::stdin().is_terminal() && io::stdout().is_terminal();

    match parse(std::env::args_os(), terminal) {
        Invocation::Open(start) => {
            let page = match start {
                ShellStart::Home => InitialPage::Home,
                ShellStart::Commit => InitialPage::Commit,
                ShellStart::Config => InitialPage::Config,
                ShellStart::Help { command } => InitialPage::Help { command },
            };
            run_interactive(
                &GitRepositoryLocator,
                &TomlProjectStore,
                &configuration,
                &GitRepositoryStatusReader,
                &GitRepositoryInitializer,
                &GitCommitWriter,
                &directory,
                &RatatuiInteractiveUi::at(page, help_pages()),
            )
            .map_or_else(
                |error| write_error(&mut stderr, error),
                |()| ExitCode::SUCCESS,
            )
        }
        Invocation::Init => {
            let global = match configuration.load() {
                Ok(global) => global,
                Err(error) => return write_error(&mut stderr, error),
            };
            match initialize_project(
                &GitRepositoryLocator,
                &TomlProjectStore,
                &global,
                &directory,
            ) {
                Ok(outcome) => write_init_outcome(&mut stdout, &outcome),
                Err(error) => write_error(&mut stderr, error),
            }
        }
        Invocation::Answer(answer) => write_answer(answer, &mut stdout, &mut stderr),
    }
}

/// The CLI's Help topics as TUI pages.
fn help_pages() -> Vec<HelpPage> {
    help_topics()
        .into_iter()
        .map(|topic| {
            HelpPage::styled(topic.title, topic.command, move |width| {
                styled_help(topic.command, width)
            })
        })
        .collect()
}
