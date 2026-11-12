use std::process::ExitCode;

use gitserious_cli::{Destination, Invocation, parse, write_answer};

fn invocations(arguments: &[&str]) -> [Invocation; 2] {
    let arguments = || std::iter::once("gitserious").chain(arguments.iter().copied());
    [parse(arguments(), true), parse(arguments(), false)]
}

/// Writes what a redirected session answers for `arguments`.
fn answered(arguments: &[&str]) -> (ExitCode, String, String) {
    let [_, Invocation::Answer(answer)] = invocations(arguments) else {
        unreachable!("{arguments:?} is answered by the CLI");
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit = write_answer(answer, &mut stdout, &mut stderr);
    (
        exit,
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    )
}

#[test]
fn init_initializes_whatever_the_terminal() {
    assert_eq!(invocations(&["init"]), [Invocation::Init, Invocation::Init]);
}

#[test]
fn commit_and_config_open_the_shell_whatever_the_terminal() {
    for (command, page) in [
        ("commit", Destination::Commit),
        ("config", Destination::Config),
    ] {
        assert_eq!(
            invocations(&[command]),
            [Invocation::Open(page.clone()), Invocation::Open(page)],
            "a missing terminal is the shell's concern, not a second author"
        );
    }
}

#[test]
fn help_and_version_succeed_on_stdout() {
    for (arguments, expected) in [
        (&["--help"][..], "Create durable commit-message policy"),
        (
            &["--version"][..],
            concat!("gitserious ", env!("CARGO_PKG_VERSION")),
        ),
    ] {
        let (exit, stdout, stderr) = answered(arguments);

        assert_eq!(exit, ExitCode::SUCCESS);
        assert!(stdout.contains(expected), "{stdout}");
        assert!(stderr.is_empty());
    }
}

#[test]
fn unknown_commands_and_extra_arguments_are_usage_errors() {
    for arguments in [
        &["unknown"][..],
        &["init", "extra"],
        &["init", "--taxonomy", "research"],
        &["commit", "--taxonomy", "research"],
        &["commit", "--type", "feat"],
    ] {
        let (exit, stdout, stderr) = answered(arguments);

        assert_eq!(exit, ExitCode::from(2), "{arguments:?}");
        assert!(stdout.is_empty());
        assert!(stderr.contains("Usage:"), "{stderr}");
    }
}

#[test]
fn legacy_template_flags_and_commands_are_rejected() {
    let (exit, _, stderr) = answered(&["commit", "--template", "default"]);
    assert_eq!(exit, ExitCode::from(2));
    assert!(
        stderr.contains("unexpected argument '--template'"),
        "{stderr}"
    );

    for subcommand in ["fork", "list", "show", "verify"] {
        let (exit, _, stderr) = answered(&["config", subcommand]);
        assert_eq!(exit, ExitCode::from(2));
        assert!(
            stderr.contains(&format!("unexpected argument '{subcommand}'")),
            "{stderr}"
        );
    }
}
