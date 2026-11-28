/// The pages the executable gives the TUI, from the executable's own source.
#[path = "../src/help.rs"]
mod help;
mod support;

use std::error::Error;
use std::process::Output;

use gitserious_tui::HelpPage;
use help::help_pages;
use support::Home;

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
    let home = Home::new()?;
    Ok(home
        .command(home.path())
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

fn concise() -> Result<String, Box<dyn Error>> {
    succeed_at(COLUMNS, &["-h"])
}

fn long_flag() -> Result<String, Box<dyn Error>> {
    succeed_at(COLUMNS, &["--help"])
}

fn detailed(command: Option<&str>) -> Result<String, Box<dyn Error>> {
    succeed_at(COLUMNS, &[&["help"][..], command.as_slice()].concat())
}

/// A command, `None` for the root, and one help it prints.
type Printed = (Option<&'static str>, String);

/// What `-h` and `--help` print, then what `help` prints for every command.
fn every_help() -> Result<Vec<Printed>, Box<dyn Error>> {
    let mut helps = vec![(None, concise()?), (None, long_flag()?)];
    for command in COMMANDS {
        helps.push((command, detailed(command)?));
    }
    Ok(helps)
}

/// The flags listed under `heading` in `help`.
fn flags(help: &str, heading: &str) -> Vec<String> {
    sections(help)
        .into_iter()
        .filter(|(name, _)| *name == heading)
        .flat_map(|(_, body)| body)
        .filter(|line| line.trim_start().starts_with('-'))
        .filter_map(|line| line.trim().split("  ").next())
        .map(str::to_owned)
        .collect()
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
fn both_help_flags_print_the_concise_help_listing_the_root_options() -> Result<(), Box<dyn Error>> {
    let help = concise()?;
    let entries: Vec<_> = help.lines().map(str::trim).collect();

    assert_eq!(long_flag()?, help);
    assert_ne!(help, detailed(None)?);
    for (flag, description) in [
        ("-h, --help", "Print the concise help"),
        ("-V, --version", "Print the version"),
    ] {
        assert!(
            entries
                .iter()
                .any(|line| line.starts_with(&format!("{flag} "))
                    && line.ends_with(&format!(" {description}"))),
            "{help}"
        );
    }
    Ok(())
}

#[test]
fn root_flags_with_a_command_are_usage_errors_that_name_the_way_out() -> Result<(), Box<dyn Error>>
{
    for (command, documented) in [
        (&["init"][..], "init"),
        (&["config"], "config"),
        (&["commit"], "commit"),
        (&["help"], "help"),
        (&["help", "commit"], "commit"),
    ] {
        let help =
            format!("Use `gitserious help {documented}` for command-specific documentation.");
        for (flag, tip) in [
            ("-h", help.as_str()),
            ("--help", &help),
            (
                "-V",
                "Use `gitserious --version` without a command to print the version.",
            ),
            (
                "--version",
                "Use `gitserious --version` without a command to print the version.",
            ),
        ] {
            for arguments in [[&[flag], command].concat(), [command, &[flag]].concat()] {
                let output = run(COLUMNS, &arguments)?;
                let stderr = String::from_utf8(output.stderr)?;

                assert_eq!(output.status.code(), Some(2), "{arguments:?}");
                assert!(output.stdout.is_empty(), "{arguments:?}");
                assert!(stderr.starts_with("error: "), "{stderr}");
                assert!(stderr.ends_with(&format!("\n\n{tip}\n")), "{stderr}");
            }
        }
    }
    Ok(())
}

#[test]
fn the_help_command_takes_no_pager_option() -> Result<(), Box<dyn Error>> {
    let output = run(COLUMNS, &["help", "--no-pager"])?;

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)?.contains("'--no-pager'"),
        "usage error"
    );
    Ok(())
}

#[test]
fn every_usage_shows_what_its_command_takes() -> Result<(), Box<dyn Error>> {
    for (command, help) in every_help()? {
        let expected = match command {
            None => "Usage: gitserious [COMMAND]\n       gitserious [OPTIONS]".to_owned(),
            Some("help") => "Usage: gitserious help [COMMAND]".to_owned(),
            Some(command) => format!("Usage: gitserious {command}"),
        };
        let usage = help
            .split_once("Usage: ")
            .and_then(|(_, rest)| rest.split("\n\n").next())
            .map(|usage| format!("Usage: {}", usage.trim_end()));

        assert_eq!(usage, Some(expected), "{help}");
    }
    Ok(())
}

#[test]
fn only_the_root_lists_options() -> Result<(), Box<dyn Error>> {
    for (command, help) in every_help()? {
        let expected: &[&str] = match command {
            None => &["-h, --help", "-V, --version"],
            Some(_) => &[],
        };

        assert_eq!(flags(&help, "Options:"), expected, "{help}");
        assert!(!help.contains("Global options:"), "{help}");
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
fn root_concise_help_points_to_the_help_command() -> Result<(), Box<dyn Error>> {
    let help = concise()?;

    assert!(
        help.ends_with("\n\nUse `gitserious help` for more details.\n"),
        "{help}"
    );
    let rail: Vec<String> = help_pages()
        .into_iter()
        .filter_map(|page| page.command)
        .collect();
    assert_eq!(
        command_names(&help),
        rail,
        "commands list in Help's rail order"
    );
    assert!(
        command_lines(&help).all(|line| !line.ends_with('.')),
        "{help}"
    );
    Ok(())
}

#[test]
fn only_the_concise_help_points_to_the_help_command() -> Result<(), Box<dyn Error>> {
    let pointer = "Use `gitserious help` for more details.";
    let overview = help_pages()
        .into_iter()
        .find(|page| page.command.is_none())
        .ok_or("no Overview page")?;

    assert!(concise()?.contains(pointer));
    for help in [detailed(None)?, page_text(&overview, 80)] {
        assert!(!help.contains(pointer), "{help}");
    }
    Ok(())
}

#[test]
fn the_help_command_prints_the_concise_help_pointing_to_command_help() -> Result<(), Box<dyn Error>>
{
    let help = detailed(None)?;

    assert!(
        help.ends_with(
            "\n\nUse `gitserious help <COMMAND>` for more information on a specific command.\n"
        ),
        "{help}"
    );
    assert_eq!(
        help,
        concise()?.replace(
            "Use `gitserious help` for more details.",
            "Use `gitserious help <COMMAND>` for more information on a specific command."
        )
    );
    Ok(())
}

#[test]
fn command_help_is_the_description_then_the_usage() -> Result<(), Box<dyn Error>> {
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
        assert_eq!(
            sections,
            format!("gitserious {command}\n"),
            "{command}: {help}"
        );
        assert!(!help.contains("gitserious help"), "{command}: {help}");
        for heading in ["Examples", "NAME", "SYNOPSIS", "DESCRIPTION", "EXIT STATUS"] {
            assert!(!help.contains(heading), "{command}: {help}");
        }
    }
    Ok(())
}

#[test]
fn command_help_puts_each_description_below_its_name() -> Result<(), Box<dyn Error>> {
    let help = detailed(Some("help"))?;

    assert!(
        help.ends_with("\nArguments:\n  [COMMAND]\n          The command to document\n"),
        "{help}"
    );
    Ok(())
}

#[test]
fn help_wraps_each_paragraph_to_the_terminal_width_up_to_100_columns() -> Result<(), Box<dyn Error>>
{
    let paragraph = "Commit opens the type picker when changes are staged, and the Commit \
                     page otherwise. You choose a type, write its properties, and review \
                     the exact message before gitserious passes it to `git commit`.";
    let arguments = ["help", "commit"];
    let wide = succeed_at(100, &arguments)?;

    assert_eq!(succeed_at(200, &arguments)?, wide);
    for (columns, help) in [(60, succeed_at(60, &arguments)?), (100, wide)] {
        let lines: Vec<_> = help
            .lines()
            .skip_while(|line| !line.starts_with("Commit opens"))
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
    let output = run(COLUMNS, &["help", "nope"])?;
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
    let pages = help_pages();
    let shown: Vec<_> = pages
        .iter()
        .map(|page| (page.title.as_str(), page.command.as_deref()))
        .collect();

    assert_eq!(
        shown,
        [
            ("Overview", None),
            ("Commit", Some("commit")),
            ("Config", Some("config")),
            ("Help", Some("help")),
            ("Init", Some("init")),
        ]
    );
    for page in &pages {
        let arguments = [&["help"][..], page.command.as_deref().as_slice()].concat();
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
    for page in help_pages() {
        let spans = page.lines(80).concat();
        let strong: Vec<_> = spans
            .iter()
            .filter(|span| span.strong)
            .map(|span| span.text.as_str())
            .collect();
        let names: Vec<String> = match page.command.as_deref() {
            None => [
                "Usage:",
                "Commands:",
                "init",
                "Options:",
                "-h",
                "--help",
                "-V",
                "--version",
            ]
            .map(str::to_owned)
            .to_vec(),
            Some(command) => vec!["Usage:".to_owned(), format!("gitserious {command}")],
        };

        for name in &names {
            assert!(
                strong.contains(&name.as_str()),
                "{}: {strong:?}",
                page.title
            );
        }
        if page.command.is_none() {
            assert!(
                spans
                    .iter()
                    .any(|span| !span.strong && span.text.trim() == "Print the concise help"),
                "{}: {spans:?}",
                page.title
            );
        }
    }
}
