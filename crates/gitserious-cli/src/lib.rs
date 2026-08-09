//! Command-line delivery and presentation for gitserious.

use std::error::Error;
use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use gitserious_app::{
    CommitDraftAuthor, CommitDraftAuthorOutcome, CommitOutcome, CommitOutput, CommitTypeCatalog,
    CommitWriter, InitOutcome, InitStatus, ProjectStateStore, RepositoryLocator, RepositoryRoot,
    create_commit, initialize_project,
};
use gitserious_core::{CommitMessage, CommitTypeDefinition, CommitTypeId};

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
    /// Author and create a durable commit from the staged index.
    Commit {
        /// Select a commit type without opening the terminal picker.
        #[arg(long = "type", value_name = "COMMIT TYPE")]
        commit_type: Option<CommitTypeId>,
    },
    /// Initialize repository-local gitserious policy.
    Init,
}

/// Concrete adapters required only by the interactive commit workflow.
#[derive(Clone, Copy)]
pub struct CommitAdapters<'a, C: ?Sized, A: ?Sized, W: ?Sized> {
    catalog: &'a C,
    author: &'a A,
    writer: &'a W,
}

impl<'a, C: ?Sized, A: ?Sized, W: ?Sized> CommitAdapters<'a, C, A, W> {
    /// Bundles the independent commit-workflow adapters for command dispatch.
    #[must_use]
    pub const fn new(catalog: &'a C, author: &'a A, writer: &'a W) -> Self {
        Self {
            catalog,
            author,
            writer,
        }
    }
}

/// Runs the CLI using the process's current directory.
///
/// This compatibility entry point supports initialization. The installable
/// binary uses [`run_with_commit`] to supply concrete commit adapters.
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
    let unavailable = UnsupportedCommitAdapter;
    let commit = CommitAdapters::new(&unavailable, &unavailable, &unavailable);
    run_with_commit(arguments, locator, store, &commit, stdout, stderr)
}

/// Runs the CLI with concrete interactive commit adapters.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn run_with_commit<I, T, L, S, C, A, W, Out, Err>(
    arguments: I,
    locator: &L,
    store: &S,
    commit: &CommitAdapters<'_, C, A, W>,
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
    C: CommitTypeCatalog + ?Sized,
    C::Error: Display,
    A: CommitDraftAuthor + ?Sized,
    A::Error: Display,
    W: CommitWriter + ?Sized,
    W::Error: Display,
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
    execute(&cli, &start, locator, store, commit, stdout, stderr)
}

/// Runs the CLI from an explicit invocation directory.
///
/// This compatibility entry point supports initialization. Use
/// [`run_from_with_commit`] to exercise commit behavior with explicit adapters.
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
    let unavailable = UnsupportedCommitAdapter;
    let commit = CommitAdapters::new(&unavailable, &unavailable, &unavailable);
    run_from_with_commit(arguments, start, locator, store, &commit, stdout, stderr)
}

