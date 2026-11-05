use std::error::Error;
use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::io::{IsTerminal, Write};
use std::path::Path;
use std::process::{self, ExitCode, Stdio};

use clap::error::ErrorKind;
use clap::{ArgAction, CommandFactory, Parser, Subcommand};
use gitserious_app::{
    CommitDraftAuthor, CommitDraftAuthorOutcome, CommitOutcome, CommitOutput, CommitWriter,
    ConfigurationEditor, GlobalConfigurationStore, HelpPage, InitOutcome, InitStatus,
    InteractiveEntry, InteractiveUi, ProjectConfigurationStore, RepositoryInitializer,
    RepositoryLocator, RepositoryRoot, RepositoryStatusReader, create_commit_with_taxonomy,
    edit_configuration, initialize_project, run_interactive,
};
use gitserious_core::{CommitMessage, CommitTypeDefinition, VersionedTaxonomy};

#[derive(Debug, Parser)]
#[command(
    name = "gitserious",
    bin_name = "gitserious",
    version,
    about = "Create durable commit-message policy for Git repositories",
    disable_colored_help = true,
    max_term_width = 100,
    disable_help_flag = true,
    disable_help_subcommand = true,
    after_help = "Use `gitserious help` for more details.",
    after_long_help = "Use `gitserious help <command>` for more information on a specific command.",
    subcommand_required = false
)]
struct Cli {
    #[arg(
        short,
        long,
        global = true,
        action = ArgAction::HelpShort,
        help = "Display the concise help for this command"
    )]
    help: Option<bool>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize repository-local gitserious policy.
    ///
    /// Init writes `gitserious.toml` at the root of the current Git repository.
    /// It holds complete copies of the taxonomies your Library gives new
    /// projects. The Library's default taxonomy becomes the project default.
    ///
    /// An existing `gitserious.toml` is never rewritten.
    Init,
    /// Interactively configure taxonomy policy.
    ///
    /// Config opens in the terminal with two sections. Project edits the
    /// taxonomies in this repository's `gitserious.toml`. Library edits the
    /// taxonomies in your global configuration, which new projects copy.
    ///
    /// Library edits save when you press Esc. Copies, reorders, and confirmed
    /// deletions save at once. Project changes save only after you review and
    /// apply them.
    Config,
    /// Author and create a durable commit from the staged index.
    ///
    /// Commit starts from the project default taxonomy in `gitserious.toml`.
    /// You choose a type, write its properties, and review the exact message
    /// before gitserious passes it to `git commit`.
    ///
    /// Only staged changes are committed. Stage them with `git add` first.
    Commit,
    /// Display documentation for a command
    Help {
        /// Print the documentation directly instead of through a pager
        #[arg(long)]
        no_pager: bool,
        /// The command to document
        command: Option<String>,
    },
}

type InteractiveAdapters<'a> = (
    &'a dyn InteractiveUi,
    &'a dyn RepositoryStatusReader,
    &'a dyn RepositoryInitializer,
);

/// Concrete interaction adapters required by the binary.
#[derive(Clone, Copy)]
pub struct CommitAdapters<'a, A: ?Sized, W: ?Sized> {
    author: &'a A,
    writer: &'a W,
    configuration_editor: Option<&'a dyn ConfigurationEditor>,
    interactive: Option<InteractiveAdapters<'a>>,
}

impl<'a, A: ?Sized, W: ?Sized> CommitAdapters<'a, A, W> {
    #[must_use]
    pub const fn new(author: &'a A, writer: &'a W) -> Self {
        Self {
            author,
            writer,
            configuration_editor: None,
            interactive: None,
        }
    }

    /// Supplies the unified shell, its read-only Git status, and `git init` for Home.
    #[must_use]
    pub fn with_interactive(
        mut self,
        ui: &'a dyn InteractiveUi,
        status: &'a dyn RepositoryStatusReader,
        initializer: &'a dyn RepositoryInitializer,
    ) -> Self {
        self.interactive = Some((ui, status, initializer));
        self
    }

