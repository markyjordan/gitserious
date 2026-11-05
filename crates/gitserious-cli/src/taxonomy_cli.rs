use std::error::Error;
use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::io::{IsTerminal, Write};
use std::path::Path;
use std::process::{self, ExitCode, Stdio};

use clap::builder::StyledStr;
use clap::builder::styling::{Style, Styles};
use clap::error::ErrorKind;
use clap::{ArgAction, CommandFactory, FromArgMatches, Parser, Subcommand};
use gitserious_app::{
    CommitDraftAuthor, CommitDraftAuthorOutcome, CommitOutcome, CommitOutput, CommitWriter,
    ConfigurationEditor, GlobalConfigurationStore, HelpLine, HelpPage, HelpSpan, InitOutcome,
    InitStatus, InteractiveEntry, InteractiveUi, ProjectConfigurationStore, RepositoryInitializer,
    RepositoryLocator, RepositoryRoot, RepositoryStatusReader, create_commit_with_taxonomy,
    edit_configuration, initialize_project, run_interactive,
};
use gitserious_core::{CommitMessage, CommitTypeDefinition, VersionedTaxonomy};

const HELP_WIDTH: usize = 100;

#[derive(Debug, Parser)]
#[command(
    name = "gitserious",
    bin_name = "gitserious",
    version,
    about = "Create durable commit-message policy for Git repositories",
    disable_colored_help = true,
    max_term_width = HELP_WIDTH,
    disable_help_flag = true,
    disable_help_subcommand = true,
    after_help = "Use `gitserious help` for more details.",
    after_long_help = "Use `gitserious help <command>` for more information on a specific command.",
    subcommand_required = false
)]
struct Cli {
    #[arg(
        short = 'h',
        global = true,
        action = ArgAction::HelpShort,
        help = "Display the concise help for this command",
        help_heading = "Global options"
    )]
    concise: Option<bool>,
    #[arg(
        long = "help",
        global = true,
        help = "Display the full documentation for this command",
        help_heading = "Global options"
    )]
    full: bool,
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
    let terminal = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let interactive = |entry| {
        let (ui, status, initializer) = adapters
            .interactive
            .ok_or_else(|| "interactive shell is not configured".to_owned())?;
        run_interactive(
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
        )
    };
    let matches = match root().try_get_matches_from(arguments) {
        Ok(matches) => matches,
        Err(error) if error.kind() == ErrorKind::DisplayHelp => {
            let request = HelpRequest::Concise(printed(&error.render()));
            return answer(route(request, terminal), stdout, stderr, interactive);
        }
        Err(error) => return write_clap_error(stdout, stderr, &error),
    };
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => return write_clap_error(stdout, stderr, &error),
    };
    if cli.full {
        let request = HelpRequest::Full(matches.subcommand_name().map(str::to_owned));
        return answer(route(request, terminal), stdout, stderr, interactive);
    }
    let entry = match &cli.command {
        None => Some(InteractiveEntry::Home),
        Some(Command::Config) if adapters.interactive.is_some() => Some(InteractiveEntry::Config),
        _ => None,
    };
    if let Some(entry) = entry {
        if entry == InteractiveEntry::Home && !terminal {
            return write_all(stdout, &printed(&concise_help(root())));
        }
        return interactive(entry).map_or_else(
            |error| operational_error(stderr, error),
            |()| ExitCode::SUCCESS,
        );
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
        Command::Help { command } => answer(
            route(HelpRequest::Topic(command), terminal),
            stdout,
            stderr,
            interactive,
        ),
    }
}

/// The command line as the CLI parses and prints it, where `help` takes only a command's name.
fn root() -> clap::Command {
    let root = Cli::command();
    let names: Vec<String> = root
        .get_subcommands()
        .map(|command| command.get_name().to_owned())
        .collect();
    with_options_usage(root.mut_subcommand("help", |help| {
        help.mut_arg("command", |command| {
            command.value_parser(names).hide_possible_values(true)
        })
    }))
}

/// `root` with `[OPTIONS]` in every command's usage, built for rendering.
fn with_options_usage(mut root: clap::Command) -> clap::Command {
    root.build();
    add_options_tag(&mut root);
    root
}

