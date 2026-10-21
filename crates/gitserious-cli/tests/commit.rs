mod support;

use std::error::Error;
use std::process::ExitCode;

use gitserious_app::fingerprint_taxonomy;
use gitserious_core::{CommitTypeId, built_in_commit_types, built_in_taxonomies};
use support::{AuthorOutcome, Harness, Project, initialized_state};

fn initialized() -> Result<Harness, Box<dyn Error>> {
    Ok(Harness::new(Project::with(initialized_state()?)))
}

#[test]
fn taxonomy_option_selects_a_locked_taxonomy_without_changing_policy() -> Result<(), Box<dyn Error>>
{
    let harness = initialized()?;
    let (exit, _, stderr) = harness.run(&[
        "gitserious",
        "commit",
        "--taxonomy",
        "research",
        "--type",
        "hypothesis",
    ]);
    assert_eq!(exit, ExitCode::SUCCESS, "{stderr}");
    let messages = harness.writer.messages.borrow();
    assert!(messages[0].starts_with("hypothesis(tui-editor)!: expose command\n"));
    assert!(messages[0].contains("prediction:\n"));
    assert!(messages[0].contains("Gitserious-Taxonomy: research@1\n"));
    assert_eq!(*harness.project.state.borrow(), Some(initialized_state()?));

    let invalid = initialized()?;
    let (exit, _, stderr) = invalid.run(&[
        "gitserious",
        "commit",
        "--taxonomy",
        "infra-ops",
        "--type",
        "feat",
    ]);
    let first_infra_type = built_in_taxonomies()[2].commit_types()[0].id().to_string();
    assert_eq!(exit, ExitCode::FAILURE);
    assert!(
        stderr.contains(&format!("choose one of: {first_infra_type}")),
        "{stderr}"
    );
    assert_eq!(invalid.author.calls.get(), 0);
    assert_eq!(invalid.writer.calls.get(), 0);

    let (exit, _, stderr) = invalid.run(&["gitserious", "commit", "--taxonomy", "global-only"]);
    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stderr.contains("global-only"), "{stderr}");
    assert!(
        stderr.contains("choose one of: conventional, research, infra-ops"),
        "{stderr}"
    );
    assert_eq!(invalid.author.calls.get(), 0);
    Ok(())
}

#[test]
fn type_option_is_a_preselection_and_forwards_exact_git_output() -> Result<(), Box<dyn Error>> {
    let harness = initialized()?;

    let (exit, stdout, stderr) = harness.run(&["gitserious", "commit", "--type", "feat"]);

    assert_eq!(exit, ExitCode::SUCCESS, "{stderr}");
    assert_eq!(stdout, "git summary\n");
    assert_eq!(stderr, "hook warning\n");
    assert_eq!(harness.author.calls.get(), 1);
    assert_eq!(
        harness.author.seen.borrow()[0].1,
        Some(CommitTypeId::new("feat")?)
    );
    assert_eq!(harness.writer.calls.get(), 1);
    let fingerprint = fingerprint_taxonomy(&built_in_taxonomies()[0]);
    assert_eq!(
        harness.writer.messages.borrow()[0],
        format!(
            "feat(tui-editor)!: expose command\n\nintent:\nauthored intent\n\ndecision:\nauthored decision\n\nBREAKING CHANGE: replace CLI contract\n\nGitserious-Taxonomy: conventional@1\nGitserious-Schema: {fingerprint}\n"
        )
    );
    Ok(())
}

#[test]
fn bare_commit_delegates_type_selection_to_the_author() -> Result<(), Box<dyn Error>> {
    let harness = initialized()?;

    let (exit, _, stderr) = harness.run(&["gitserious", "commit"]);

    assert_eq!(exit, ExitCode::SUCCESS, "{stderr}");
    let seen = harness.author.seen.borrow();
    assert_eq!(seen[0].1, None);
    assert_eq!(seen[0].0.len(), built_in_commit_types().len());
    assert_eq!(harness.writer.calls.get(), 1);
    Ok(())
}

#[test]
fn cancellation_keeps_the_failure_status_and_never_writes() -> Result<(), Box<dyn Error>> {
    let harness = initialized()?;
    harness.author.outcome.replace(AuthorOutcome::Cancelled);

    let (exit, stdout, stderr) = harness.run(&["gitserious", "commit"]);

    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert_eq!(stderr, "Commit cancelled.\n");
    assert_eq!(harness.writer.calls.get(), 0);
    Ok(())
}

#[test]
fn unavailable_type_is_rejected_before_authoring_in_policy_order() -> Result<(), Box<dyn Error>> {
    let harness = initialized()?;

    let (exit, stdout, stderr) = harness.run(&["gitserious", "commit", "--type", "custom"]);

    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert!(stderr.starts_with("error: commit type "), "{stderr}");
    assert!(stderr.contains("custom"));
    assert!(stderr.contains("choose one of: feat, fix"), "{stderr}");
    assert!(stderr.ends_with("revert\n"), "{stderr}");
    assert_eq!(harness.author.calls.get(), 0);
    assert_eq!(harness.writer.calls.get(), 0);
    Ok(())
}

#[test]
fn author_errors_are_presented_without_invoking_git() -> Result<(), Box<dyn Error>> {
    let harness = initialized()?;
    harness.author.outcome.replace(AuthorOutcome::Failed);

    let (exit, stdout, stderr) = harness.run(&["gitserious", "commit"]);

    assert_eq!(exit, ExitCode::FAILURE);
    assert!(stdout.is_empty());
    assert_eq!(stderr, "error: fake adapter failure\n");
    assert_eq!(harness.writer.calls.get(), 0);
    Ok(())
}
