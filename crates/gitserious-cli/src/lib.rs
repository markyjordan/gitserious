//! Command-line delivery and presentation for gitserious.

mod config_view;

use std::error::Error;
use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use gitserious_app::{
    CommitDraftAuthor, CommitDraftAuthorOutcome, CommitOutcome, CommitOutput, CommitWriter,
    InitOutcome, InitStatus, ProjectStateStore, RepositoryLocator, RepositoryRoot,
    UserConfigurationStore, create_commit, initialize_project, load_effective_catalog,
};
use gitserious_core::{CommitMessage, CommitTypeDefinition, CommitTypeId, TemplateId};

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
    Init {
        /// Select an installed template when creating fresh policy.
        #[arg(long, value_name = "TEMPLATE")]
        template: Option<TemplateId>,
    },
    /// Inspect the installed configuration catalog.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigAction {
    /// List every effective taxonomy, typeset, and template.
    List {
        /// Restrict the listing to one entity kind.
        #[arg(value_enum)]
        kind: Option<ConfigKindArg>,
    },
    /// Show one definition in full detail.
    Show {
        /// The entity kind to inspect.
        #[arg(value_enum)]
        kind: ConfigKindArg,
        /// The entity identifier; typesets use TAXONOMY/TYPESET.
        identity: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
enum ConfigKindArg {
    Taxonomy,
    Typeset,
    Template,
}

impl From<ConfigKindArg> for config_view::ConfigurationKind {
    fn from(kind: ConfigKindArg) -> Self {
        match kind {
            ConfigKindArg::Taxonomy => Self::Taxonomy,
            ConfigKindArg::Typeset => Self::Typeset,
            ConfigKindArg::Template => Self::Template,
        }
    }
}

/// Concrete adapters required only by the interactive commit workflow.
#[derive(Clone, Copy)]
pub struct CommitAdapters<'a, A: ?Sized, W: ?Sized> {
    author: &'a A,
    writer: &'a W,
}

impl<'a, A: ?Sized, W: ?Sized> CommitAdapters<'a, A, W> {
    /// Bundles the independent commit-workflow adapters for command dispatch.
    #[must_use]
    pub const fn new(author: &'a A, writer: &'a W) -> Self {
        Self { author, writer }
    }
}

/// Runs the CLI using the process's current directory.
///
/// This compatibility entry point supports initialization. The installable
/// binary uses [`run_with_commit`] to supply concrete commit adapters.
#[must_use]
pub fn run<I, T, L, S, U, Out, Err>(
    arguments: I,
    locator: &L,
    store: &S,
    configuration: &U,
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
    U: UserConfigurationStore + ?Sized,
    U::Error: Display,
    Out: Write + ?Sized,
    Err: Write + ?Sized,
{
    let unavailable = UnsupportedCommitAdapter;
    let commit = CommitAdapters::new(&unavailable, &unavailable);
    run_with_commit(
        arguments,
        locator,
        store,
        configuration,
        &commit,
        stdout,
        stderr,
    )
}

/// Runs the CLI with concrete interactive commit adapters.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn run_with_commit<I, T, L, S, U, A, W, Out, Err>(
    arguments: I,
    locator: &L,
    store: &S,
    configuration: &U,
    commit: &CommitAdapters<'_, A, W>,
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
    U: UserConfigurationStore + ?Sized,
    U::Error: Display,
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
    execute(
        &cli,
        &start,
        locator,
        store,
        configuration,
        commit,
        stdout,
        stderr,
    )
}