/// Clap leaves `[OPTIONS]` out of a usage whose only options are help and version, so this
/// overrides such a usage with clap's own, the tag inserted after the command's name.
fn add_options_tag(command: &mut clap::Command) {
    for subcommand in command.get_subcommands_mut() {
        add_options_tag(subcommand);
    }
    let usage = command.render_usage().ansi().to_string();
    let styles = command.get_styles();
    let (title, literal, placeholder) = (
        styles.get_usage(),
        styles.get_literal(),
        styles.get_placeholder(),
    );
    let name = command.get_bin_name().unwrap_or_default();
    let prefix = format!("{title}Usage:{title:#} {literal}{name}{literal:#}");
    let tag = format!("{placeholder}[OPTIONS]{placeholder:#}");
    if let Some(rest) = usage.strip_prefix(&prefix)
        && !rest.starts_with(&format!(" {tag}"))
    {
        let usage = format!("{literal}{name}{literal:#} {tag}{rest}");
        *command = std::mem::take(command).override_usage(StyledStr::from(usage));
    }
}

/// The concise help `-h` prints, rendered by `root`.
fn concise_help(mut root: clap::Command) -> StyledStr {
    root.render_help()
}

/// The long help `gitserious help [<command>]` prints, rendered by `root`.
fn detailed_help(mut root: clap::Command, command: Option<&str>) -> StyledStr {
    root.build();
    match command.and_then(|name| root.find_subcommand_mut(name)) {
        Some(subcommand) => subcommand.render_long_help(),
        None => root.render_long_help(),
    }
}

/// The help an invocation asks for; a `None` command is the root.
#[derive(Clone, Debug, Eq, PartialEq)]
enum HelpRequest {
    /// `-h`, with the concise help clap rendered for the command it followed.
    Concise(String),
    /// `--help` after the root or a command.
    Full(Option<String>),
    /// `help [<command>]`, its command one that exists.
    Topic(Option<String>),
}

/// How the CLI answers a help request.
#[derive(Debug, Eq, PartialEq)]
enum HelpOutcome {
    /// Open the shell at Help, on the named command's page.
    Tui(Option<String>),
    /// Write the help through the pager.
    Page(String),
    /// Write the help to stdout.
    Print(String),
}

/// Answers `request` for a session whose stdin and stdout are both `terminal`s or not.
fn route(request: HelpRequest, terminal: bool) -> HelpOutcome {
    match (request, terminal) {
        (HelpRequest::Concise(text), true) => HelpOutcome::Page(text),
        (HelpRequest::Concise(text), false) => HelpOutcome::Print(text),
        (HelpRequest::Full(command) | HelpRequest::Topic(command), true) => {
            HelpOutcome::Tui(command)
        }
        (HelpRequest::Full(None), false) => HelpOutcome::Print(printed(&concise_help(root()))),
        (HelpRequest::Full(command) | HelpRequest::Topic(command), false) => {
            HelpOutcome::Print(printed(&detailed_help(root(), command.as_deref())))
        }
    }
}

fn answer(
    outcome: HelpOutcome,
    stdout: &mut (impl Write + ?Sized),
    stderr: &mut (impl Write + ?Sized),
    interactive: impl FnOnce(InteractiveEntry) -> Result<(), String>,
) -> ExitCode {
    match outcome {
        HelpOutcome::Tui(command) => interactive(InteractiveEntry::Help { command }).map_or_else(
            |error| operational_error(stderr, error),
            |()| ExitCode::SUCCESS,
        ),
        HelpOutcome::Page(text) if page(&text).is_ok() => ExitCode::SUCCESS,
        HelpOutcome::Page(text) | HelpOutcome::Print(text) => write_all(stdout, &text),
    }
}

