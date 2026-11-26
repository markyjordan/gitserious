use std::ffi::OsString;
use std::process::ExitCode;

use clap::builder::StyledStr;
use clap::builder::styling::{Style, Styles};
use clap::error::{ContextKind, ContextValue, ErrorKind};
use clap::{CommandFactory, FromArgMatches, Parser};

use crate::commands::Command;
use crate::invocation::{Answer, Destination, Invocation};

const HELP_WIDTH: usize = 100;

#[derive(Debug, Parser)]
#[command(
    name = "gitserious",
    bin_name = "gitserious",
    version,
    about = "Structured commits that keep what the diff leaves out.",
    disable_colored_help = true,
    max_term_width = HELP_WIDTH,
    disable_help_flag = true,
    disable_version_flag = true,
    args_conflicts_with_subcommands = true,
    disable_help_subcommand = true,
    after_help = "Use `gitserious help` for more details.",
    after_long_help = "Use `gitserious help <command>` for more information on a specific command.",
    subcommand_required = false
)]
struct Cli {
    #[arg(
        short = 'h',
        long = "help",
        conflicts_with = "print_version",
        help = "Print the concise help"
    )]
    print_help: bool,
    #[arg(short = 'V', long = "version", help = "Print the version")]
    print_version: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

/// Parses `arguments` for a session whose stdin and stdout are both `terminal`s or not.
pub fn parse<I, T>(arguments: I, terminal: bool) -> Invocation
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let arguments: Vec<OsString> = arguments.into_iter().map(Into::into).collect();
    let cli = match root()
        .try_get_matches_from(&arguments)
        .and_then(|matches| Cli::from_arg_matches(&matches))
    {
        Ok(cli) => cli,
        Err(error) => return rejected(error, &arguments),
    };
    if cli.print_help {
        return Invocation::Answer(Answer::Print(printed(&HelpText::Concise.render(root()))));
    }
    if cli.print_version {
        return Invocation::Answer(Answer::Print(root().render_version()));
    }
    match cli.command {
        Some(Command::Help { command }) => help(command, terminal),
        None if !terminal => {
            Invocation::Answer(Answer::Print(printed(&HelpText::Concise.render(root()))))
        }
        None => Invocation::Open(Destination::Home),
        Some(Command::Init) => Invocation::Init,
        Some(Command::Config) => Invocation::Open(Destination::Config),
        Some(Command::Commit) => Invocation::Open(Destination::Commit),
    }
}

/// Clap's usage error, closed by the way out when a command was given a flag only gitserious takes.
fn rejected(mut error: clap::Error, arguments: &[OsString]) -> Invocation {
    let message = match root_flag_tip(&error, arguments) {
        Some(tip) => {
            // Clap would suggest passing the flag to `help` as a value after `--`.
            error.remove(ContextKind::Suggested);
            format!("{}\n\n{tip}\n", error.to_string().trim_end())
        }
        None => error.to_string(),
    };
    Invocation::Answer(Answer::Reject {
        message,
        code: exit_code(error.exit_code()),
    })
}

/// Clap names the root flag a command was given: the flag before the command conflicts with it,
/// and a command does not know the flag after it.
fn root_flag_tip(error: &clap::Error, arguments: &[OsString]) -> Option<String> {
    let flag = match error.kind() {
        ErrorKind::ArgumentConflict if error.get(ContextKind::InvalidSubcommand).is_some() => {
            error.get(ContextKind::PriorArg)
        }
        ErrorKind::UnknownArgument => error.get(ContextKind::InvalidArg),
        _ => None,
    };
    let Some(ContextValue::String(flag)) = flag else {
        return None;
    };
    match flag.as_str() {
        "-h" | "--help" => Some(format!(
            "Use `gitserious help {}` for command-specific documentation.",
            documented_command(arguments).unwrap_or("<COMMAND>")
        )),
        "-V" | "--version" => {
            Some("Use `gitserious --version` without a command to print the version.".to_owned())
        }
        _ => None,
    }
}

