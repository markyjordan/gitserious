use std::ffi::OsString;
use std::fmt::Display;
use std::io::Write;
use std::process::{self, ExitCode, Stdio};

use clap::builder::StyledStr;
use clap::builder::styling::{Style, Styles};
use clap::error::ErrorKind;
use clap::{ArgAction, CommandFactory, FromArgMatches, Parser, Subcommand};
use gitserious_app::{InitOutcome, InitStatus};
use gitserious_core::TaxonomyReference;

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
    /// Config opens in the terminal with two sections. Project shows the
    /// taxonomies installed in this repository's `gitserious.toml` and does
    /// not change them. In a repository without one, it offers to initialize
    /// the project. Library edits the taxonomies in your global
    /// configuration, which new projects copy.
    ///
    /// While you edit a Library item, Ctrl+W saves it and Esc discards the
    /// edit. Copies, reorders, and confirmed deletions save at once.
    Config,
    /// Author and create a durable commit from the staged index.
    ///
    /// Commit opens the Commit page. From that page you start authoring,
    /// choose a type, write its properties, and review the exact message
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

/// The page an interactive command asks the shell to show first.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShellStart {
    Home,
    Commit,
    Config,
    /// Help, on the page of the named command, or the overview for none.
    Help {
        command: Option<String>,
    },
}

/// What an argument list asks gitserious to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Invocation {
    /// Open the unified shell on this page.
    Open(ShellStart),
    /// Initialize the project that contains the start directory.
    Init,
    /// Text the CLI writes without running anything.
    Answer(Answer),
}

/// A response the CLI writes in full, with `write_answer`.
#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    /// Help for a terminal, shown through the pager.
    Page(String),
    /// Help or the version, written to stdout.
    Print(String),
    /// A usage error: clap's message for stderr and the exit code it chose.
    Reject { message: String, code: ExitCode },
}

/// Parses `arguments` for a session whose stdin and stdout are both `terminal`s or not.
pub fn parse<I, T>(arguments: I, terminal: bool) -> Invocation
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let matches = match root().try_get_matches_from(arguments) {
        Ok(matches) => matches,
        Err(error) if error.kind() == ErrorKind::DisplayHelp => {
            return help(HelpRequest::Concise(printed(&error.render())), terminal);
        }
        Err(error) => return rejected(&error),
    };
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => return rejected(&error),
    };
    if cli.full {
        let command = matches.subcommand_name().map(str::to_owned);
        return help(HelpRequest::Full(command), terminal);
    }
    match cli.command {
        Some(Command::Help { command }) => help(HelpRequest::Topic(command), terminal),
        None if !terminal => Invocation::Answer(Answer::Print(printed(&concise_help(root())))),
        None => Invocation::Open(ShellStart::Home),
        Some(Command::Init) => Invocation::Init,
        Some(Command::Config) => Invocation::Open(ShellStart::Config),
        Some(Command::Commit) => Invocation::Open(ShellStart::Commit),
    }
}

