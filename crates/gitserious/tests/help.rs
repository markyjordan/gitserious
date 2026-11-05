use std::error::Error;
use std::process::{Command, Output};

use gitserious_app::{HelpPage, HelpSpan};
use gitserious_cli::interactive_help;

const DOCUMENTED: [&str; 3] = ["init", "config", "commit"];
const COMMANDS: [Option<&str>; 5] = [
    None,
    Some("init"),
    Some("config"),
    Some("commit"),
    Some("help"),
];
const COLUMNS: usize = 100;

/// Every stream of the child is a pipe, so clap takes the terminal width from `COLUMNS`.
fn run(columns: usize, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
    Ok(Command::new(env!("CARGO_BIN_EXE_gitserious"))
        .args(arguments)
        .env("COLUMNS", columns.to_string())
        .output()?)
}

fn succeed_at(columns: usize, arguments: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = run(columns, arguments)?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() || !stderr.is_empty() {
        return Err(format!("{arguments:?} failed: {stderr}").into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn concise(command: Option<&str>, flag: &str) -> Result<String, Box<dyn Error>> {
    succeed_at(COLUMNS, &[command.as_slice(), &[flag]].concat())
}

fn detailed(command: Option<&str>) -> Result<String, Box<dyn Error>> {
    succeed_at(
        COLUMNS,
        &[&["help", "--no-pager"][..], command.as_slice()].concat(),
    )
}

/// A command, `None` for the root, and one help it prints.
type Printed = (Option<&'static str>, String);

/// The concise help both flags print and the detailed help, for every command.
fn every_help() -> Result<Vec<Printed>, Box<dyn Error>> {
    let mut helps = Vec::new();
    for command in COMMANDS {
        for flag in ["-h", "--help"] {
            helps.push((command, concise(command, flag)?));
        }
        helps.push((command, detailed(command)?));
    }
    Ok(helps)
}

/// Each heading with the lines below it, up to the next line at column 0.
fn sections(help: &str) -> Vec<(&str, Vec<&str>)> {
    let mut sections: Vec<(&str, Vec<&str>)> = Vec::new();
    let mut open = false;
    for line in help.lines() {
        if line.ends_with(':') && !line.starts_with(' ') {
            sections.push((line, Vec::new()));
            open = true;
        } else if !line.is_empty() && !line.starts_with(' ') {
            open = false;
        } else if let Some((_, body)) = sections.last_mut().filter(|_| open) {
            body.push(line);
        }
    }
    sections
}

fn command_lines(help: &str) -> impl Iterator<Item = &str> {
    help.split("Commands:\n")
        .nth(1)
        .unwrap_or_default()
        .lines()
        .take_while(|line| !line.is_empty())
}

fn command_names(help: &str) -> Vec<&str> {
    command_lines(help)
        .filter_map(|line| line.split_whitespace().next())
        .collect()
}

#[test]
fn short_and_long_help_flags_print_the_same_concise_help() -> Result<(), Box<dyn Error>> {
    for command in COMMANDS {
        let short = concise(command, "-h")?;

        assert_eq!(short, concise(command, "--help")?, "{command:?}");
        assert!(
            short.lines().any(|line| {
                let line = line.trim();
                line.starts_with("-h, --help ")
                    && line.ends_with(" Display the concise help for this command")
            }),
            "{short}"
        );
    }
    Ok(())
}

#[test]
fn every_usage_line_shows_options() -> Result<(), Box<dyn Error>> {
    for (command, help) in every_help()? {
        let expected = match command {
            None => "Usage: gitserious [OPTIONS] [COMMAND]".to_owned(),
            Some("help") => "Usage: gitserious help [OPTIONS] [COMMAND]".to_owned(),
            Some(command) => format!("Usage: gitserious {command} [OPTIONS]"),
        };
        let usage: Vec<_> = help
            .lines()
            .filter(|line| line.starts_with("Usage:"))
            .collect();

        assert_eq!(usage, [expected], "{help}");
    }
    Ok(())
}

#[test]
fn help_flag_is_listed_only_under_global_options() -> Result<(), Box<dyn Error>> {
    for (command, help) in every_help()? {
        let flags = |heading: &str| -> Vec<String> {
            sections(&help)
                .into_iter()
                .filter(|(name, _)| *name == heading)
                .flat_map(|(_, body)| body)
                .filter(|line| line.trim_start().starts_with('-'))
                .filter_map(|line| line.trim().split("  ").next())
                .map(str::to_owned)
                .collect()
        };

        assert_eq!(flags("Global options:"), ["-h, --help"], "{help}");
        let options = flags("Options:");
        assert!(
            !options
                .iter()
                .flat_map(|flag| flag.split(", "))
                .any(|name| name == "-h" || name == "--help"),
            "{help}"
        );
        if command.is_none() {
            assert_eq!(options, ["-V, --version"], "{help}");
        }
    }
    Ok(())
}

#[test]
fn every_heading_lists_something() -> Result<(), Box<dyn Error>> {
    for (_, help) in every_help()? {
        for (heading, body) in sections(&help) {
            assert!(
                body.first().is_some_and(|line| !line.trim().is_empty()),
                "{heading:?} is empty: {help}"
            );
        }
    }
    Ok(())
}

#[test]
fn long_help_flag_does_not_print_the_detailed_help() -> Result<(), Box<dyn Error>> {
    assert_ne!(concise(None, "--help")?, detailed(None)?);
    for command in DOCUMENTED {
        let help = concise(Some(command), "--help")?;

        assert_ne!(help, detailed(Some(command))?, "{command}");
        assert!(!help.contains("gitserious help"), "{help}");
    }
    Ok(())
}

#[test]
fn root_concise_help_points_to_the_help_command() -> Result<(), Box<dyn Error>> {
    let help = concise(None, "--help")?;

    assert!(
        help.ends_with("\n\nUse `gitserious help` for more details.\n"),
        "{help}"
    );
    assert_eq!(command_names(&help), ["init", "config", "commit", "help"]);
    assert!(
        command_lines(&help).all(|line| !line.ends_with('.')),
        "{help}"
    );
    Ok(())
}

#[test]
fn root_detailed_help_lists_the_help_command_and_points_to_command_help()
-> Result<(), Box<dyn Error>> {
    let help = detailed(None)?;

    assert!(
        help.lines().any(|line| line.starts_with("  help ")
            && line.ends_with(" Display documentation for a command")),
        "{help}"
    );
    assert_eq!(command_names(&help), ["init", "config", "commit", "help"]);
    assert!(
        help.ends_with(
            "\n\nUse `gitserious help <command>` for more information on a specific command.\n"
        ),
        "{help}"
    );
    Ok(())
}

#[test]
fn command_help_opens_with_the_description_then_usage_and_global_options()
-> Result<(), Box<dyn Error>> {
    for command in DOCUMENTED {
        let help = detailed(Some(command))?;
        let (description, sections) = help.split_once("\n\nUsage: ").unwrap_or_default();

        assert!(description.contains("\n\n"), "{command}: {help}");
        assert!(
            description
                .lines()
                .all(|line| !line.ends_with(':') && line.trim() == line),
            "{command}: {help}"
        );
        assert!(
            sections.starts_with(&format!(
                "gitserious {command} [OPTIONS]\n\nGlobal options:\n"
            )),
            "{command}: {help}"
        );
        assert!(!help.contains("Arguments:"), "{command}: {help}");
        assert!(!help.contains("gitserious help"), "{command}: {help}");
        for heading in ["Examples", "NAME", "SYNOPSIS", "DESCRIPTION", "EXIT STATUS"] {
            assert!(!help.contains(heading), "{command}: {help}");
        }
    }
    Ok(())
}

#[test]
fn command_help_puts_each_description_below_its_flag_with_blank_lines_between()
-> Result<(), Box<dyn Error>> {
    let help = detailed(Some("help"))?;
    let options = help.split_once("\nOptions:\n").unwrap_or_default().1;

    assert!(
        help.contains("\nArguments:\n  [COMMAND]\n          "),
        "{help}"
    );
    let options = options.replace("\n\nGlobal options:\n", "\n\n");
    let entries: Vec<_> = options.trim_end().split("\n\n").collect();
    assert_eq!(entries.len(), 2, "{help}");
    for entry in entries {
        let lines: Vec<_> = entry.lines().collect();
        assert_eq!(lines.len(), 2, "{entry}");
        assert!(lines[0].trim_start().starts_with('-'), "{entry}");
        assert!(lines[1].starts_with("          "), "{entry}");
        assert!(!lines[1].trim().is_empty(), "{entry}");
    }
    Ok(())
}

#[test]
fn help_wraps_each_paragraph_to_the_terminal_width_up_to_100_columns() -> Result<(), Box<dyn Error>>
{
    let paragraph = "Commit starts from the project default taxonomy in `gitserious.toml`. \
                     You choose a type, write its properties, and review the exact message \
                     before gitserious passes it to `git commit`.";
    let arguments = ["help", "--no-pager", "commit"];
    let wide = succeed_at(100, &arguments)?;

    assert_eq!(succeed_at(200, &arguments)?, wide);
    for (columns, help) in [(60, succeed_at(60, &arguments)?), (100, wide)] {
        let lines: Vec<_> = help
            .lines()
            .skip_while(|line| !line.starts_with("Commit starts"))
            .take_while(|line| !line.is_empty())
            .collect();

        assert!(lines.len() > 1, "{columns}: {help}");
        assert_eq!(lines.join(" "), paragraph, "{columns}: {help}");
        assert!(
            help.lines().all(|line| line.chars().count() <= columns),
            "{columns}: {help}"
        );
    }
    Ok(())
}

#[test]
fn help_for_an_unknown_command_is_a_usage_error_naming_the_valid_ones() -> Result<(), Box<dyn Error>>
{
    let output = run(COLUMNS, &["help", "--no-pager", "nope"])?;
    let stderr = String::from_utf8(output.stderr)?;

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(stderr.contains("'nope'"), "{stderr}");
    for command in DOCUMENTED {
        assert!(stderr.contains(command), "{stderr}");
    }
    Ok(())
}

fn page_text(page: &HelpPage, width: u16) -> String {
    page.lines(width)
        .iter()
        .map(|line| {
            let mut text: String = line.iter().map(|span| span.text.as_str()).collect();
            text.push('\n');
            text
        })
        .collect()
}

#[test]
fn each_help_page_shows_what_its_help_command_prints_at_the_same_width()
-> Result<(), Box<dyn Error>> {
    let pages = interactive_help();
    let shown: Vec<_> = pages
        .iter()
        .map(|page| (page.title.as_str(), page.command.as_deref()))
        .collect();

    assert_eq!(
        shown,
        [
            ("Overview", None),
            ("Init", Some("init")),
            ("Config", Some("config")),
            ("Commit", Some("commit")),
        ]
    );
    for page in &pages {
        let arguments = match page.command.as_deref() {
            None => vec!["--help"],
            Some(command) => vec!["help", command],
        };
        for width in [50, 80, 120] {
            assert_eq!(
                page_text(page, width),
                succeed_at(usize::from(width), &arguments)?,
                "{} at {width}",
                page.title
            );
        }
    }
    Ok(())
}

#[test]
fn help_pages_mark_headings_and_names_strong_and_leave_descriptions_plain() {
    for page in interactive_help() {
        let spans: Vec<HelpSpan> = page.lines(80).concat();
        let strong: Vec<_> = spans
            .iter()
            .filter(|span| span.strong)
            .map(|span| span.text.as_str())
            .collect();

        for name in ["Usage:", "Global options:", "-h", "--help"] {
            assert!(strong.contains(&name), "{}: {strong:?}", page.title);
        }
        assert!(
            spans.iter().any(|span| !span.strong
                && span.text.trim() == "Display the concise help for this command"),
            "{}: {spans:?}",
            page.title
        );
    }
}
