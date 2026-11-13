use std::io::{self, IsTerminal};
use std::process::ExitCode;

use gitserious_app::{
    CommitServices, ConfigurationServices, GlobalConfigurationStore, ProjectServices,
    initialize_project, resolve_global_paths,
};
use gitserious_cli::{
    Destination, Invocation, help_topics, parse, styled_help, write_answer, write_error,
    write_init_outcome,
};
use gitserious_fs::{
    GitCommitWriter, GitRepositoryInitializer, GitRepositoryLocator, GitRepositoryStatusReader,
    GitWorktreePaths, SystemGlobalPathResolver, TomlGlobalConfigurationStore, TomlProjectStore,
};
use gitserious_tui::{Capabilities, HelpPage, InitialPage, RatatuiInteractiveUi};

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
                Destination::Home => InitialPage::Home,
                Destination::Commit => InitialPage::Commit,
                Destination::Config => InitialPage::Config,
                Destination::Help { command } => InitialPage::Help { command },
            };
            let worktree = GitWorktreePaths;
            let commit = CommitServices::new(
                &GitRepositoryLocator,
                &TomlProjectStore,
                &GitRepositoryStatusReader,
                &GitCommitWriter,
                &directory,
            )
            .citing(&worktree);
            let config = ConfigurationServices::new(
                &GitRepositoryLocator,
                &TomlProjectStore,
                &configuration,
                &directory,
            );
            let project = ProjectServices::new(
                &GitRepositoryLocator,
                &TomlProjectStore,
                &GitRepositoryStatusReader,
                &GitRepositoryInitializer,
                &directory,
            );
            RatatuiInteractiveUi::at(page, help_pages())
                .run(Capabilities {
                    commit: &commit,
                    config: &config,
                    project: &project,
                })
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