    #[must_use]
    pub fn with_configuration_editor(mut self, editor: &'a dyn ConfigurationEditor) -> Self {
        self.configuration_editor = Some(editor);
        self
    }
}

/// Runs from the process current directory without interactive adapters.
#[must_use]
pub fn run<I, T, L, S, G, Out, Err>(
    arguments: I,
    locator: &L,
    store: &S,
    global: &G,
    stdout: &mut Out,
    stderr: &mut Err,
) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectConfigurationStore + ?Sized,
    S::Error: Display,
    G: GlobalConfigurationStore + ?Sized,
    G::Error: Display,
    Out: Write + ?Sized,
    Err: Write + ?Sized,
{
    let unavailable = UnsupportedCommitAdapter;
    run_with_commit(
        arguments,
        locator,
        store,
        global,
        &CommitAdapters::new(&unavailable, &unavailable),
        stdout,
        stderr,
    )
}

/// Runs from the process current directory with concrete adapters.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn run_with_commit<I, T, L, S, G, A, W, Out, Err>(
    arguments: I,
    locator: &L,
    store: &S,
    global: &G,
    adapters: &CommitAdapters<'_, A, W>,
    stdout: &mut Out,
    stderr: &mut Err,
) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectConfigurationStore + ?Sized,
    S::Error: Display,
    G: GlobalConfigurationStore + ?Sized,
    G::Error: Display,
    A: CommitDraftAuthor + ?Sized,
    A::Error: Display,
    W: CommitWriter + ?Sized,
    W::Error: Display,
    Out: Write + ?Sized,
    Err: Write + ?Sized,
{
    let start = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => return operational_error(stderr, error),
    };
    run_from_with_commit(
        arguments, &start, locator, store, global, adapters, stdout, stderr,
    )
}

/// Runs from an explicit directory without interactive adapters.
#[must_use]
pub fn run_from<I, T, L, S, G, Out, Err>(
    arguments: I,
    start: &Path,
    locator: &L,
    store: &S,
    global: &G,
    stdout: &mut Out,
    stderr: &mut Err,
) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectConfigurationStore + ?Sized,
    S::Error: Display,
    G: GlobalConfigurationStore + ?Sized,
    G::Error: Display,
    Out: Write + ?Sized,
    Err: Write + ?Sized,
{
    let unavailable = UnsupportedCommitAdapter;
    run_from_with_commit(
        arguments,
        start,
        locator,
        store,
        global,
        &CommitAdapters::new(&unavailable, &unavailable),
        stdout,
        stderr,
    )
}

