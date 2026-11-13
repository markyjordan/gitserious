use super::*;

#[test]
fn full_git_result_scrolls_and_keeps_navigation_modal() -> Result<(), Box<dyn std::error::Error>> {
    use std::fmt::Write as _;

    let services = Services;
    let mut root = Root::new(InitialPage::Home, of(&services), &[]);
    root.execute(Action::Select(Section::Commit), of(&services));
    for index in 0..100 {
        writeln!(root.commit.notice_mut(), "Git diagnostic {index}")?;
    }
    root.handle_event(key(KeyCode::Char('o')), of(&services), &[]);
    root.handle_event(key(KeyCode::PageDown), of(&services), &[]);
    root.handle_event(key(KeyCode::Tab), of(&services), &[]);
    assert_eq!(root.section, Section::Commit);
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| root.render(frame, &[]))?;
    assert!(rendered(&terminal).contains("Git diagnostic 10"));
    root.handle_event(key(KeyCode::Esc), of(&services), &[]);
    assert!(root.overlay.is_none());
    Ok(())
}
#[test]
fn opening_commit_matches_navigating_there_and_does_not_author() {
    let services = Services;
    let mut opened = Root::new(InitialPage::Commit, of(&services), &[]);
    let mut visited = Root::new(InitialPage::Home, of(&services), &[]);
    visited.execute(Action::Select(Section::Commit), of(&services));

    assert_eq!(opened.section, Section::Commit);
    assert_eq!(commit_page(&opened), commit_page(&visited));
    assert!(!opened.commit.has_pending_review());
    assert!(opened.commit.notice().is_empty());
    assert_eq!(
        opened.handle_event(key(KeyCode::Enter), of(&services), &[]),
        None
    );
    assert!(!opened.commit.editing());
    assert_eq!(opened.commit.notice(), "Stage changes with Git");
    assert!(!opened.commit.has_pending_review());
}

#[test]
fn a_missing_repository_blocks_the_commit_page_and_leaves_the_other_pages_open() {
    let workspace = Unavailable;
    let mut opened = Root::new(InitialPage::Commit, of(&workspace), &[]);
    let mut visited = Root::new(InitialPage::Home, of(&workspace), &[]);
    visited.execute(Action::Select(Section::Commit), of(&workspace));

    assert_eq!(opened.commit.blocker(), Some("repository unavailable"));
    assert!(opened.commit.snapshot().is_none());
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
    let mut root = Root::new(InitialPage::Commit, of(&Ready), &[]);
    let notice = root.commit.notice().to_owned();

    assert_eq!(
        root.handle_event(key(KeyCode::Enter), of(&Ready), &[]),
        None
    );
    assert!(root.commit.editing());
    assert_eq!(root.handle_event(key(KeyCode::Esc), of(&Ready), &[]), None);

    assert_eq!(root.section, Section::Commit);
    assert!(!root.commit.editing());
    assert!(!root.commit.has_pending_review());
    assert_eq!(root.commit.notice(), notice);
}

#[test]
fn a_created_commit_returns_to_commit_and_a_rejected_one_keeps_the_review()
-> Result<(), Box<dyn std::error::Error>> {
    let created = CommitOutput::new(b"created\n".to_vec(), b"hook note\n".to_vec());
    let mut root = Root::new(InitialPage::Commit, of(&Ready), &[]);

    assert_eq!(
        approve_a_commit(&mut root),
        Some(Action::Commit(CommitRequest::Submit))
    );
    root.commit.conclude_submission(&Ready, Ok(created));

    assert_eq!(root.section, Section::Commit);
    assert!(!root.commit.editing());
    assert!(!root.commit.has_pending_review());
    assert!(root.commit.notice().contains("Commit created."));
    assert!(root.commit.notice().contains("created"));
    assert!(root.commit.notice().contains("hook note"));

    approve_a_commit(&mut root);
    root.commit
        .conclude_submission(&Ready, Err("hook rejected".into()));

    assert_eq!(root.section, Section::Commit);
    assert!(!root.commit.editing());
    assert!(root.commit.has_pending_review());
    assert!(
        root.commit
            .notice()
            .contains("The reviewed message is retained.")
    );
    assert!(root.commit.notice().contains("hook rejected"));
    assert_eq!(
        root.handle_event(key(KeyCode::Enter), of(&Ready), &[]),
        None
    );
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| root.render(frame, &[]))?;
    assert!(rendered(&terminal).contains("feat: open the commit page"));
    assert_eq!(
        root.handle_event(key(KeyCode::Enter), of(&Ready), &[]),
        Some(Action::Commit(CommitRequest::Submit))
    );
    Ok(())
}

