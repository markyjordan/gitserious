//! Command-line delivery and presentation for gitserious.

use std::ffi::OsString;
use std::fmt::Display;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use gitserious_app::{
    InitOutcome, InitStatus, ProjectStateStore, RepositoryLocator, initialize_project,
};

#[derive(Debug, Parser)]
#[command(
    name = "gitserious",
    version,
    about = "Create durable commit-message policy for Git repositories",
    disable_colored_help = true,
    subcommand_required = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize repository-local gitserious policy.
    Init,
}

/// Runs the CLI using the process's current directory.
#[must_use]
pub fn run<I, T, L, S, Out, Err>(
    arguments: I,
    locator: &L,
    store: &S,
    stdout: &mut Out,
    stderr: &mut Err,
) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectStateStore + ?Sized,
    S::Error: Display,
    Out: Write + ?Sized,
    Err: Write + ?Sized,
{
    let cli = match parse(arguments, stdout, stderr) {
        Ok(cli) => cli,
        Err(exit) => return exit,
    };
    let start = match std::env::current_dir() {
        Ok(start) => start,
        Err(error) => return write_operational_error(stderr, error),
    };
    execute(&cli, &start, locator, store, stdout, stderr)
}

/// Runs the CLI from an explicit invocation directory.
#[must_use]
pub fn run_from<I, T, L, S, Out, Err>(
    arguments: I,
    start: &Path,
    locator: &L,
    store: &S,
    stdout: &mut Out,
    stderr: &mut Err,
) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectStateStore + ?Sized,
    S::Error: Display,
    Out: Write + ?Sized,
    Err: Write + ?Sized,
{
    let cli = match parse(arguments, stdout, stderr) {
        Ok(cli) => cli,
        Err(exit) => return exit,
    };
    execute(&cli, start, locator, store, stdout, stderr)
}

fn parse<I, T, Out, Err>(arguments: I, stdout: &mut Out, stderr: &mut Err) -> Result<Cli, ExitCode>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
    Out: Write + ?Sized,
    Err: Write + ?Sized,
{
    Cli::try_parse_from(arguments).map_err(|error| {
        let exit = exit_code(error.exit_code());
        let write_result = if error.use_stderr() {
            write!(stderr, "{error}")
        } else {
            write!(stdout, "{error}")
        };
        if write_result.is_err() {
            ExitCode::FAILURE
        } else {
            exit
        }
    })
}

fn execute<L, S, Out, Err>(
    cli: &Cli,
    start: &Path,
    locator: &L,
    store: &S,
    stdout: &mut Out,
    stderr: &mut Err,
) -> ExitCode
where
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectStateStore + ?Sized,
    S::Error: Display,
    Out: Write + ?Sized,
    Err: Write + ?Sized,
{
    match cli.command {
        Command::Init => match initialize_project(locator, store, start) {
            Ok(outcome) => write_outcome(stdout, &outcome),
            Err(error) => write_operational_error(stderr, error),
        },
    }
}

fn write_outcome(output: &mut (impl Write + ?Sized), outcome: &InitOutcome) -> ExitCode {
    let project_directory = outcome.root().as_path().join(".gitserious");
    let template_reference = outcome.template_reference();
    let resolved_template = outcome.resolved_template();
    let resolved_version = outcome.resolved_version();
    let result = match outcome.status() {
        InitStatus::Initialized => writeln!(
            output,
            "Initialized gitserious in {} ({template_reference} -> {resolved_template}@{resolved_version}).",
            project_directory.display()
        ),
        InitStatus::LockCreated => writeln!(
            output,
            "Created missing lock {} ({template_reference} -> {resolved_template}@{resolved_version}).",
            project_directory.join("gitserious.lock").display()
        ),
        InitStatus::LockRefreshed => writeln!(
            output,
            "Refreshed {} ({template_reference} -> {resolved_template}@{resolved_version}).",
            project_directory.join("gitserious.lock").display()
        ),
        InitStatus::AlreadyInitialized => writeln!(
            output,
            "gitserious is already initialized in {} ({template_reference} -> {resolved_template}@{resolved_version}).",
            project_directory.display()
        ),
    };

    if result.is_err() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn write_operational_error(output: &mut (impl Write + ?Sized), error: impl Display) -> ExitCode {
    if writeln!(output, "error: {error}").is_err() {
        return ExitCode::FAILURE;
    }
    ExitCode::FAILURE
}

fn exit_code(code: i32) -> ExitCode {
    u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from)
}