/// Runs the CLI from an explicit directory with concrete commit adapters.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn run_from_with_commit<I, T, L, S, C, A, W, Out, Err>(
    arguments: I,
    start: &Path,
    locator: &L,
    store: &S,
    commit: &CommitAdapters<'_, C, A, W>,
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
    C: CommitTypeCatalog + ?Sized,
    C::Error: Display,
    A: CommitDraftAuthor + ?Sized,
    A::Error: Display,
    W: CommitWriter + ?Sized,
    W::Error: Display,
    Out: Write + ?Sized,
    Err: Write + ?Sized,
{
    let cli = match parse(arguments, stdout, stderr) {
        Ok(cli) => cli,
        Err(exit) => return exit,
    };
    execute(&cli, start, locator, store, commit, stdout, stderr)
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

#[allow(clippy::too_many_arguments)]
fn execute<L, S, C, A, W, Out, Err>(
    cli: &Cli,
    start: &Path,
    locator: &L,
    store: &S,
    commit: &CommitAdapters<'_, C, A, W>,
    stdout: &mut Out,
    stderr: &mut Err,
) -> ExitCode
where
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectStateStore + ?Sized,
    S::Error: Display,
    C: CommitTypeCatalog + ?Sized,
    C::Error: Display,
    A: CommitDraftAuthor + ?Sized,
    A::Error: Display,
    W: CommitWriter + ?Sized,
    W::Error: Display,
    Out: Write + ?Sized,
    Err: Write + ?Sized,
{
    match &cli.command {
        Command::Commit { commit_type } => match create_commit(
            locator,
            store,
            commit.catalog,
            commit.author,
            commit.writer,
            start,
            commit_type.as_ref(),
        ) {
            Ok(outcome) => write_commit_outcome(stdout, stderr, &outcome),
            Err(error) => write_operational_error(stderr, error),
        },
        Command::Init => match initialize_project(locator, store, start) {
            Ok(outcome) => write_init_outcome(stdout, &outcome),
            Err(error) => write_operational_error(stderr, error),
        },
    }
}

fn write_commit_outcome(
    stdout: &mut (impl Write + ?Sized),
    stderr: &mut (impl Write + ?Sized),
    outcome: &CommitOutcome,
) -> ExitCode {
    match outcome {
        CommitOutcome::Created(output) => write_commit_output(stdout, stderr, output),
        CommitOutcome::Cancelled => {
            if writeln!(stderr, "Commit cancelled.").is_err() {
                return ExitCode::FAILURE;
            }
            ExitCode::FAILURE
        }
    }
}

fn write_commit_output(
    stdout: &mut (impl Write + ?Sized),
    stderr: &mut (impl Write + ?Sized),
    output: &CommitOutput,
) -> ExitCode {
    if stdout.write_all(output.stdout()).is_err() || stderr.write_all(output.stderr()).is_err() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn write_init_outcome(output: &mut (impl Write + ?Sized), outcome: &InitOutcome) -> ExitCode {
    let project_root = outcome.root().as_path();
    let template_reference = outcome.template_reference();
    let resolved_template = outcome.resolved_template();
    let resolved_version = outcome.resolved_version();
    let result = match outcome.status() {
        InitStatus::Initialized => writeln!(
            output,
            "Initialized gitserious in {} ({template_reference} -> {resolved_template}@{resolved_version}).",
            project_root.display()
        ),
        InitStatus::LockCreated => writeln!(
            output,
            "Created missing lock {} ({template_reference} -> {resolved_template}@{resolved_version}).",
            project_root.join("gitserious.lock").display()
        ),
        InitStatus::LockRefreshed => writeln!(
            output,
            "Refreshed {} ({template_reference} -> {resolved_template}@{resolved_version}).",
            project_root.join("gitserious.lock").display()
        ),
        InitStatus::AlreadyInitialized => writeln!(
            output,
            "gitserious is already initialized in {} ({template_reference} -> {resolved_template}@{resolved_version}).",
            project_root.display()
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

#[derive(Clone, Copy, Debug)]
struct UnsupportedCommitAdapter;

impl CommitTypeCatalog for UnsupportedCommitAdapter {
    type Error = UnsupportedCommitError;

    fn find(&self, _id: &CommitTypeId) -> Result<Option<CommitTypeDefinition>, Self::Error> {
        Ok(None)
    }

    fn list(&self) -> Result<Vec<CommitTypeDefinition>, Self::Error> {
        Ok(Vec::new())
    }
}

impl CommitDraftAuthor for UnsupportedCommitAdapter {
    type Error = UnsupportedCommitError;

    fn author(
        &self,
        _definitions: &[CommitTypeDefinition],
        _preselected: Option<&CommitTypeDefinition>,
    ) -> Result<CommitDraftAuthorOutcome, Self::Error> {
        Err(UnsupportedCommitError)
    }
}

impl CommitWriter for UnsupportedCommitAdapter {
    type Error = UnsupportedCommitError;

    fn commit(
        &self,
        _root: &RepositoryRoot,
        _message: &CommitMessage,
    ) -> Result<CommitOutput, Self::Error> {
        Err(UnsupportedCommitError)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UnsupportedCommitError;

impl Display for UnsupportedCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("interactive commit adapters are not configured")
    }
}

impl Error for UnsupportedCommitError {}