/// The last argument that names a command, which `help commit -h` asks to document.
fn documented_command(arguments: &[OsString]) -> Option<&str> {
    let root = root();
    arguments
        .iter()
        .skip(1)
        .rev()
        .filter_map(|argument| argument.to_str())
        .find(|argument| root.find_subcommand(argument).is_some())
}

/// The command line as the CLI parses and prints it, where `help` takes only a command's name.
fn root() -> clap::Command {
    let root = Cli::command();
    let names: Vec<String> = root
        .get_subcommands()
        .map(|command| command.get_name().to_owned())
        .collect();
    with_root_usage(root.mut_subcommand("help", |help| {
        help.mut_arg("command", |command| {
            command.value_parser(names).hide_possible_values(true)
        })
    }))
}

/// The root takes a command or one of its options, never both. Clap would list the options form
/// first and leave out `[OPTIONS]`, because the root's only options are help and version.
fn with_root_usage(root: clap::Command) -> clap::Command {
    let styles = root.get_styles();
    let (literal, placeholder) = (styles.get_literal(), styles.get_placeholder());
    let name = root.get_name();
    let usage = format!(
        "{literal}{name}{literal:#} {placeholder}[COMMAND]{placeholder:#}\n       \
         {literal}{name}{literal:#} {placeholder}[OPTIONS]{placeholder:#}"
    );
    root.override_usage(StyledStr::from(usage))
}

/// A help text the CLI prints and Help shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HelpText<'a> {
    /// What `-h` and `--help` print: the root's summary, which points to `gitserious help`.
    Concise,
    /// What `help` prints and the Help Overview shows: the root's full documentation.
    Overview,
    /// What `help <command>` prints and the command's Help page shows.
    Command(&'a str),
}

impl<'a> HelpText<'a> {
    /// What `help [<command>]` documents.
    fn documenting(command: Option<&'a str>) -> Self {
        command.map_or(Self::Overview, Self::Command)
    }

    /// The text as `root` renders it.
    fn render(self, mut root: clap::Command) -> StyledStr {
        match self {
            Self::Concise => root.render_help(),
            Self::Overview => root.render_long_help(),
            Self::Command(name) => {
                root.build();
                match root.find_subcommand_mut(name) {
                    Some(command) => command.render_long_help(),
                    None => root.render_long_help(),
                }
            }
        }
    }
}

/// Answers `help [<command>]`, its command one that exists, for a session whose stdin and stdout
/// are both `terminal`s or not.
fn help(command: Option<String>, terminal: bool) -> Invocation {
    if terminal {
        Invocation::Open(Destination::Help { command })
    } else {
        Invocation::Answer(Answer::Print(printed(
            &HelpText::documenting(command.as_deref()).render(root()),
        )))
    }
}

