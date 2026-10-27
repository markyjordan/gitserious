mod support;

use std::process::ExitCode;

use gitserious_core::{TaxonomyId, built_in_taxonomies};
use support::{Harness, Locator, Project, repository_path};

#[test]
fn init_dispatches_and_reports_the_exact_resolution() -> Result<(), Box<dyn std::error::Error>> {
    let harness = Harness::new(Project::default());

    let (exit, stdout, stderr) = harness.run(&["gitserious", "init"]);

    assert_eq!(exit, ExitCode::SUCCESS);
    assert_eq!(
        stdout,
        format!(
            "Initialized gitserious in {} (conventional@2).\n",
            repository_path().display()
        )
    );
    assert!(stderr.is_empty());
    let initialized = harness.project.initialized.borrow();
    let (config, lock) = initialized.as_ref().ok_or("init did not write")?;
    assert_eq!(config.default_taxonomy(), None);
    assert_eq!(lock.taxonomies(), built_in_taxonomies());
    Ok(())
}

#[test]
fn init_reports_an_unreadable_global_configuration_without_writing() {
    let mut harness = Harness::new(Project::default());
    harness.global.fail = true;

    let (exit, stdout, stderr) = harness.run(&["gitserious", "init"]);

    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert_eq!(stderr, "error: global configuration unavailable\n");
    assert!(harness.project.initialized.borrow().is_none());
}

#[test]
fn operational_failures_use_stderr_and_exit_one() {
    let mut harness = Harness::new(Project::default());
    harness.locator = Locator { fail: true };

    let (exit, stdout, stderr) = harness.run(&["gitserious", "init"]);

    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert_eq!(stderr, "error: repository unavailable\n");
    assert!(harness.project.initialized.borrow().is_none());
}

#[test]
fn init_selects_an_explicit_installed_taxonomy() -> Result<(), Box<dyn std::error::Error>> {
    let harness = Harness::new(Project::default());

    let (exit, stdout, stderr) = harness.run(&["gitserious", "init", "--taxonomy", "research"]);

    assert_eq!(exit, ExitCode::SUCCESS, "{stderr}");
    assert!(stdout.ends_with(" (research@2).\n"), "{stdout}");
    let initialized = harness.project.initialized.borrow();
    let (config, lock) = initialized.as_ref().ok_or("init did not write")?;
    assert_eq!(
        config.default_taxonomy(),
        Some(&TaxonomyId::new("research")?)
    );
    assert_eq!(lock.default_taxonomy().as_str(), "research");
    Ok(())
}

#[test]
fn init_rejects_an_uninstalled_taxonomy() {
    let harness = Harness::new(Project::default());

    let (exit, stdout, stderr) = harness.run(&["gitserious", "init", "--taxonomy", "missing"]);

    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert_eq!(stderr, "error: taxonomy \"missing\" is not installed\n");
    assert!(harness.project.initialized.borrow().is_none());
}

#[test]
fn help_and_version_succeed_on_stdout_without_touching_adapters() {
    for (arguments, expected) in [
        (
            &["gitserious", "--help"][..],
            "Create durable commit-message policy",
        ),
        (&["gitserious", "--version"][..], "gitserious 0.1.0"),
    ] {
        let mut harness = Harness::new(Project::default());
        harness.locator = Locator { fail: true };
        harness.global.fail = true;

        let (exit, stdout, stderr) = harness.run(arguments);

        assert_eq!(exit, ExitCode::SUCCESS);
        assert!(stdout.contains(expected), "{stdout}");
        assert!(stderr.is_empty());
        assert!(harness.project.initialized.borrow().is_none());
    }
}

#[test]
fn unknown_commands_and_extra_arguments_are_usage_errors() {
    for arguments in [
        &["gitserious", "unknown"][..],
        &["gitserious", "init", "extra"][..],
    ] {
        let harness = Harness::new(Project::default());

        let (exit, stdout, stderr) = harness.run(arguments);

        assert_eq!(exit, ExitCode::from(2), "{arguments:?}");
        assert!(stdout.is_empty());
        assert!(stderr.contains("Usage:"), "{stderr}");
        assert!(harness.project.initialized.borrow().is_none());
    }
}