/// Help as the CLI prints it: plain text ending in one newline.
fn printed(help: &StyledStr) -> String {
    format!("{}\n", help.to_string().trim_end())
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

/// Help pages for the unified Help surface: what `-h` prints, then what
/// `help <command>` prints for each command.
#[must_use]
pub fn interactive_help() -> Vec<HelpPage> {
    let mut pages = vec![HelpPage::new("Overview", None, |width| {
        help_lines(&concise_help(styled_root(width)))
    })];
    for (title, command) in [("Init", "init"), ("Config", "config"), ("Commit", "commit")] {
        pages.push(HelpPage::new(title, Some(command), move |width| {
            help_lines(&detailed_help(styled_root(width), Some(command)))
        }));
    }
    pages
}

/// The root command as a terminal `width` columns wide renders it, names in bold.
fn styled_root(width: u16) -> clap::Command {
    const BOLD: Style = Style::new().bold();
    // An explicit width bypasses `max_term_width`, so apply the CLI's cap here.
    with_options_usage(
        Cli::command()
            .term_width(usize::from(width).min(HELP_WIDTH))
            .styles(Styles::plain().header(BOLD).usage(BOLD).literal(BOLD)),
    )
}

/// Splits rendered help into lines of spans that are strong while SGR bold is on.
fn help_lines(help: &StyledStr) -> Vec<HelpLine> {
    let ansi = help.ansi().to_string();
    let mut pieces = ansi.split('\x1b');
    let mut lines = Lines::default();
    lines.push(pieces.next().unwrap_or_default());
    for piece in pieces {
        let Some(sequence) = piece.strip_prefix('[') else {
            lines.push(piece);
            continue;
        };
        let end = sequence
            .find(|character| ('@'..='~').contains(&character))
            .unwrap_or(sequence.len());
        let (parameters, rest) = sequence.split_at(end);
        let mut text = rest.chars();
        if text.next() == Some('m') {
            for parameter in parameters.split(';') {
                match parameter {
                    "1" => lines.strong = true,
                    "" | "0" | "22" => lines.strong = false,
                    _ => {}
                }
            }
        }
        lines.push(text.as_str());
    }
    lines.finish()
}

#[derive(Default)]
struct Lines {
    done: Vec<HelpLine>,
    current: HelpLine,
    strong: bool,
}

impl Lines {
    fn push(&mut self, text: &str) {
        for (index, part) in text.split('\n').enumerate() {
            if index > 0 {
                self.done.push(std::mem::take(&mut self.current));
            }
            if part.is_empty() {
                continue;
            }
            match self.current.last_mut() {
                Some(span) if span.strong == self.strong => span.text.push_str(part),
                _ => self.current.push(HelpSpan {
                    text: part.into(),
                    strong: self.strong,
                }),
            }
        }
    }

    /// The lines without trailing blank ones, as `printed` trims them.
    fn finish(mut self) -> Vec<HelpLine> {
        self.done.push(self.current);
        while self
            .done
            .last()
            .is_some_and(|line| line.iter().all(|span| span.text.trim().is_empty()))
        {
            self.done.pop();
        }
        self.done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str, strong: bool) -> HelpSpan {
        HelpSpan {
            text: text.into(),
            strong,
        }
    }

    #[test]
    fn help_opens_the_shell_on_a_terminal_and_prints_when_redirected() {
        let clap_rendered = "Usage: gitserious config [OPTIONS]\n".to_owned();
        let concise = HelpOutcome::Print(printed(&concise_help(root())));
        let long = |command| HelpOutcome::Print(printed(&detailed_help(root(), command)));
        let named = |command: &str| Some(command.to_owned());
        let table = [
            (
                HelpRequest::Concise(clap_rendered.clone()),
                HelpOutcome::Page(clap_rendered.clone()),
                HelpOutcome::Print(clap_rendered),
            ),
            (HelpRequest::Full(None), HelpOutcome::Tui(None), concise),
            (
                HelpRequest::Full(named("config")),
                HelpOutcome::Tui(named("config")),
                long(Some("config")),
            ),
            (
                HelpRequest::Topic(named("config")),
                HelpOutcome::Tui(named("config")),
                long(Some("config")),
            ),
            (HelpRequest::Topic(None), HelpOutcome::Tui(None), long(None)),
            (
                HelpRequest::Topic(named("help")),
                HelpOutcome::Tui(named("help")),
                long(Some("help")),
            ),
        ];

        for (request, terminal, redirected) in table {
            assert_eq!(route(request.clone(), true), terminal, "{request:?}");
            assert_eq!(route(request.clone(), false), redirected, "{request:?}");
        }
        assert_ne!(
            route(HelpRequest::Full(None), false),
            route(HelpRequest::Topic(None), false),
            "redirected `--help` is concise and `help` is long"
        );
    }

    #[test]
    fn usage_shows_options_once_with_or_without_a_real_option() {
        let mut root = with_options_usage(
            clap::Command::new("tool")
                .arg(
                    clap::Arg::new("flag")
                        .long("flag")
                        .action(ArgAction::SetTrue),
                )
                .subcommand(clap::Command::new("sub").arg(clap::Arg::new("path"))),
        );

        assert_eq!(
            root.render_usage().to_string(),
            "Usage: tool [OPTIONS] [COMMAND]"
        );
        let sub = root
            .find_subcommand_mut("sub")
            .map(clap::Command::render_usage);
        assert_eq!(
            sub.map(|usage| usage.to_string()).as_deref(),
            Some("Usage: tool sub [OPTIONS] [path]")
        );
    }

    #[test]
    fn help_spans_are_strong_only_while_sgr_bold_is_on() {
        let help = StyledStr::from(
            "\x1b[1mUsage:\x1b[m a\x1b[1;4mb\x1b[22mc\x1b[1m\nd\x1b[0me\x1b[32mf\x1b[1m\n\n",
        );

        assert_eq!(
            help_lines(&help),
            [
                vec![
                    span("Usage:", true),
                    span(" a", false),
                    span("b", true),
                    span("c", false),
                ],
                vec![span("d", true), span("ef", false)],
            ]
        );
    }
}