/// Runs the CLI from an explicit invocation directory.
///
/// This compatibility entry point supports initialization. Use
/// [`run_from_with_commit`] to exercise commit behavior with explicit adapters.
#[must_use]
pub fn run_from<I, T, L, S, U, Out, Err>(
    arguments: I,
    start: &Path,
    locator: &L,
    store: &S,
    configuration: &U,
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
    U: UserConfigurationStore + ?Sized,
    U::Error: Display,
    Out: Write + ?Sized,
    Err: Write + ?Sized,
{
    let unavailable = UnsupportedCommitAdapter;
    let commit = CommitAdapters::new(&unavailable, &unavailable);
    run_from_with_commit(
        arguments,
        start,
        locator,
        store,
        configuration,
        &commit,
        stdout,
        stderr,
    )
}

/// Runs the CLI from an explicit directory with concrete commit adapters.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn run_from_with_commit<I, T, L, S, U, A, W, Out, Err>(
    arguments: I,
    start: &Path,
    locator: &L,
    store: &S,
    configuration: &U,
    commit: &CommitAdapters<'_, A, W>,
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
    U: UserConfigurationStore + ?Sized,
    U::Error: Display,
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
    execute(
        &cli,
        start,
        locator,
        store,
        configuration,
        commit,
        stdout,
        stderr,
    )
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
fn execute<L, S, U, A, W, Out, Err>(
    cli: &Cli,
    start: &Path,
    locator: &L,
    store: &S,
    configuration: &U,
    commit: &CommitAdapters<'_, A, W>,
    stdout: &mut Out,
    stderr: &mut Err,
) -> ExitCode
where
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectStateStore + ?Sized,
    S::Error: Display,
    U: UserConfigurationStore + ?Sized,
    U::Error: Display,
    A: CommitDraftAuthor + ?Sized,
    A::Error: Display,
    W: CommitWriter + ?Sized,
    W::Error: Display,
    Out: Write + ?Sized,
    Err: Write + ?Sized,
{
    let catalog = match load_effective_catalog(configuration) {
        Ok(catalog) => catalog,
        Err(error) => return write_operational_error(stderr, error),
    };
    match &cli.command {
        Command::Commit { commit_type } => match create_commit(
            locator,
            store,
            &catalog,
            commit.author,
            commit.writer,
            start,
            commit_type.as_ref(),
        ) {
            Ok(outcome) => write_commit_outcome(stdout, stderr, &outcome),
            Err(error) => write_operational_error(stderr, error),
        },
        Command::Init { template } => {
            match initialize_project(locator, store, &catalog, template.as_ref(), start) {
                Ok(outcome) => write_init_outcome(stdout, &outcome),
                Err(error) => write_operational_error(stderr, error),
            }
        }
        Command::Config { action } => match action {
            ConfigAction::List { kind } => write_config_list(
                stdout,
                &catalog,
                kind.map(config_view::ConfigurationKind::from),
            ),
            ConfigAction::Show { kind, identity } => write_config_show(
                stdout,
                stderr,
                &catalog,
                config_view::ConfigurationKind::from(*kind),
                identity,
            ),
        },
    }
}

fn write_config_list(
    stdout: &mut (impl Write + ?Sized),
    catalog: &gitserious_app::ConfigurationCatalog,
    kind: Option<config_view::ConfigurationKind>,
) -> ExitCode {
    let rendered = match kind {
        Some(kind) => config_view::render_list_kind(catalog, kind),
        None => config_view::render_list(catalog),
    };
    if stdout.write_all(rendered.as_bytes()).is_err() {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn write_config_show<Err>(
    stdout: &mut (impl Write + ?Sized),
    stderr: &mut Err,
    catalog: &gitserious_app::ConfigurationCatalog,
    kind: config_view::ConfigurationKind,
    identity: &str,
) -> ExitCode
where
    Err: Write + ?Sized,
{
    let rendered = match resolve_show_target(catalog, kind, identity) {
        Ok(rendered) => rendered,
        Err(error) => return write_operational_error(stderr, error),
    };
    if stdout.write_all(rendered.as_bytes()).is_err() {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn resolve_show_target(
    catalog: &gitserious_app::ConfigurationCatalog,
    kind: config_view::ConfigurationKind,
    identity: &str,
) -> Result<String, String> {
    use config_view::ConfigurationKind;
    match kind {
        ConfigurationKind::Taxonomy => {
            let id = gitserious_core::TaxonomyId::new(identity)
                .map_err(|error| format!("invalid taxonomy identifier {identity:?}: {error}"))?;
            let taxonomy = catalog
                .find_taxonomy(&id)
                .ok_or_else(|| format!("taxonomy {id} was not found"))?;
            Ok(config_view::render_taxonomy(taxonomy))
        }
        ConfigurationKind::Typeset => {
            let Some((taxonomy_text, typeset_text)) = identity.split_once('/') else {
                return Err(format!(
                    "typeset identity must be TAXONOMY/TYPESET, found {identity:?}"
                ));
            };
            let taxonomy = gitserious_core::TaxonomyId::new(taxonomy_text).map_err(|error| {
                format!("invalid taxonomy identifier {taxonomy_text:?}: {error}")
            })?;
            let id = gitserious_core::TypesetId::new(typeset_text)
                .map_err(|error| format!("invalid typeset identifier {typeset_text:?}: {error}"))?;
            let typeset = catalog
                .find_typeset(&taxonomy, &id)
                .ok_or_else(|| format!("typeset {taxonomy}/{id} was not found"))?;
            Ok(config_view::render_typeset(typeset))
        }
        ConfigurationKind::Template => {
            let id = gitserious_core::TemplateId::new(identity)
                .map_err(|error| format!("invalid template identifier {identity:?}: {error}"))?;
            let template = catalog
                .find_template(&id)
                .ok_or_else(|| format!("template {id} was not found"))?;
            Ok(config_view::render_template(template, catalog))
        }
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
