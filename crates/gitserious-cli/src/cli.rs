use std::ffi::OsString;
use std::process::ExitCode;

use clap::builder::StyledStr;
use clap::builder::styling::{Style, Styles};
use clap::{CommandFactory, FromArgMatches, Parser};

use crate::commands::Command;
use crate::invocation::{Answer, Destination, Invocation};

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
    let cli = match root()
        .try_get_matches_from(arguments)
        .and_then(|matches| Cli::from_arg_matches(&matches))
    {
        Ok(cli) => cli,
        Err(error) => return rejected(&error),
    };
    if cli.print_help {
        return Invocation::Answer(Answer::Print(printed(&concise_help(root()))));
    }
    if cli.print_version {
        return Invocation::Answer(Answer::Print(root().render_version()));
    }
    match cli.command {
        Some(Command::Help { command }) => help(command, terminal),
        None if !terminal => Invocation::Answer(Answer::Print(printed(&concise_help(root())))),
        None => Invocation::Open(Destination::Home),
        Some(Command::Init) => Invocation::Init,
        Some(Command::Config) => Invocation::Open(Destination::Config),
        Some(Command::Commit) => Invocation::Open(Destination::Commit),
    }
}

fn rejected(error: &clap::Error) -> Invocation {
    Invocation::Answer(Answer::Reject {
        message: error.to_string(),
        code: exit_code(error.exit_code()),
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

/// Clap leaves `[OPTIONS]` out of a usage whose only options are help and version, so this
/// overrides the root's usage with clap's own, the tag inserted after the command's name.
fn with_options_usage(mut root: clap::Command) -> clap::Command {
    root.build();
    let usage = root.render_usage().ansi().to_string();
    let styles = root.get_styles();
    let (title, literal, placeholder) = (
        styles.get_usage(),
        styles.get_literal(),
        styles.get_placeholder(),
    );
    let name = root.get_bin_name().unwrap_or_default();
    let prefix = format!("{title}Usage:{title:#} {literal}{name}{literal:#}");
    let tag = format!("{placeholder}[OPTIONS]{placeholder:#}");
    match usage.strip_prefix(&prefix) {
        Some(rest) => {
            let usage = format!("{literal}{name}{literal:#} {tag}{rest}");
            root.override_usage(StyledStr::from(usage))
        }
        None => root,
    }
}

/// The concise help `-h` and `--help` print, rendered by `root`.
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

/// Answers `help [<command>]`, its command one that exists, for a session whose stdin and stdout
/// are both `terminal`s or not.
fn help(command: Option<String>, terminal: bool) -> Invocation {
    if terminal {
        Invocation::Open(Destination::Help { command })
    } else {
        Invocation::Answer(Answer::Print(printed(&detailed_help(
            root(),
            command.as_deref(),
        ))))
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
        print(printed(&detailed_help(root(), command)))
    }

    #[test]
    fn commands_select_a_destination_and_commit_does_not_author() {
        let concise = print(printed(&concise_help(root())));
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
        let concise = print(printed(&concise_help(root())));
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
    fn help_and_version_flags_are_usage_errors_before_or_after_a_command() {
        for command in [
            &["init"][..],
            &["config"],
            &["commit"],
            &["help"],
            &["help", "commit"],
        ] {
            for flag in ["-h", "--help", "-V", "--version"] {
                for arguments in [[&[flag], command].concat(), [command, &[flag]].concat()] {
                    for invocation in invocations(&arguments) {
                        let Invocation::Answer(Answer::Reject { code, .. }) = invocation else {
                            unreachable!("{arguments:?} is a usage error");
                        };
                        assert_eq!(code, ExitCode::from(2), "{arguments:?}");
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
            }
        }
    }

    #[test]
    fn usage_shows_options_once_with_or_without_a_real_option() {
        let mut root = with_options_usage(
            clap::Command::new("tool")
                .arg(
                    clap::Arg::new("flag")
                        .long("flag")
                        .action(clap::ArgAction::SetTrue),
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
            Some("Usage: tool sub [path]")
        );
    }
}
