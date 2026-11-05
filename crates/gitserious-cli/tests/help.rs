mod support;

use std::process::ExitCode;

use support::{Harness, Project};

fn succeed(arguments: &[&str]) -> String {
    let (exit, stdout, stderr) = Harness::new(Project::default()).run(arguments);
    assert_eq!(exit, ExitCode::SUCCESS, "{arguments:?}: {stderr}");
    assert!(stderr.is_empty(), "{arguments:?}: {stderr}");
    stdout
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
    assert_eq!(pages[0].text, succeed(&["gitserious", "--help"]));
    for (page, command) in pages[1..4].iter().zip(["init", "config", "commit"]) {
        assert_eq!(
            page.text,
            succeed(&["gitserious", "help", "--no-pager", command]),
            "{command}"
        );
    }
}