/// Runs from an explicit directory with concrete adapters.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn run_from_with_commit<I, T, L, S, G, A, W, Out, Err>(
    arguments: I,
    start: &Path,
    locator: &L,
    store: &S,
    global: &G,
    adapters: &CommitAdapters<'_, A, W>,
    stdout: &mut Out,
    stderr: &mut Err,
) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
    L: RepositoryLocator + ?Sized,
    L::Error: Display,
    S: ProjectConfigurationStore + ?Sized,
    S::Error: Display,
    G: GlobalConfigurationStore + ?Sized,
    G::Error: Display,
    A: CommitDraftAuthor + ?Sized,
    A::Error: Display,
    W: CommitWriter + ?Sized,
    W::Error: Display,
    Out: Write + ?Sized,
    Err: Write + ?Sized,
{
    let cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(error) => return write_clap_error(stdout, stderr, &error),
    };
    let entry = match &cli.command {
        None => Some(InteractiveEntry::Home),
        Some(Command::Config) if adapters.interactive.is_some() => Some(InteractiveEntry::Config),
        _ => None,
    };
    if let Some(entry) = entry {
        if entry == InteractiveEntry::Home
            && (!std::io::stdin().is_terminal() || !std::io::stdout().is_terminal())
        {
            return write_all(stdout, &concise_help());
        }
        let Some((ui, status, initializer)) = adapters.interactive else {
            return operational_error(stderr, "interactive shell is not configured");
        };
        return match run_interactive(
            locator,
            store,
            global,
            status,
            initializer,
            adapters.writer,
            start,
            ui,
            entry,
            &interactive_help(),
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => operational_error(stderr, error),
        };
    }
    let Some(command) = cli.command else {
        return ExitCode::SUCCESS;
    };
    match command {
        Command::Commit => match create_commit_with_taxonomy(
            locator,
            store,
            adapters.author,
            adapters.writer,
            start,
        ) {
            Ok(CommitOutcome::Created(output)) => write_commit_output(stdout, stderr, &output),
            Ok(CommitOutcome::Cancelled) => {
                let _ = writeln!(stderr, "Commit cancelled.");
                ExitCode::FAILURE
            }
            Err(error) => operational_error(stderr, error),
        },
        Command::Init => {
            let global = match global.load() {
                Ok(value) => value,
                Err(error) => return operational_error(stderr, error),
            };
            match initialize_project(locator, store, &global, start) {
                Ok(outcome) => write_init_outcome(stdout, &outcome),
                Err(error) => operational_error(stderr, error),
            }
        }
        Command::Config => {
            let Some(editor) = adapters.configuration_editor else {
                return operational_error(stderr, "configuration editor is not configured");
            };
            match edit_configuration(locator, store, global, start, editor) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => operational_error(stderr, error),
            }
        }
        Command::Help { no_pager, command } => match detailed_help(command.as_deref()) {
            Ok(text) => write_help(stdout, &text, no_pager),
            Err(error) => write_clap_error(stdout, stderr, &error),
        },
    }
}

fn concise_help() -> String {
    trailing_newline(&Cli::command().render_help().to_string())
}

/// The long help `gitserious help [<command>]` prints.
fn detailed_help(command: Option<&str>) -> Result<String, clap::Error> {
    let mut root = Cli::command();
    root.build();
    let Some(name) = command else {
        return Ok(trailing_newline(&root.render_long_help().to_string()));
    };
    if let Some(subcommand) = root.find_subcommand_mut(name) {
        return Ok(trailing_newline(&subcommand.render_long_help().to_string()));
    }
    let names: Vec<_> = root
        .get_subcommands()
        .map(clap::Command::get_name)
        .collect();
    Err(clap::Error::raw(
        ErrorKind::InvalidValue,
        format!(
            "there is no command '{name}'\n\n  [possible commands: {}]\n",
            names.join(", ")
        ),
    ))
}

fn trailing_newline(text: &str) -> String {
    format!("{}\n", text.trim_end())
}

fn write_help(stdout: &mut (impl Write + ?Sized), text: &str, no_pager: bool) -> ExitCode {
    if !no_pager && std::io::stdout().is_terminal() && page(text).is_ok() {
        return ExitCode::SUCCESS;
    }
    write_all(stdout, text)
}

fn page(text: &str) -> std::io::Result<()> {
    let pager = std::env::var("PAGER").unwrap_or_default();
    let mut words = pager.split_whitespace();
    let mut command = process::Command::new(words.next().unwrap_or("less"));
    command.args(words).stdin(Stdio::piped());
    // Git's default: exit on one screen of text and leave it in scrollback.
    if std::env::var_os("LESS").is_none() {
        command.env("LESS", "FRX");
    }
    let mut child = command.spawn()?;
    if let Some(mut input) = child.stdin.take() {
        // Quitting the pager early closes the pipe, which is not an error.
        let _ = input.write_all(text.as_bytes());
    }
    child.wait()?;
    Ok(())
}

