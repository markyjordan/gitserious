mod support;

use std::process::ExitCode;

use support::{Harness, Project};

const DOCUMENTED: [&str; 3] = ["init", "config", "commit"];

fn succeed(arguments: &[&str]) -> String {
    let (exit, stdout, stderr) = Harness::new(Project::default()).run(arguments);
    assert_eq!(exit, ExitCode::SUCCESS, "{arguments:?}: {stderr}");
    assert!(stderr.is_empty(), "{arguments:?}: {stderr}");
    stdout
}

fn concise(command: Option<&str>, flag: &str) -> String {
    succeed(&[&["gitserious"][..], command.as_slice(), &[flag]].concat())
}

fn detailed(command: Option<&str>) -> String {
    succeed(
        &[
            &["gitserious", "help", "--no-pager"][..],
            command.as_slice(),
        ]
        .concat(),
    )
}

fn command_names(help: &str) -> Vec<&str> {
    help.split("Commands:\n")
        .nth(1)
        .unwrap_or_default()
        .lines()
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .collect()
}

#[test]
fn short_and_long_help_flags_print_the_same_concise_help() {
    for command in [
        None,
        Some("init"),
        Some("config"),
        Some("commit"),
        Some("help"),
    ] {
        let short = concise(command, "-h");

        assert_eq!(short, concise(command, "--help"), "{command:?}");
        assert!(
            short.lines().any(|line| {
                let line = line.trim();
                line.starts_with("-h, --help ")
                    && line.ends_with(" Display the concise help for this command")
            }),
            "{short}"
        );
    }
}

#[test]
fn long_help_flag_does_not_print_the_detailed_help() {
    assert_ne!(concise(None, "--help"), detailed(None));
    for command in DOCUMENTED {
        let help = concise(Some(command), "--help");

        assert_ne!(help, detailed(Some(command)), "{command}");
        assert!(!help.contains("gitserious help"), "{help}");
    }
}

#[test]
fn root_concise_help_points_to_the_help_command() {
    let help = concise(None, "--help");

    assert!(
        help.ends_with("\n\nUse `gitserious help` for more details.\n"),
        "{help}"
    );
    assert_eq!(command_names(&help), ["init", "config", "commit", "help"]);
}

#[test]
fn root_detailed_help_lists_the_help_command_and_points_to_command_help() {
    let help = detailed(None);

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
}

#[test]
fn command_help_opens_with_the_description_then_usage_and_options() {
    for command in DOCUMENTED {
        let help = detailed(Some(command));
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
}

#[test]
fn command_help_puts_each_description_below_its_flag_with_blank_lines_between() {
    let help = detailed(Some("help"));
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
}

#[test]
fn help_for_an_unknown_command_is_a_usage_error_naming_the_valid_ones() {
    let (exit, stdout, stderr) =
        Harness::new(Project::default()).run(&["gitserious", "help", "--no-pager", "nope"]);

    assert_eq!(exit, ExitCode::from(2));
    assert!(stdout.is_empty());
    assert!(stderr.contains("'nope'"), "{stderr}");
    for command in DOCUMENTED {
        assert!(stderr.contains(command), "{stderr}");
    }
}

#[test]
fn interactive_help_pages_are_the_cli_help() {
    let pages = gitserious_cli::interactive_help();
    let titles: Vec<_> = pages.iter().map(|page| page.title.as_str()).collect();

    assert_eq!(
        titles,
        [
            "Overview",
            "Init",
            "Config",
            "Commit",
            "Keyboard Navigation"
        ]
    );
    assert_eq!(pages[0].text, concise(None, "--help"));
    for (page, command) in pages[1..4].iter().zip(DOCUMENTED) {
        assert_eq!(page.text, detailed(Some(command)), "{command}");
    }
}
