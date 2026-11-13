use super::*;

#[test]
fn every_root_footer_is_one_band_row_with_the_nav_bar_hint()
-> Result<(), Box<dyn std::error::Error>> {
    let services = Services;
    for (width, height) in [(87, 40), (MINIMUM_WIDTH, MINIMUM_HEIGHT)] {
        let mut root = Root::new(InitialPage::Config, of(&services), &[]);
        let mut terminal = Terminal::new(TestBackend::new(width, height))?;
        for section in Section::ALL {
            root.execute(Action::Select(section), of(&services));
            if section == Section::Config {
                root.handle_event(key(KeyCode::Char('2')), of(&services), &[]);
            }
            terminal.draw(|frame| root.render(frame, &[]))?;
            let buffer = terminal.backend().buffer();
            for x in [0, width - 1] {
                assert_eq!(buffer[(x, height - 1)].symbol(), " ", "{section:?}");
                assert_eq!(buffer[(x, height - 1)].bg, theme().band, "{section:?}");
            }
            let footer = rendered(&terminal)
                .lines()
                .last()
                .unwrap_or_default()
                .to_owned();
            // The Library status line drops its trailing hints when crowded.
            if section != Section::Config || width == 87 {
                assert!(
                    footer
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .windows(3)
                        .any(|words| words == ["tab", "nav", "bar"]),
                    "{section:?}: {footer}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn root_wrap_palette_is_modal_and_help_scrolls_with_resize()
-> Result<(), Box<dyn std::error::Error>> {
    let services = Services;
    let mut root = Root::new(InitialPage::Home, of(&services), &[]);
    let help = vec![
        HelpPage::new("Overview", None, |_| numbered(100)),
        HelpPage::new("Config", Some("config"), |_| numbered(1)),
    ];
    root.handle_event(key(KeyCode::BackTab), of(&services), &help);
    assert_eq!(root.section, Section::Help);
    root.handle_event(key(KeyCode::Down), of(&services), &help);
    let mut terminal = Terminal::new(TestBackend::new(66, 18))?;
    terminal.draw(|frame| root.render(frame, &help))?;
    assert_eq!(top(&terminal).as_deref(), Some("gitserious help: config"));
    root.handle_event(key(KeyCode::Tab), of(&services), &help);
    assert_eq!(root.section, Section::Home, "tab wraps from Help to Home");
    root.handle_event(key(KeyCode::Char('/')), of(&services), &help);
    terminal.draw(|frame| root.render(frame, &help))?;
    assert!(rendered(&terminal).contains("┌ Search ─"));
    root.handle_event(Event::Paste("open commit".into()), of(&services), &help);
    root.handle_event(key(KeyCode::Tab), of(&services), &help);
    assert_eq!(root.section, Section::Home);
    root.handle_event(key(KeyCode::Enter), of(&services), &help);
    assert_eq!(root.section, Section::Commit);
    terminal.draw(|frame| root.render(frame, &help))?;
    assert!(rendered(&terminal).contains("Staged (0)"));
    assert!(rendered(&terminal).contains("┌ Changes ─"));
    terminal.backend_mut().resize(40, 10);
    terminal.resize(Rect::new(0, 0, 40, 10))?;
    terminal.draw(|frame| root.render(frame, &help))?;
    assert!(rendered(&terminal).contains("Terminal too small"));
    Ok(())
}

#[test]
fn the_palette_lists_open_commands_then_the_pages_own_then_quit() {
    let uninitialized = Root::new(InitialPage::Home, of(&Services), &[]);
    assert_eq!(palette(&uninitialized), listing(&["Initialize gitserious"]));

    let workspace = HomeWorkspace::new(ProjectOverview::NoRepository {
        directory: fresh_root().as_path().to_path_buf(),
    });
    let no_repository = Root::new(InitialPage::Home, of(&workspace), &[]);
    assert_eq!(palette(&no_repository), listing(&["Initialize gitserious"]));

    let workspace = HomeWorkspace::new(ProjectOverview::Initialized {
        root: fresh_root(),
        default: gitserious_core::built_in_taxonomies()[0].id().clone(),
        version: Some(TaxonomyVersion::V1),
        policy_untracked: false,
    });
    let initialized = Root::new(InitialPage::Home, of(&workspace), &[]);
    assert_eq!(palette(&initialized), listing(&[]));

    let help = Root::new(InitialPage::Help { command: None }, of(&Services), &[]);
    assert_eq!(palette(&help), listing(&[]));
}

#[test]
fn the_commit_palette_follows_the_blocker_the_last_result_and_the_pending_review() {
    let missing = Root::new(InitialPage::Commit, of(&Unavailable), &[]);
    assert_eq!(palette(&missing), listing(&["Refresh repository"]));
    let unstaged = Root::new(InitialPage::Commit, of(&Services), &[]);
    assert_eq!(palette(&unstaged), listing(&["Refresh repository"]));

    let mut ready = Root::new(InitialPage::Commit, of(&Ready), &[]);
    assert_eq!(
        palette(&ready),
        listing(&["Refresh repository", "Start commit"])
    );
    approve_a_commit(&mut ready);
    let created = CommitOutput::new(b"created\n".to_vec(), Vec::new());
    ready.commit.conclude_submission(&Ready, Ok(created));
    assert_eq!(
        palette(&ready),
        listing(&["Refresh repository", "View last Git result", "Start commit"])
    );

    assert_eq!(
        palette(&rejected_review_with(&Ready)),
        listing(&[
            "Refresh repository",
            "View last Git result",
            "Start commit",
            "Resume review"
        ])
    );
    assert_eq!(
        palette(&rejected_review_with(&Services)),
        listing(&[
            "Refresh repository",
            "View last Git result",
            "Resume review"
        ])
    );
}

#[test]
fn each_palette_command_does_what_its_label_says() -> Result<(), Box<dyn std::error::Error>> {
    let mut root = rejected_review_with(&Ready);
    assert_eq!(run(&mut root, &Ready, "Start commit"), None);
    assert!(root.commit.editing());
    assert!(root.commit.has_pending_review());
    root.handle_event(key(KeyCode::Esc), of(&Ready), &[]);
    assert_eq!(run(&mut root, &Ready, "Resume review"), None);
    assert!(root.commit.editing());
    assert!(!root.commit.has_pending_review());

    let mut root = rejected_review_with(&Ready);

    assert_eq!(run(&mut root, &Ready, "View last Git result"), None);
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| root.render(frame, &[]))?;
    let text = rendered(&terminal);
    assert!(text.contains("Commit failed: hook rejected"), "{text}");
    assert!(text.contains("pgup/pgdn"), "{text}");
    root.handle_event(key(KeyCode::Esc), of(&Ready), &[]);

    assert_eq!(run(&mut root, &Ready, "Refresh repository"), None);
    assert!(!palette(&root).contains(&"View last Git result".to_owned()));
    assert!(root.commit.has_pending_review());

    // Config answers `/` with its own palette, so it is opened last.
    for section in [
        Section::Home,
        Section::Help,
        Section::Commit,
        Section::Config,
    ] {
        assert_eq!(
            run(&mut root, &Ready, &format!("Open {}", section.label())),
            None
        );
        assert_eq!(root.section, section);
    }

    root.execute(Action::Select(Section::Commit), of(&Ready));
    assert_eq!(run(&mut root, &Ready, "Quit"), None);
    assert_eq!(
        root.handle_event(key(KeyCode::Char('y')), of(&Ready), &[]),
        Some(Action::Quit)
    );

    let mut home = Root::new(InitialPage::Home, of(&Services), &[]);
    assert_eq!(run(&mut home, &Services, "Initialize gitserious"), None);
    assert_eq!(home.section, Section::Home);
    assert!(home.home.editing());
    Ok(())
}

#[test]
fn ctrl_c_and_esc_reach_quit_from_every_page() {
    let control_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    for section in Section::ALL {
        let mut root = Root::new(InitialPage::Home, of(&Services), &[]);
        root.execute(Action::Select(section), of(&Services));
        assert_eq!(
            root.handle_event(control_c.clone(), of(&Services), &[]),
            Some(Action::Quit),
            "{section:?}"
        );

        root.execute(Action::Select(section), of(&Services));
        let escaped = root.handle_event(key(KeyCode::Esc), of(&Services), &[]);
        if section == Section::Home {
            assert_eq!(escaped, Some(Action::Quit));
        } else {
            assert_eq!(escaped, None, "{section:?}");
            assert_eq!(root.section, Section::Home, "{section:?}");
        }

        let mut pending = rejected_review_on(section);
        assert_eq!(
            pending.handle_event(control_c.clone(), of(&Services), &[]),
            None,
            "{section:?}"
        );
        assert_eq!(
            pending.handle_event(key(KeyCode::Char('y')), of(&Services), &[]),
            Some(Action::Quit),
            "{section:?}"
        );
    }
}

#[test]
fn quit_confirmation_footer_shows_only_the_dialogs_choices()
-> Result<(), Box<dyn std::error::Error>> {
    let services = Services;
    let mut root = rejected_review_on(Section::Config);
    root.handle_event(key(KeyCode::Char('q')), of(&services), &[]);
    assert!(matches!(root.overlay, Some(Overlay::QuitConfirm)));
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| root.render(frame, &[]))?;
    assert!(rendered(&terminal).contains("Discard unapplied changes and quit?"));
    let buffer = terminal.backend().buffer();
    let footer = (0..100)
        .map(|x| buffer[(x, 29)].symbol())
        .collect::<String>();
    assert_eq!(footer.trim_end(), " y discard and quit  enter/esc/n stay");
    assert_eq!(
        root.handle_event(key(KeyCode::Char('n')), of(&services), &[]),
        None
    );
    assert!(root.overlay.is_none());
    Ok(())
}

#[test]
fn tab_and_shift_tab_switch_sections_in_one_press_and_arrows_never_do()
-> Result<(), Box<dyn std::error::Error>> {
    let services = Services;
    let mut root = Root::new(InitialPage::Home, of(&services), &[]);
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| root.render(frame, &[]))?;
    let header: String = (0..100)
        .map(|x| terminal.backend().buffer()[(x, 0)].symbol())
        .collect();
    let version = format!("gitserious v{} ", env!("CARGO_PKG_VERSION"));
    assert!(header.starts_with(" Home  Commit  Config  Help "));
    assert!(header.ends_with(&version));
    let original = root.tabs;

    for expected in [
        Section::Commit,
        Section::Config,
        Section::Help,
        Section::Home,
    ] {
        root.handle_event(key(KeyCode::Tab), of(&services), &[]);
        assert_eq!(root.section, expected, "one tab moves one section");
        terminal.draw(|frame| root.render(frame, &[]))?;
        let active = root.tabs.area(expected);
        let cell = terminal.backend().buffer()[(active.x, 0)].clone();
        assert_eq!(cell.fg, theme().text);
        assert!(
            cell.modifier
                .contains(Modifier::UNDERLINED | Modifier::BOLD)
        );
        let text = rendered(&terminal);
        assert!(!text.contains("tab back"), "no separate nav bar step");
        assert!(text.contains("tab nav bar"), "{expected:?}");
    }
    for expected in [
        Section::Help,
        Section::Config,
        Section::Commit,
        Section::Home,
    ] {
        root.handle_event(key(KeyCode::BackTab), of(&services), &[]);
        assert_eq!(
            root.section, expected,
            "one shift+tab moves back one section"
        );
    }
    for code in [KeyCode::Left, KeyCode::Right] {
        root.handle_event(key(code), of(&services), &[]);
        assert_eq!(root.section, Section::Home, "arrows never switch sections");
    }

    terminal.backend_mut().resize(66, 18);
    terminal.resize(Rect::new(0, 0, 66, 18))?;
    terminal.draw(|frame| root.render(frame, &[]))?;
    assert_eq!(root.tabs, original, "tabs stay left-aligned");
    let help = root.tabs.area(Section::Help);
    root.handle_event(click(help), of(&services), &[]);
    assert_eq!(root.section, Section::Help);
    Ok(())
}

#[test]
fn a_pending_review_requires_confirmation_before_quitting() {
    let mut root = rejected_review_with(&Services);

    assert_eq!(
        root.handle_event(key(KeyCode::Char('q')), of(&Services), &[]),
        None
    );
    assert!(matches!(root.overlay, Some(Overlay::QuitConfirm)));
    assert_eq!(
        root.handle_event(key(KeyCode::Char('n')), of(&Services), &[]),
        None
    );
    assert!(root.overlay.is_none());
    assert!(root.commit.has_pending_review());
    assert_eq!(root.section, Section::Commit);

    root.handle_event(key(KeyCode::Char('q')), of(&Services), &[]);
    assert_eq!(
        root.handle_event(key(KeyCode::Char('y')), of(&Services), &[]),
        Some(Action::Quit)
    );
}

#[test]
fn key_releases_change_nothing_on_any_page_or_overlay() -> Result<(), Box<dyn std::error::Error>> {
    let release = |code| {
        Event::Key(KeyEvent::new_with_kind(
            code,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ))
    };
    let codes = [
        KeyCode::Tab,
        KeyCode::BackTab,
        KeyCode::Esc,
        KeyCode::Enter,
        KeyCode::Char('q'),
        KeyCode::Char('/'),
        KeyCode::Char('c'),
    ];
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    for section in Section::ALL {
        let mut root = Root::new(InitialPage::Home, of(&Services), &[]);
        root.execute(Action::Select(section), of(&Services));
        for open in [false, true] {
            if open {
                root.handle_event(key(KeyCode::Char('/')), of(&Services), &[]);
            }
            terminal.draw(|frame| root.render(frame, &[]))?;
            let before = rendered(&terminal);
            for code in codes {
                assert_eq!(
                    root.handle_event(release(code), of(&Services), &[]),
                    None,
                    "{section:?} {code:?}"
                );
            }
            assert_eq!(root.section, section);
            terminal.draw(|frame| root.render(frame, &[]))?;
            assert_eq!(rendered(&terminal), before, "{section:?} palette {open}");
        }
    }

    let mut root = Root::new(InitialPage::Commit, of(&Services), &[]);
    root.commit.notice_mut().push_str("hook rejected");
    root.handle_event(key(KeyCode::Char('o')), of(&Services), &[]);
    for code in codes {
        assert_eq!(root.handle_event(release(code), of(&Services), &[]), None);
        assert!(root.commit.modal(), "{code:?}");
    }
    Ok(())
}

#[test]
fn a_too_small_terminal_quits_on_q_esc_and_ctrl_c_and_ignores_other_input()
-> Result<(), Box<dyn std::error::Error>> {
    let control_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    for section in Section::ALL {
        for (width, height) in [(MINIMUM_WIDTH - 1, 40), (87, MINIMUM_HEIGHT - 1)] {
            let fresh = || {
                let mut root = Root::new(InitialPage::Home, of(&Services), &[]);
                root.execute(Action::Select(section), of(&Services));
                root
            };
            let mut root = fresh();
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| root.render(frame, &[]))?;
            assert!(
                rendered(&terminal).contains("Resize or press esc/q to quit"),
                "{section:?} {width}x{height}"
            );
            for event in [
                key(KeyCode::Tab),
                key(KeyCode::BackTab),
                key(KeyCode::Enter),
                key(KeyCode::Char('/')),
                key(KeyCode::Char('r')),
                key(KeyCode::Char('c')),
                click(Rect::new(1, 0, 1, 1)),
                click(Rect::new(8, 0, 1, 1)),
                click(Rect::new(15, 0, 1, 1)),
            ] {
                let shown = format!("{event:?}");
                assert_eq!(
                    root.handle_event(event, of(&Services), &[]),
                    None,
                    "{section:?} {shown}"
                );
                assert_eq!(root.section, section, "{shown}");
                assert!(root.overlay.is_none(), "{shown}");
                assert!(!root.home.editing(), "{shown}");
            }
            for event in [
                key(KeyCode::Char('q')),
                key(KeyCode::Esc),
                control_c.clone(),
            ] {
                let mut root = fresh();
                terminal.draw(|frame| root.render(frame, &[]))?;
                assert_eq!(
                    root.handle_event(event.clone(), of(&Services), &[]),
                    Some(Action::Quit),
                    "{section:?} {event:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn ctrl_c_waits_while_the_git_result_or_a_hidden_config_edit_is_open()
-> Result<(), Box<dyn std::error::Error>> {
    let control_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    let mut root = Root::new(InitialPage::Commit, of(&Services), &[]);
    root.commit.notice_mut().push_str("hook rejected");
    root.handle_event(key(KeyCode::Char('o')), of(&Services), &[]);
    assert!(root.commit.modal());
    assert_eq!(
        root.handle_event(control_c.clone(), of(&Services), &[]),
        None
    );
    assert!(root.commit.modal());
    root.handle_event(key(KeyCode::Esc), of(&Services), &[]);
    assert!(!root.commit.modal());
    assert_eq!(
        root.handle_event(control_c.clone(), of(&Services), &[]),
        Some(Action::Quit)
    );

    let mut config = Root::new(InitialPage::Config, of(&Services), &[]);
    config.handle_event(key(KeyCode::Char('2')), of(&Services), &[]);
    config.handle_event(key(KeyCode::Char('n')), of(&Services), &[]);
    let mut small = Terminal::new(TestBackend::new(MINIMUM_WIDTH - 1, 40))?;
    small.draw(|frame| config.render(frame, &[]))?;
    assert_eq!(config.handle_event(control_c, of(&Services), &[]), None);
    assert!(config.config.modal());
    assert_eq!(config.section, Section::Config);
    Ok(())
}

#[test]
fn an_open_palette_takes_every_key_and_ignores_clicks() -> Result<(), Box<dyn std::error::Error>> {
    let control_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    for section in [Section::Home, Section::Commit, Section::Help] {
        let mut root = Root::new(InitialPage::Home, of(&Services), &[]);
        root.execute(Action::Select(section), of(&Services));
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| root.render(frame, &[]))?;
        let (x, y) = position(&terminal, "Config").ok_or("no Config tab")?;
        root.handle_event(key(KeyCode::Char('/')), of(&Services), &[]);
        for event in [
            key(KeyCode::Char('q')),
            key(KeyCode::Tab),
            key(KeyCode::BackTab),
            click(Rect::new(x, y, 1, 1)),
            control_c.clone(),
        ] {
            let shown = format!("{event:?}");
            assert_eq!(
                root.handle_event(event, of(&Services), &[]),
                None,
                "{section:?} {shown}"
            );
            assert_eq!(root.section, section, "{shown}");
            assert!(matches!(root.overlay, Some(Overlay::Palette(_))), "{shown}");
        }
        terminal.draw(|frame| root.render(frame, &[]))?;
        let search = rendered(&terminal);
        assert!(search.contains("/ q "), "{section:?}: q joins the query");
        assert!(!search.contains("/ qc"), "{section:?}: ctrl+c does not");
        assert_eq!(
            root.handle_event(key(KeyCode::Esc), of(&Services), &[]),
            None
        );
        assert!(root.overlay.is_none());
        assert_eq!(root.section, section);
    }
    Ok(())
}

#[test]
fn every_initial_page_draws_what_selecting_it_draws() -> Result<(), Box<dyn std::error::Error>> {
    let help = pages(3);
    for (start, section) in [
        (InitialPage::Home, Section::Home),
        (InitialPage::Commit, Section::Commit),
        (InitialPage::Config, Section::Config),
        (InitialPage::Help { command: None }, Section::Help),
    ] {
        let from = if section == Section::Home {
            InitialPage::Commit
        } else {
            InitialPage::Home
        };
        let mut opened = Root::new(start, of(&Services), &help);
        let mut visited = Root::new(from, of(&Services), &help);
        visited.execute(Action::Select(section), of(&Services));
        assert_eq!(opened.section, section);
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| opened.render(frame, &help))?;
        let opened_frame = terminal.backend().buffer().clone();
        terminal.draw(|frame| visited.render(frame, &help))?;
        assert_eq!(&opened_frame, terminal.backend().buffer(), "{section:?}");
    }
    Ok(())
}