/// Help as the CLI prints it: plain text ending in one newline.
fn printed(help: &StyledStr) -> String {
    format!("{}\n", help.to_string().trim_end())
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

/// What `help [<command>]` prints, wrapped to `width` columns with headings and names in SGR
/// bold.
#[must_use]
pub fn styled_help(command: Option<&str>, width: u16) -> String {
    HelpText::documenting(command)
        .render(styled_root(width))
        .ansi()
        .to_string()
}

/// The root command as a terminal `width` columns wide renders it, names in bold.
fn styled_root(width: u16) -> clap::Command {
    const BOLD: Style = Style::new().bold();
    // An explicit width bypasses `max_term_width`, so apply the CLI's cap here.
    with_root_usage(
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

    fn open(destination: Destination) -> Invocation {
        Invocation::Open(destination)
    }

    fn help_on(command: Option<&str>) -> Invocation {
        open(Destination::Help {
            command: command.map(str::to_owned),
        })
    }

    fn print(text: String) -> Invocation {
        Invocation::Answer(Answer::Print(text))
    }

    fn printed_long(command: Option<&str>) -> Invocation {
        print(printed(&HelpText::documenting(command).render(root())))
    }

    #[test]
    fn commands_select_a_destination_and_commit_does_not_author() {
        let concise = print(printed(&HelpText::Concise.render(root())));
        let table = [
            (&[][..], [open(Destination::Home), concise]),
            (&["init"], [Invocation::Init, Invocation::Init]),
            (
                &["config"],
                [open(Destination::Config), open(Destination::Config)],
            ),
            (
                &["commit"],
                [open(Destination::Commit), open(Destination::Commit)],
            ),
        ];

        for (arguments, expected) in table {
            assert_eq!(invocations(arguments), expected, "{arguments:?}");
        }
    }

    #[test]
    fn the_help_command_opens_help_on_a_terminal_and_prints_when_redirected() {
        let table = [
            (&["help"][..], [help_on(None), printed_long(None)]),
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
            "`--help` is concise and redirected `help` is long"
        );
    }

    #[test]
    fn the_root_help_and_version_flags_print_on_stdout_whatever_the_terminal() {
        let concise = print(printed(&HelpText::Concise.render(root())));
        let version = print(format!("gitserious {}\n", env!("CARGO_PKG_VERSION")));

        for (flag, expected) in [
            ("-h", &concise),
            ("--help", &concise),
            ("-V", &version),
            ("--version", &version),
        ] {
            assert_eq!(
                invocations(&[flag]),
                [expected.clone(), expected.clone()],
                "{flag}"
            );
        }
    }

    #[test]
    fn root_flags_with_a_command_are_usage_errors_that_name_the_way_out() {
        let version = "Use `gitserious --version` without a command to print the version.";
        for (command, documented) in [
            (&["init"][..], "init"),
            (&["config"], "config"),
            (&["commit"], "commit"),
            (&["help"], "help"),
            (&["help", "commit"], "commit"),
            (&["nope"], "<COMMAND>"),
        ] {
            let help =
                format!("Use `gitserious help {documented}` for command-specific documentation.");
            for (flag, tip) in [
                ("-h", help.as_str()),
                ("--help", &help),
                ("-V", version),
                ("--version", version),
            ] {
                let mut placements = vec![[&[flag], command].concat()];
                if documented != "<COMMAND>" {
                    placements.push([command, &[flag]].concat());
                }
                for arguments in placements {
                    for invocation in invocations(&arguments) {
                        let Invocation::Answer(Answer::Reject { message, code }) = invocation
                        else {
                            unreachable!("{arguments:?} is a usage error");
                        };
                        assert_eq!(code, ExitCode::from(2), "{arguments:?}");
                        assert!(message.starts_with("error: "), "{message}");
                        assert!(message.ends_with(&format!("\n\n{tip}\n")), "{message}");
                        assert!(!message.contains("tip:"), "{message}");
                    }
                }
            }
        }
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
            &["-h", "-V"],
        ] {
            for invocation in invocations(arguments) {
                let Invocation::Answer(Answer::Reject { message, code }) = invocation else {
                    unreachable!("{arguments:?} is a usage error");
                };
                assert_eq!(code, ExitCode::from(2), "{arguments:?}");
                assert!(message.starts_with("error: "), "{message}");
                assert!(!message.contains("\nUse `gitserious"), "{message}");
            }
        }
    }

    #[test]
    fn the_root_usage_lists_the_command_form_then_the_options_form() {
        let mut root = root();

        assert_eq!(
            root.render_usage().to_string(),
            "Usage: gitserious [COMMAND]\n       gitserious [OPTIONS]"
        );
        root.build();
        let commit = root
            .find_subcommand_mut("commit")
            .map(clap::Command::render_usage);
        assert_eq!(
            commit.map(|usage| usage.to_string()).as_deref(),
            Some("Usage: gitserious commit")
        );
    }
}