#[test]
fn a_failed_review_survives_navigation_refresh_and_cancelling_new_authoring()
-> Result<(), Box<dyn std::error::Error>> {
    let mut root = rejected_review_with(&Ready);
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| root.render(frame, &[]))?;
    assert!(rendered(&terminal).contains("The reviewed message is retained."));

    for expected in [
        Section::Config,
        Section::Help,
        Section::Home,
        Section::Commit,
    ] {
        assert_eq!(root.handle_event(key(KeyCode::Tab), of(&Ready), &[]), None);
        assert_eq!(root.section, expected);
        assert!(root.commit.has_pending_review());
    }
    assert_eq!(
        root.handle_event(key(KeyCode::Char('r')), of(&Ready), &[]),
        None
    );
    assert!(root.commit.has_pending_review());
    assert_eq!(
        root.handle_event(key(KeyCode::Char('c')), of(&Ready), &[]),
        None
    );
    assert!(root.commit.editing());
    assert_eq!(root.handle_event(key(KeyCode::Esc), of(&Ready), &[]), None);
    assert!(!root.commit.editing());
    assert!(root.commit.has_pending_review());
    assert_eq!(
        root.handle_event(key(KeyCode::Enter), of(&Ready), &[]),
        None
    );
    assert!(root.commit.editing());
    assert!(!root.commit.has_pending_review());
    Ok(())
}

#[test]
fn repeated_rejections_remain_resumable_until_a_successful_retry() {
    let mut root = Root::new(InitialPage::Commit, of(&Ready), &[]);
    approve_a_commit(&mut root);
    for rejection in ["first hook rejection", "second hook rejection"] {
        root.commit
            .conclude_submission(&Ready, Err(rejection.into()));
        assert!(root.commit.has_pending_review());
        assert!(root.commit.notice().contains(rejection));
        assert!(!root.commit.notice().contains("Commit created."));
        assert_eq!(
            root.handle_event(key(KeyCode::Enter), of(&Ready), &[]),
            None
        );
        assert_eq!(
            root.handle_event(key(KeyCode::Enter), of(&Ready), &[]),
            Some(Action::Commit(CommitRequest::Submit))
        );
    }

    root.commit.conclude_submission(
        &Ready,
        Ok(CommitOutput::new(b"retry created\n".to_vec(), Vec::new())),
    );
    assert!(!root.commit.has_pending_review());
    assert_eq!(root.commit.notice(), "Commit created.\nretry created");
    assert_eq!(
        root.handle_event(key(KeyCode::Enter), of(&Ready), &[]),
        None
    );
    assert!(root.commit.editing());
}

/// A root on Commit with the editor open on a feat draft whose subject is `subject`.
fn editing(subject: &str) -> Root {
    let mut root = Root::new(InitialPage::Commit, of(&Ready), &[]);
    for event in [
        key(KeyCode::Enter),
        key(KeyCode::Enter),
        key(KeyCode::Down),
        Event::Paste(subject.into()),
    ] {
        assert_eq!(root.handle_event(event, of(&Ready), &[]), None);
    }
    root
}

fn draw(root: &mut Root) -> Result<Terminal<TestBackend>, Box<dyn std::error::Error>> {
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| root.render(frame, &[]))?;
    Ok(terminal)
}

