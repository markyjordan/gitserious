use super::*;

#[test]
fn full_git_result_scrolls_and_keeps_navigation_modal() -> Result<(), Box<dyn std::error::Error>> {
    use std::fmt::Write as _;

    let services = Services;
    let mut shell = Shell::new(InitialPage::Home, of(&services), &[]);
    shell.execute(Action::Select(Section::Commit), of(&services));
    for index in 0..100 {
        writeln!(&mut shell.commit.notice, "Git diagnostic {index}")?;
    }
    shell.handle_event(key(KeyCode::Char('o')), of(&services), &[]);
    shell.handle_event(key(KeyCode::PageDown), of(&services), &[]);
    shell.handle_event(key(KeyCode::Tab), of(&services), &[]);
    assert_eq!(shell.section, Section::Commit);
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| shell.render(frame, &[]))?;
    assert!(rendered(&terminal).contains("Git diagnostic 10"));
    shell.handle_event(key(KeyCode::Esc), of(&services), &[]);
    assert!(shell.overlay.is_none());
    Ok(())
}
#[test]
fn opening_commit_matches_navigating_there_and_does_not_author() {
    let services = Services;
    let mut opened = Shell::new(InitialPage::Commit, of(&services), &[]);
    let mut visited = Shell::new(InitialPage::Home, of(&services), &[]);
    visited.execute(Action::Select(Section::Commit), of(&services));

    assert_eq!(opened.section, Section::Commit);
    assert_eq!(commit_page(&opened), commit_page(&visited));
    assert!(!opened.commit.has_pending_review());
    assert!(opened.commit.notice.is_empty());
    assert_eq!(
        opened.handle_event(key(KeyCode::Enter), of(&services), &[]),
        Some(Action::Author)
    );
    assert!(!opened.commit.has_pending_review());
}

#[test]
fn a_missing_repository_blocks_the_commit_page_and_leaves_the_other_pages_open() {
    let workspace = Unavailable;
    let mut opened = Shell::new(InitialPage::Commit, of(&workspace), &[]);
    let mut visited = Shell::new(InitialPage::Home, of(&workspace), &[]);
    visited.execute(Action::Select(Section::Commit), of(&workspace));

    assert_eq!(
        opened.commit.blocker.as_deref(),
        Some("repository unavailable")
    );
    assert!(opened.commit.snapshot.is_none());
    assert_eq!(commit_page(&opened), commit_page(&visited));
    assert_eq!(
        opened.handle_event(key(KeyCode::Tab), of(&workspace), &[]),
        None
    );
    assert_eq!(opened.section, Section::Config);
    assert_eq!(
        opened.handle_event(key(KeyCode::Char('q')), of(&workspace), &[]),
        Some(Action::Quit)
    );
}

#[test]
fn cancelling_authoring_returns_to_commit_without_writing() {
    let services = Services;
    let mut shell = Shell::new(InitialPage::Commit, of(&services), &[]);
    let notice = shell.commit.notice.clone();

    shell.commit.conclude_authoring(&services, None);

    assert_eq!(shell.section, Section::Commit);
    assert!(!shell.commit.has_pending_review());
    assert_eq!(shell.commit.notice, notice);
}

#[test]
fn a_created_commit_returns_to_commit_and_a_rejected_one_keeps_the_review()
-> Result<(), Box<dyn std::error::Error>> {
    let (context, _, checkpoint) = commit::reviewed_submission()?;
    let created = CommitOutput::new(b"created\n".to_vec(), b"hook note\n".to_vec());
    let mut shell = Shell::new(InitialPage::Commit, of(&Services), &[]);

    shell
        .commit
        .conclude_authoring(&Services, Some((context, checkpoint, Ok(created))));

    assert_eq!(shell.section, Section::Commit);
    assert!(!shell.commit.has_pending_review());
    assert!(shell.commit.notice.contains("Commit created."));
    assert!(shell.commit.notice.contains("created"));
    assert!(shell.commit.notice.contains("hook note"));

    let (context, _, checkpoint) = commit::reviewed_submission()?;
    shell.commit.conclude_authoring(
        &Services,
        Some((context, checkpoint, Err("hook rejected".into()))),
    );

    assert_eq!(shell.section, Section::Commit);
    assert!(shell.commit.has_pending_review());
    assert!(
        shell
            .commit
            .notice
            .contains("The reviewed message is retained.")
    );
    assert!(shell.commit.notice.contains("hook rejected"));
    Ok(())
}

#[test]
fn a_failed_review_survives_navigation_refresh_and_cancelling_new_authoring()
-> Result<(), Box<dyn std::error::Error>> {
    let (context, _, checkpoint) = commit::reviewed_submission()?;
    let mut shell = Shell::new(InitialPage::Commit, of(&Services), &[]);
    shell.commit.conclude_authoring(
        &Services,
        Some((context, checkpoint, Err("hook rejected".into()))),
    );
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| shell.render(frame, &[]))?;
    assert!(rendered(&terminal).contains("The reviewed message is retained."));

    for expected in [
        Section::Config,
        Section::Help,
        Section::Home,
        Section::Commit,
    ] {
        assert_eq!(
            shell.handle_event(key(KeyCode::Tab), of(&Services), &[]),
            None
        );
        assert_eq!(shell.section, expected);
        assert!(shell.commit.has_pending_review());
    }
    assert_eq!(
        shell.handle_event(key(KeyCode::Char('r')), of(&Services), &[]),
        None
    );
    assert!(shell.commit.has_pending_review());
    assert_eq!(
        shell.handle_event(key(KeyCode::Enter), of(&Services), &[]),
        Some(Action::Resume)
    );
    assert_eq!(
        shell.handle_event(key(KeyCode::Char('c')), of(&Services), &[]),
        Some(Action::Author)
    );
    shell.commit.conclude_authoring(&Services, None);
    assert!(shell.commit.has_pending_review());
    assert_eq!(
        shell.handle_event(key(KeyCode::Enter), of(&Services), &[]),
        Some(Action::Resume)
    );
    Ok(())
}

#[test]
fn repeated_rejections_remain_resumable_until_a_successful_retry()
-> Result<(), Box<dyn std::error::Error>> {
    let mut shell = Shell::new(InitialPage::Commit, of(&Services), &[]);
    for rejection in ["first hook rejection", "second hook rejection"] {
        let (context, _, checkpoint) = commit::reviewed_submission()?;
        shell.commit.conclude_authoring(
            &Services,
            Some((context, checkpoint, Err(rejection.into()))),
        );
        assert!(shell.commit.has_pending_review());
        assert!(shell.commit.notice.contains(rejection));
        assert!(!shell.commit.notice.contains("Commit created."));
        assert_eq!(
            shell.handle_event(key(KeyCode::Enter), of(&Services), &[]),
            Some(Action::Resume)
        );
    }

    let (context, _, checkpoint) = commit::reviewed_submission()?;
    shell.commit.conclude_authoring(
        &Services,
        Some((
            context,
            checkpoint,
            Ok(CommitOutput::new(b"retry created\n".to_vec(), Vec::new())),
        )),
    );
    assert!(!shell.commit.has_pending_review());
    assert_eq!(shell.commit.notice, "Commit created.\nretry created");
    assert_eq!(
        shell.handle_event(key(KeyCode::Enter), of(&Services), &[]),
        Some(Action::Author)
    );
    Ok(())
}
