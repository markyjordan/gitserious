mod support;

use std::error::Error;
use std::process::ExitCode;

use gitserious_app::fingerprint_taxonomy;
use gitserious_core::{built_in_commit_types, built_in_taxonomies};
use support::{AuthorOutcome, Harness, Project, initialized_state};

fn initialized() -> Result<Harness, Box<dyn Error>> {
    Ok(Harness::new(Project::with(initialized_state()?)))
}

#[test]
fn commit_forwards_exact_git_output_and_reviewed_bytes() -> Result<(), Box<dyn Error>> {
    let harness = initialized()?;

    let (exit, stdout, stderr) = harness.run(&["gitserious", "commit"]);

    assert_eq!(exit, ExitCode::SUCCESS, "{stderr}");
    assert_eq!(stdout, "git summary\n");
    assert_eq!(stderr, "hook warning\n");
    assert_eq!(harness.author.calls.get(), 1);
    assert_eq!(harness.writer.calls.get(), 1);
    let fingerprint = fingerprint_taxonomy(&built_in_taxonomies()[0]);
    assert_eq!(
        harness.writer.messages.borrow()[0],
        format!(
            "feat(tui-editor)!: expose command\n\nintent:\nauthored intent\n\ndecision:\nauthored decision\n\nBREAKING CHANGE: replace CLI contract\n\nGitserious-Taxonomy: conventional-v1\nGitserious-Schema: {fingerprint}\n"
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