/// Clap reports the version as an error that belongs on stdout.
fn rejected(error: &clap::Error) -> Invocation {
    Invocation::Answer(if error.use_stderr() {
        Answer::Reject {
            message: error.to_string(),
            code: exit_code(error.exit_code()),
        }
    } else {
        Answer::Print(error.to_string())
    })
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

/// Answers `request` for a session whose stdin and stdout are both `terminal`s or not.
fn help(request: HelpRequest, terminal: bool) -> Invocation {
    match (request, terminal) {
        (HelpRequest::Concise(text), true) => Invocation::Answer(Answer::Page(text)),
        (HelpRequest::Concise(text), false) => Invocation::Answer(Answer::Print(text)),
        (HelpRequest::Full(command) | HelpRequest::Topic(command), true) => {
            Invocation::Open(ShellStart::Help { command })
        }
        (HelpRequest::Full(None), false) => {
            Invocation::Answer(Answer::Print(printed(&concise_help(root()))))
        }
        (HelpRequest::Full(command) | HelpRequest::Topic(command), false) => Invocation::Answer(
            Answer::Print(printed(&detailed_help(root(), command.as_deref()))),
        ),
    }
}

/// Help as the CLI prints it: plain text ending in one newline.
fn printed(help: &StyledStr) -> String {
    format!("{}\n", help.to_string().trim_end())
}

/// Writes `answer` and returns the exit code it carries. Paged help falls back to stdout
/// when the pager cannot start.
pub fn write_answer(
    answer: Answer,
    stdout: &mut (impl Write + ?Sized),
    stderr: &mut (impl Write + ?Sized),
) -> ExitCode {
    match answer {
        Answer::Page(text) if page(&text).is_ok() => ExitCode::SUCCESS,
        Answer::Page(text) | Answer::Print(text) => write_all(stdout, &text),
        Answer::Reject { message, code } => {
            if stderr.write_all(message.as_bytes()).is_ok() {
                code
            } else {
                ExitCode::FAILURE
            }
        }
    }
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

/// Writes the line that reports `outcome`.
pub fn write_init_outcome(output: &mut (impl Write + ?Sized), outcome: &InitOutcome) -> ExitCode {
    let action = match outcome.status() {
        InitStatus::Initialized => "Initialized gitserious in",
        InitStatus::AlreadyInitialized => "gitserious is already initialized in",
    };
    if writeln!(
        output,
        "{action} {} ({}).",
        outcome.root().as_path().display(),
        TaxonomyReference::new(outcome.taxonomy(), outcome.version())
    )
    .is_ok()
    {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Writes `error` as an `error: ` line and returns failure.
pub fn write_error(output: &mut (impl Write + ?Sized), error: impl Display) -> ExitCode {
    let _ = writeln!(output, "error: {error}");
    ExitCode::FAILURE
}

fn exit_code(code: i32) -> ExitCode {
    u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from)
}

/// One page of the interactive Help: the overview, or a command's documentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HelpTopic {
    pub title: &'static str,
    /// The command the topic documents; the overview documents none.
    pub command: Option<&'static str>,
}

/// The overview, then each command, in the order Help lists them.
#[must_use]
pub const fn help_topics() -> [HelpTopic; 4] {
    [
        HelpTopic {
            title: "Overview",
            command: None,
        },
        HelpTopic {
            title: "Init",
            command: Some("init"),
        },
        HelpTopic {
            title: "Config",
            command: Some("config"),
        },
        HelpTopic {
            title: "Commit",
            command: Some("commit"),
        },
    ]
}

/// What `-h` prints for no `command`, or what `help <command>` prints, wrapped to `width`
/// columns with headings and names in SGR bold.
#[must_use]
pub fn styled_help(command: Option<&str>, width: u16) -> String {
    let help = match command {
        None => concise_help(styled_root(width)),
        Some(command) => detailed_help(styled_root(width), Some(command)),
    };
    help.ansi().to_string()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn invocations(arguments: &[&str]) -> [Invocation; 2] {
        let arguments = || std::iter::once("gitserious").chain(arguments.iter().copied());
        [parse(arguments(), true), parse(arguments(), false)]
    }

    fn shell(page: ShellStart) -> Invocation {
        Invocation::Open(page)
    }

    fn help_on(command: Option<&str>) -> Invocation {
        shell(ShellStart::Help {
            command: command.map(str::to_owned),
        })
    }

    fn print(text: String) -> Invocation {
        Invocation::Answer(Answer::Print(text))
    }

    fn printed_long(command: Option<&str>) -> Invocation {
        print(printed(&detailed_help(root(), command)))
    }

    #[test]
    fn commands_select_a_shell_page_and_commit_does_not_author() {
        let concise = print(printed(&concise_help(root())));
        let table = [
            (&[][..], [shell(ShellStart::Home), concise]),
            (&["init"], [Invocation::Init, Invocation::Init]),
            (
                &["config"],
                [shell(ShellStart::Config), shell(ShellStart::Config)],
            ),
            (
                &["commit"],
                [shell(ShellStart::Commit), shell(ShellStart::Commit)],
            ),
        ];

        for (arguments, expected) in table {
            assert_eq!(invocations(arguments), expected, "{arguments:?}");
        }
    }

    #[test]
    fn help_opens_the_shell_on_a_terminal_and_prints_when_redirected() {
        let concise = print(printed(&concise_help(root())));
        let table = [
            (&["--help"][..], [help_on(None), concise]),
            (
                &["config", "--help"],
                [help_on(Some("config")), printed_long(Some("config"))],
            ),
            (&["help"], [help_on(None), printed_long(None)]),
            (
                &["help", "config"],
                [help_on(Some("config")), printed_long(Some("config"))],
            ),
            (
                &["help", "help"],
                [help_on(Some("help")), printed_long(Some("help"))],
            ),
        ];

        for (arguments, expected) in table {
            assert_eq!(invocations(arguments), expected, "{arguments:?}");
        }
        assert_ne!(
            invocations(&["--help"])[1],
            invocations(&["help"])[1],
            "redirected `--help` is concise and `help` is long"
        );
    }

    #[test]
    fn concise_help_pages_on_a_terminal_and_prints_when_redirected() {
        for arguments in [&["-h"][..], &["config", "-h"], &["commit", "-h"]] {
            let [
                Invocation::Answer(Answer::Page(paged)),
                Invocation::Answer(Answer::Print(printed)),
            ] = invocations(arguments)
            else {
                unreachable!("{arguments:?} is concise help");
            };
            assert_eq!(paged, printed, "{arguments:?}");
            assert!(printed.contains("Usage: gitserious"), "{printed}");
        }
        assert_eq!(
            invocations(&["-h"])[1],
            print(printed(&concise_help(root())))
        );
    }

    #[test]
    fn the_version_prints_on_stdout_whatever_the_terminal() {
        let version = print(format!("gitserious {}\n", env!("CARGO_PKG_VERSION")));

        assert_eq!(invocations(&["--version"]), [version.clone(), version]);
    }

    #[test]
    fn usage_errors_reject_with_the_clap_message_and_code_two() {
        for arguments in [
            &["unknown"][..],
            &["init", "extra"],
            &["init", "--taxonomy", "research"],
            &["commit", "--type", "feat"],
            &["commit", "--template", "default"],
            &["config", "fork"],
            &["help", "unknown"],
        ] {
            for invocation in invocations(arguments) {
                let Invocation::Answer(Answer::Reject { message, code }) = invocation else {
                    unreachable!("{arguments:?} is a usage error");
                };
                assert_eq!(code, ExitCode::from(2), "{arguments:?}");
                assert!(message.starts_with("error: "), "{message}");
            }
        }
    }

    struct Closed;

    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn answers_write_their_text_to_one_stream_with_their_exit_code() {
        let rejected = || Answer::Reject {
            message: "error: unexpected argument\n".into(),
            code: ExitCode::from(2),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let printed = write_answer(Answer::Print("help\n".into()), &mut stdout, &mut stderr);
        let rejection = write_answer(rejected(), &mut stdout, &mut stderr);

        assert_eq!((printed, rejection), (ExitCode::SUCCESS, ExitCode::from(2)));
        assert_eq!(stdout, b"help\n");
        assert_eq!(stderr, b"error: unexpected argument\n");
        assert_eq!(
            write_answer(Answer::Print("help\n".into()), &mut Closed, &mut Vec::new()),
            ExitCode::FAILURE
        );
        assert_eq!(
            write_answer(rejected(), &mut Vec::new(), &mut Closed),
            ExitCode::FAILURE
        );
    }

    #[test]
    fn errors_are_one_prefixed_line_and_fail_even_when_unwritable() {
        let mut stderr = Vec::new();

        assert_eq!(
            write_error(&mut stderr, "help shell rejected"),
            ExitCode::FAILURE
        );
        assert_eq!(stderr, b"error: help shell rejected\n");
        assert_eq!(write_error(&mut Closed, "lost"), ExitCode::FAILURE);
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
}