fn write_all(stdout: &mut (impl Write + ?Sized), text: &str) -> ExitCode {
    if stdout.write_all(text.as_bytes()).is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn write_clap_error(
    stdout: &mut (impl Write + ?Sized),
    stderr: &mut (impl Write + ?Sized),
    error: &clap::Error,
) -> ExitCode {
    let result = if error.use_stderr() {
        write!(stderr, "{error}")
    } else {
        write!(stdout, "{error}")
    };
    if result.is_ok() {
        exit_code(error.exit_code())
    } else {
        ExitCode::FAILURE
    }
}

fn write_commit_output(
    stdout: &mut (impl Write + ?Sized),
    stderr: &mut (impl Write + ?Sized),
    output: &CommitOutput,
) -> ExitCode {
    if stdout.write_all(output.stdout()).is_ok() && stderr.write_all(output.stderr()).is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn write_init_outcome(output: &mut (impl Write + ?Sized), outcome: &InitOutcome) -> ExitCode {
    let action = match outcome.status() {
        InitStatus::Initialized => "Initialized gitserious in",
        InitStatus::AlreadyInitialized => "gitserious is already initialized in",
    };
    if writeln!(
        output,
        "{action} {} ({}).",
        outcome.root().as_path().display(),
        VersionedTaxonomy::new(outcome.taxonomy(), outcome.version())
    )
    .is_ok()
    {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn operational_error(output: &mut (impl Write + ?Sized), error: impl Display) -> ExitCode {
    let _ = writeln!(output, "error: {error}");
    ExitCode::FAILURE
}

fn exit_code(code: i32) -> ExitCode {
    u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from)
}

#[derive(Clone, Copy, Debug)]
struct UnsupportedCommitAdapter;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UnsupportedCommitError;

impl Display for UnsupportedCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("interactive commit adapters are not configured")
    }
}

impl Error for UnsupportedCommitError {}

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

/// Help pages for the unified Help surface, each the text the CLI prints.
#[must_use]
pub fn interactive_help() -> Vec<HelpPage> {
    let mut pages = vec![HelpPage {
        title: "Overview".into(),
        text: concise_help(),
    }];
    for (title, command) in [("Init", "init"), ("Config", "config"), ("Commit", "commit")] {
        pages.push(HelpPage {
            title: title.into(),
            text: detailed_help(Some(command)).unwrap_or_else(|error| error.to_string()),
        });
    }
    pages.push(HelpPage { title: "Keyboard Navigation".into(), text: "NAME\n    gitserious — keyboard navigation\n\nDESCRIPTION\n    Tab / Shift+Tab: switch to the next or previous section from any overview, including Config's Project and Library.\n    Esc: close an overlay, go back, or return to Home from an overview.\n    Enter: open the selected item or start a staged commit.\n    /: contextual commands. ?: contextual help. q: quit at overview level.\n\nCONFIG\n    Menu: Up/Down selects Project or Library; Left/Right moves focus between Menu and its widgets.\n    Library: n creates, e edits, f forks, and d deletes a taxonomy.\n    Library NAV: Enter opens or closes a node; arrows move rows or panel focus.\n    Library EDIT: Enter and Up/Down move fields; Esc validates and saves. Correct inline errors before leaving EDIT.\n    Tab, navigation clicks, and quit cannot leave Library EDIT. Esc removes a new item only while it still equals its initial blank values.\n    Library copies, reorders, and confirmed deletions save immediately. A taxonomy needs a type before it can be included in a project.\n    Project: Ctrl+S reviews changes; Enter applies from Review.\n\nCOMMIT\n    Overview: r refreshes; o shows the full Git result; Left/Right controls directories.\n    Picker: Tab / Shift+Tab selects taxonomy; Up/Down selects type; Enter composes.\n    Composer: Ctrl+S validates and reviews; Esc returns through discard protection.\n    Review: Enter submits; Esc returns to editing.\n\nHELP\n    Left/Right selects pages; Up/Down and Page Up/Page Down scroll.\n".into() });
    pages
}