#[test]
fn enter_opens_the_editor_under_the_nav_bar_with_the_staged_header_and_edit_badge()
-> Result<(), Box<dyn std::error::Error>> {
    let mut root = Root::new(InitialPage::Commit, of(&Ready), &[]);
    assert_eq!(
        root.handle_event(key(KeyCode::Enter), of(&Ready), &[]),
        None
    );
    assert!(root.commit.editing());

    let terminal = draw(&mut root)?;
    let text = rendered(&terminal);
    let rows: Vec<_> = text.lines().collect();
    assert!(rows[0].contains("Home  Commit  Config  Help"), "{text}");
    assert!(
        rows[1].starts_with(" 2 staged  src/net/retry.rs · tests/retry.rs"),
        "{text}"
    );
    assert!(rows[1].trim_end().ends_with("ctrl+g change"), "{text}");
    assert!(!rows[1].contains("README.md"), "only staged paths");
    assert!(rows[2].chars().all(|symbol| symbol == '─'), "{text}");
    assert!(!text.contains("Commit / "), "no breadcrumb takeover");
    assert!(
        text.contains("feat"),
        "the type picker sits in the page body"
    );

    let footer = footer(&terminal);
    assert!(
        footer.starts_with("  EDIT  enter choose  ↑/↓ move  ←/→ taxonomy  esc back  "),
        "{footer}"
    );
    assert!(footer.trim_end().ends_with("tab nav bar"), "{footer}");
    assert!(!footer.contains("q quit"), "q reaches the editor");
    let buffer = terminal.backend().buffer();
    let (x, y) = position(&terminal, " EDIT ").ok_or("missing badge")?;
    for column in x..x + 6 {
        let cell = &buffer[(column, y)];
        assert_eq!(cell.fg, theme().ground);
        assert_eq!(cell.bg, theme().pending);
        assert!(cell.modifier.contains(Modifier::BOLD));
    }
    let (x, y) = position(&terminal, "enter choose").ok_or("missing hint")?;
    assert!(buffer[(x, y)].modifier.contains(Modifier::BOLD));
    assert_eq!(buffer[(x, y)].fg, theme().text);
    assert_eq!(buffer[(x + 6, y)].fg, theme().dim);
    assert_eq!(buffer[(x + 6, y)].bg, theme().band);
    Ok(())
}

#[test]
fn the_type_picker_fits_the_page_at_80x24_and_100x30() -> Result<(), Box<dyn std::error::Error>> {
    for (width, height) in [(80, 24), (100, 30)] {
        let mut root = Root::new(InitialPage::Commit, of(&Ready), &[]);
        assert_eq!(
            root.handle_event(key(KeyCode::Enter), of(&Ready), &[]),
            None
        );
        let mut terminal = Terminal::new(TestBackend::new(width, height))?;
        terminal.draw(|frame| root.render(frame, &[]))?;
        let text = rendered(&terminal);
        let rows: Vec<_> = text.lines().collect();
        assert!(
            rows[3].starts_with("▎   type(scope): description"),
            "{text}"
        );
        for (offset, kind) in [
            "feat", "fix", "refactor", "perf", "test", "docs", "chore", "build", "ci", "style",
            "revert",
        ]
        .into_iter()
        .enumerate()
        {
            assert!(
                rows[8 + offset].trim_start().starts_with(kind),
                "{kind} at {width}x{height}\n{text}"
            );
        }
        let detail = rows
            .iter()
            .position(|row| row.contains("An addition or expansion of capability."))
            .ok_or("missing description")?;
        assert!(detail < usize::from(height) - 1, "{text}");
        assert!(!text.contains("Typing filters"), "{text}");
        assert!(!text.contains("tab taxonomy"), "{text}");
    }
    Ok(())
}

#[test]
fn review_draws_the_message_without_provenance_and_approves_it_with_provenance()
-> Result<(), Box<dyn std::error::Error>> {
    let mut root = Root::new(InitialPage::Commit, of(&Ready), &[]);
    review_a_commit(&mut root);
    let terminal = draw(&mut root)?;
    let text = rendered(&terminal);
    let rows: Vec<_> = text.lines().collect();
    assert!(rows[1].starts_with(" 2 staged"), "{text}");
    assert!(rows[1].trim_end().ends_with("ctrl+g change"), "{text}");
    assert!(rows[3].starts_with(" feat: open the commit page"), "{text}");
    assert!(!text.contains("Gitserious-"), "{text}");
    let commit = rows
        .iter()
        .position(|row| row.starts_with("  commit to main (first commit) "))
        .ok_or("missing commit row")?;
    assert!(rows[commit].ends_with(" enter  "), "{text}");
    assert!(rows[commit - 1].trim().is_empty(), "{text}");
    assert!(rows[commit - 2].contains("reviewed decision"), "{text}");
    let footer = footer(&terminal);
    assert!(
        footer.starts_with("  EDIT  enter commit  esc keep editing  ↑/↓ scroll  "),
        "{footer}"
    );
    assert!(!footer.contains("cancel"), "{footer}");

    assert_eq!(
        root.handle_event(key(KeyCode::Enter), of(&Ready), &[]),
        Some(Action::Commit(CommitRequest::Submit))
    );
    let message = root
        .commit
        .approved()
        .ok_or("missing approved commit")?
        .reviewed_message()
        .as_str();
    assert!(
        message.contains("\nGitserious-Taxonomy: conventional-v1\n"),
        "{message}"
    );
    assert!(
        message.contains("\nGitserious-Schema: sha256:"),
        "{message}"
    );
    Ok(())
}

