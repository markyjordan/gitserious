mod support;

use std::cell::RefCell;
use std::path::Path;
use std::process::ExitCode;

use gitserious_app::HelpPage;
use gitserious_cli::{CommitAdapters, ShellLauncher, ShellStart};
use support::{Harness, Project, repository_path};

struct Opened(RefCell<Option<ShellStart>>);

impl ShellLauncher for Opened {
    fn open(&self, start: ShellStart, directory: &Path, help: &[HelpPage]) -> Result<(), String> {
        assert_eq!(directory, &repository_path().join("subdir"));
        assert!(
            help.iter()
                .any(|page| page.command.as_deref() == Some("commit"))
        );
        *self.0.borrow_mut() = Some(start);
        Ok(())
    }
}

#[test]
fn commit_opens_the_shell_on_the_commit_page() {
    let harness = Harness::new(Project::default());
    let opened = Opened(RefCell::new(None));

    let (exit, stdout, stderr) = harness.run_with(
        &["gitserious", "commit"],
        &CommitAdapters::new().with_shell(&opened),
    );

    assert_eq!(exit, ExitCode::SUCCESS, "{stderr}");
    assert!(stdout.is_empty());
    assert_eq!(opened.0.borrow().clone(), Some(ShellStart::Commit));
}

#[test]
fn commit_without_a_shell_fails_instead_of_authoring() {
    let harness = Harness::new(Project::default());

    let (exit, stdout, stderr) = harness.run(&["gitserious", "commit"]);

    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert_eq!(stderr, "error: interactive shell is not configured\n");
}

#[test]
fn config_opens_the_shell_on_the_config_page() {
    let harness = Harness::new(Project::default());
    let opened = Opened(RefCell::new(None));

    let (exit, _, stderr) = harness.run_with(
        &["gitserious", "config"],
        &CommitAdapters::new().with_shell(&opened),
    );

    assert_eq!(exit, ExitCode::SUCCESS, "{stderr}");
    assert_eq!(opened.0.borrow().clone(), Some(ShellStart::Config));
}

struct Rejected(RefCell<Vec<ShellStart>>);

impl ShellLauncher for Rejected {
    fn open(&self, start: ShellStart, directory: &Path, help: &[HelpPage]) -> Result<(), String> {
        assert_eq!(directory, &repository_path().join("subdir"));
        assert!(!help.is_empty());
        self.0.borrow_mut().push(start);
        Err("shell launch rejected".into())
    }
}

#[test]
fn shell_launch_failures_report_stderr_without_falling_back_to_another_command() {
    for (command, page) in [
        ("commit", ShellStart::Commit),
        ("config", ShellStart::Config),
    ] {
        let mut harness = Harness::new(Project::default());
        harness.locator.fail = true;
        harness.global.fail = true;
        let rejected = Rejected(RefCell::new(Vec::new()));

        let (exit, stdout, stderr) = harness.run_with(
            &["gitserious", command],
            &CommitAdapters::new().with_shell(&rejected),
        );

        assert_eq!(exit, ExitCode::FAILURE);
        assert!(stdout.is_empty());
        assert_eq!(stderr, "error: shell launch rejected\n");
        assert_eq!(*rejected.0.borrow(), [page]);
        assert!(harness.project.initialized.borrow().is_none());
    }
}
