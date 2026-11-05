use std::error::Error;
use std::process::{Command, Output};

const DOCUMENTED: [&str; 3] = ["init", "config", "commit"];
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
    for command in [
        None,
        Some("init"),
        Some("config"),
        Some("commit"),
        Some("help"),
    ] {
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
fn command_help_opens_with_the_description_then_usage_and_options() -> Result<(), Box<dyn Error>> {
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
        assert!(sections.starts_with(&format!("gitserious {command}\n\nOptions:\n")));
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
