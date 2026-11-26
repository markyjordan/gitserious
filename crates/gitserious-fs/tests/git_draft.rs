mod support;

use std::error::Error;

use gitserious_app::{DraftStore, RepositoryRoot, SavedDraft, SavedProperty};
use gitserious_fs::GitDraftStore;
use support::{git, repository};
use tempfile::TempDir;

fn draft(subject: &str) -> SavedDraft {
    SavedDraft {
        taxonomy: "conventional".into(),
        fingerprint: "sha256:00".into(),
        commit_type: "feat".into(),
        subject: subject.into(),
        properties: vec![
            SavedProperty {
                key: "intent".into(),
                text: "first line\nsecond line".into(),
                applies: None,
            },
            SavedProperty {
                key: "constraints".into(),
                text: String::new(),
                applies: Some(false),
            },
        ],
        breaking_change: String::new(),
    }
}

#[test]
fn a_draft_round_trips_stays_out_of_status_and_discards() -> Result<(), Box<dyn Error>> {
    let temp = repository()?;
    let root = RepositoryRoot::new(temp.path().to_path_buf())?;
    assert_eq!(GitDraftStore.load(&root)?, None);

    GitDraftStore.save(&root, &draft("(tui): keep drafts"))?;
    GitDraftStore.save(&root, &draft(": the latest wins"))?;

    assert_eq!(GitDraftStore.load(&root)?, Some(draft(": the latest wins")));
    let status = git(temp.path(), &["status", "--porcelain", "--ignored"])?;
    assert!(status.stdout.is_empty(), "the draft lives under .git");
    GitDraftStore.discard(&root)?;
    assert_eq!(GitDraftStore.load(&root)?, None);
    GitDraftStore.discard(&root)?;
    Ok(())
}

#[test]
fn each_linked_worktree_keeps_its_own_draft() -> Result<(), Box<dyn Error>> {
    let temp = repository()?;
    git(
        temp.path(),
        &["commit", "-q", "--allow-empty", "-m", "base"],
    )?;
    let linked_parent = TempDir::new()?;
    let linked = linked_parent.path().join("linked");
    git(
        temp.path(),
        &["worktree", "add", "-q", &linked.to_string_lossy()],
    )?;
    let main = RepositoryRoot::new(temp.path().to_path_buf())?;
    let other = RepositoryRoot::new(linked)?;

    GitDraftStore.save(&main, &draft(": main"))?;
    GitDraftStore.save(&other, &draft(": linked"))?;

    assert_eq!(GitDraftStore.load(&main)?, Some(draft(": main")));
    assert_eq!(GitDraftStore.load(&other)?, Some(draft(": linked")));
    Ok(())
}