#[test]
fn tab_reaches_the_nav_bar_and_returning_to_commit_keeps_the_draft()
-> Result<(), Box<dyn std::error::Error>> {
    let mut root = editing("keep this subject");
    for expected in [
        Section::Config,
        Section::Help,
        Section::Home,
        Section::Commit,
    ] {
        assert_eq!(root.handle_event(key(KeyCode::Tab), of(&Ready), &[]), None);
        assert_eq!(root.section, expected);
        assert!(root.commit.editing());
    }
    assert!(rendered(&draw(&mut root)?).contains("keep this subject"));

    let mut terminal = draw(&mut root)?;
    root.handle_event(click(root.tabs.area(Section::Help)), of(&Ready), &[]);
    assert_eq!(root.section, Section::Help);
    root.handle_event(
        Event::Key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)),
        of(&Ready),
        &[],
    );
    assert_eq!(root.section, Section::Config);
    root.execute(Action::Select(Section::Commit), of(&Ready));
    terminal.draw(|frame| root.render(frame, &[]))?;
    assert!(rendered(&terminal).contains("keep this subject"));
    Ok(())
}

#[test]
fn keys_the_root_claims_elsewhere_reach_the_editor() -> Result<(), Box<dyn std::error::Error>> {
    let mut root = editing("a");
    for code in [KeyCode::Char('/'), KeyCode::Char('q'), KeyCode::Char('r')] {
        assert_eq!(root.handle_event(key(code), of(&Ready), &[]), None);
    }
    assert!(root.overlay.is_none());
    assert_eq!(root.section, Section::Commit);
    assert!(rendered(&draw(&mut root)?).contains("a/qr"));

    assert_eq!(root.handle_event(key(KeyCode::Esc), of(&Ready), &[]), None);
    assert_eq!(root.section, Section::Commit, "esc stays with the editor");
    assert!(root.commit.editing());
    assert!(rendered(&draw(&mut root)?).contains("Discard Message"));
    Ok(())
}

#[test]
fn ctrl_g_shows_what_is_staged_and_esc_returns_to_the_draft()
-> Result<(), Box<dyn std::error::Error>> {
    let mut root = editing("look before committing");
    let control_g = Event::Key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL));
    assert_eq!(root.handle_event(control_g, of(&Ready), &[]), None);
    assert!(root.commit.modal());

    let text = rendered(&draw(&mut root)?);
    assert!(text.contains("Staged (2)"), "{text}");
    assert!(text.contains("retry.rs"), "{text}");
    assert!(!text.contains("Unstaged"), "{text}");
    assert!(!text.contains("README.md"), "{text}");

    root.handle_event(key(KeyCode::Tab), of(&Ready), &[]);
    root.handle_event(key(KeyCode::Char('x')), of(&Ready), &[]);
    assert_eq!(root.section, Section::Commit, "the staged view holds focus");
    root.handle_event(key(KeyCode::Esc), of(&Ready), &[]);
    assert!(!root.commit.modal());
    assert!(root.commit.editing());
    let text = rendered(&draw(&mut root)?);
    assert!(text.contains("look before committing"), "{text}");
    assert!(!text.contains("look before committingx"), "{text}");
    Ok(())
}

#[test]
fn quitting_with_a_draft_asks_first_from_any_page() {
    let mut root = editing("unsaved");
    let control_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert_eq!(root.handle_event(control_c, of(&Ready), &[]), None);
    assert!(matches!(root.overlay, Some(Overlay::QuitConfirm)));
    root.handle_event(key(KeyCode::Char('n')), of(&Ready), &[]);
    assert!(root.commit.editing());

    root.handle_event(key(KeyCode::Tab), of(&Ready), &[]);
    root.handle_event(key(KeyCode::Tab), of(&Ready), &[]);
    assert_eq!(root.section, Section::Help);
    assert_eq!(
        root.handle_event(key(KeyCode::Char('q')), of(&Ready), &[]),
        None
    );
    assert!(matches!(root.overlay, Some(Overlay::QuitConfirm)));
    assert_eq!(
        root.handle_event(key(KeyCode::Char('y')), of(&Ready), &[]),
        Some(Action::Quit)
    );
}

#[test]
fn approving_a_review_asks_the_session_loop_to_submit() {
    let mut root = rejected_review_with(&Ready);
    assert_eq!(
        root.session_event(key(KeyCode::Enter), of(&Ready), &[]),
        None
    );
    assert_eq!(
        root.session_event(key(KeyCode::Enter), of(&Ready), &[]),
        Some(SessionRequest::Submit)
    );
}
