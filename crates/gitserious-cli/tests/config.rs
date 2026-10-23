mod support;

use std::error::Error;
use std::process::ExitCode;

use gitserious_app::GlobalConfiguration;
use gitserious_core::{TaxonomyId, built_in_taxonomies};
use support::{Global, Harness, Project};

fn with_team() -> Result<GlobalConfiguration, Box<dyn Error>> {
    let team = TaxonomyId::new("team")?;
    let mut available = GlobalConfiguration::default()
        .available_taxonomies()
        .to_vec();
    available.push(team.clone());
    Ok(GlobalConfiguration::new(
        1,
        team.clone(),
        available,
        vec![built_in_taxonomies()[0].fork(team)],
    )?)
}

#[test]
fn list_reports_each_installed_taxonomy_with_origin_and_selection() {
    let harness = Harness::new(Project::default());

    let (exit, stdout, stderr) = harness.run(&["gitserious", "config", "list"]);

    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(stderr.is_empty());
    assert_eq!(
        stdout,
        "conventional\tbuilt-in\tv1 default available\n\
         research\tbuilt-in\tv1 available\n\
         infra-ops\tbuilt-in\tv1 available\n"
    );
}

#[test]
fn show_renders_a_taxonomy_in_detail() {
    let harness = Harness::new(Project::default());
    let conventional = &built_in_taxonomies()[0];

    let (exit, stdout, stderr) = harness.run(&["gitserious", "config", "show", "conventional"]);

    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(stderr.is_empty());
    assert!(stdout.starts_with(&format!(
        "conventional\nv1\n{}\n\nfeat\t",
        conventional.description()
    )));
    assert!(
        stdout.contains("\n  intent\tRequired\tSingle\n"),
        "{stdout}"
    );
    assert_eq!(
        stdout.matches("\n\n").count(),
        conventional.commit_types().len()
    );
    assert!(!stdout.contains("forked from"));
}

#[test]
fn show_missing_taxonomy_fails_on_stderr() {
    let harness = Harness::new(Project::default());

    let (exit, stdout, stderr) = harness.run(&["gitserious", "config", "show", "missing"]);

    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert_eq!(stderr, "error: taxonomy \"missing\" is not installed\n");
}

#[test]
fn list_and_show_include_user_forks() -> Result<(), Box<dyn Error>> {
    let mut harness = Harness::new(Project::default());
    harness.global = Global::new(with_team()?);

    let (exit, listing, stderr) = harness.run(&["gitserious", "config", "list"]);
    assert_eq!(exit, ExitCode::SUCCESS, "{stderr}");
    assert!(listing.starts_with("conventional\tbuilt-in\tv1 available\n"));
    assert!(
        listing.ends_with("team\tcustom\tv1 default available\n"),
        "{listing}"
    );

    let (exit, shown, stderr) = harness.run(&["gitserious", "config", "show", "team"]);
    assert_eq!(exit, ExitCode::SUCCESS, "{stderr}");
    assert!(shown.starts_with("team\nv1\n"));
    assert!(shown.contains("\nforked from conventional@1\n"), "{shown}");
    Ok(())
}

#[test]
fn domain_taxonomies_are_listed_and_inspectable() {
    for taxonomy in &built_in_taxonomies()[1..] {
        let identity = taxonomy.id().as_str();
        let first_type = taxonomy.commit_types()[0].id();
        let harness = Harness::new(Project::default());

        let (exit, listing, stderr) = harness.run(&["gitserious", "config", "list"]);
        assert_eq!(exit, ExitCode::SUCCESS);
        assert!(stderr.is_empty());
        assert!(listing.contains(&format!("{identity}\tbuilt-in\tv1")));

        let (exit, shown, stderr) = harness.run(&["gitserious", "config", "show", identity]);
        assert_eq!(exit, ExitCode::SUCCESS);
        assert!(stderr.is_empty());
        assert!(shown.starts_with(&format!("{identity}\nv1\n")));
        assert!(shown.contains(&format!("\n\n{first_type}\t")), "{shown}");
    }
}

#[test]
fn config_reports_an_unreadable_global_configuration() {
    for arguments in [
        &["gitserious", "config", "list"][..],
        &["gitserious", "config", "show", "conventional"][..],
    ] {
        let mut harness = Harness::new(Project::default());
        harness.global.fail = true;

        let (exit, stdout, stderr) = harness.run(arguments);

        assert_eq!(exit, ExitCode::FAILURE);
        assert!(stdout.is_empty());
        assert_eq!(stderr, "error: global configuration unavailable\n");
    }
}
