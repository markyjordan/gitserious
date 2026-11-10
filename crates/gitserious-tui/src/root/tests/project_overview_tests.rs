use super::*;

#[test]
fn an_uninitialized_project_matches_the_open_bone_overview()
-> Result<(), Box<dyn std::error::Error>> {
    let workspace = HomeWorkspace::new(ProjectOverview::Uninitialized { root: fresh_root() });
    let mut root = Root::new(InitialPage::Config, of(&workspace), &[]);
    for (width, height) in [(87, 40), (100, 30), (125, 51), (66, 18)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height))?;
        terminal.draw(|frame| root.render(frame, &[]))?;
        let text = rendered(&terminal);
        assert!(
            text.contains("No gitserious configuration found in this project."),
            "{text}"
        );
        assert!(text.contains("✗ gitserious.toml"));
        assert!(text.contains("fresh"));
        assert!(text.contains(&fresh_root().as_path().display().to_string()));
        assert!(!text.contains(['┌', '┐', '└', '┘']));
        let rows: Vec<_> = text.lines().collect();
        let title = 1 + u16::from(height >= 24);
        let button = title + 5;
        assert!(
            rows[usize::from(button)]
                .chars()
                .skip(14)
                .collect::<String>()
                .starts_with(" initialize gitserious")
        );
        assert!(rows[usize::from(button)].ends_with("enter "));
        let buffer = terminal.backend().buffer();
        for x in 14..width {
            assert_eq!(buffer[(x, button)].bg, theme().cursor);
            assert_eq!(buffer[(x, button)].fg, theme().ground);
            assert!(buffer[(x, button)].modifier.contains(Modifier::BOLD));
            assert_eq!(buffer[(x, button + 1)].bg, theme().ground);
        }
        let footer = rows[usize::from(height - 1)];
        assert!(
            footer.starts_with(" enter initialize  ←/→ focus"),
            "{footer}"
        );
        assert!(workspace.created.borrow().is_none());
    }
    Ok(())
}

#[test]
fn project_enter_opens_the_same_editor_as_home_and_cancel_writes_nothing()
-> Result<(), Box<dyn std::error::Error>> {
    for (width, height) in [(87, 40), (100, 30), (125, 51), (66, 18)] {
        let workspace = HomeWorkspace::new(ProjectOverview::Uninitialized { root: fresh_root() });
        let mut project = Root::new(InitialPage::Config, of(&workspace), &[]);
        let mut home = Root::new(InitialPage::Home, of(&workspace), &[]);
        let mut terminal = Terminal::new(TestBackend::new(width, height))?;
        project.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
        home.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
        assert_eq!(project.section, Section::Home);
        terminal.draw(|frame| project.render(frame, &[]))?;
        let project_frame = terminal.backend().buffer().clone();
        terminal.draw(|frame| home.render(frame, &[]))?;
        assert_eq!(&project_frame, terminal.backend().buffer());
        assert!(workspace.created.borrow().is_none());
        project.handle_event(key(KeyCode::Esc), of(&workspace), &[]);
        assert!(!project.home.editing());
        assert!(workspace.created.borrow().is_none());
        project.execute(Action::Select(Section::Config), of(&workspace));
        project.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
        // Exclude conventional and make infra-ops the default; only those choices may
        // appear in Project after the root reloads the initialized configuration.
        project.handle_event(key(KeyCode::Char(' ')), of(&workspace), &[]);
        project.handle_event(key(KeyCode::Down), of(&workspace), &[]);
        project.handle_event(key(KeyCode::Down), of(&workspace), &[]);
        project.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
        for _ in 0..3 {
            project.handle_event(key(KeyCode::Down), of(&workspace), &[]);
        }
        project.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
        assert!(!project.home.editing());
        let configuration = workspace
            .created
            .borrow()
            .clone()
            .ok_or("not initialized")?;
        assert_eq!(configuration.default_taxonomy().as_str(), "infra-ops");
        assert_eq!(
            configuration
                .taxonomies()
                .iter()
                .map(|taxonomy| taxonomy.reference().to_string())
                .collect::<Vec<_>>(),
            ["infra-ops-v1", "research-v1"]
        );
        project.execute(Action::Select(Section::Config), of(&workspace));
        terminal.draw(|frame| project.render(frame, &[]))?;
        let text = rendered(&terminal);
        for name in ["infra-ops", "research"] {
            let row = text
                .lines()
                .find(|row| row.contains(&format!("▸ {name}")))
                .ok_or("installed row missing")?;
            let row = row
                .chars()
                .skip(14)
                .take_while(|character| *character != '│')
                .collect::<String>();
            assert!(
                row.trim_end().ends_with("10 types  v1"),
                "{width}x{height}: {row}"
            );
            assert!(!row.contains(&format!("{name}-v1")));
            assert_eq!(row.contains(" default "), name == "infra-ops", "{row}");
        }
        assert!(
            !text.contains("conventional"),
            "excluded taxonomy must stay absent"
        );
    }
    Ok(())
}

#[test]
fn the_project_initialize_row_and_palette_share_the_editor_and_its_errors()
-> Result<(), Box<dyn std::error::Error>> {
    let mut workspace = HomeWorkspace::new(ProjectOverview::Uninitialized { root: fresh_root() });
    workspace.failure = Some("The project file appeared on disk.");
    let mut root = Root::new(InitialPage::Config, of(&workspace), &[]);
    let mut terminal = Terminal::new(TestBackend::new(87, 40))?;
    terminal.draw(|frame| root.render(frame, &[]))?;
    root.handle_event(click(Rect::new(14, 7, 73, 1)), of(&workspace), &[]);
    assert!(root.home.editing());
    root.handle_event(key(KeyCode::Esc), of(&workspace), &[]);
    root.execute(Action::Select(Section::Config), of(&workspace));
    root.handle_event(key(KeyCode::Char('/')), of(&workspace), &[]);
    terminal.draw(|frame| root.render(frame, &[]))?;
    let text = rendered(&terminal);
    assert!(!text.contains("Configure project"));
    assert!(!text.contains("Review changes"));
    root.handle_event(Event::Paste("initialize".into()), of(&workspace), &[]);
    root.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
    assert!(root.home.editing());
    for _ in 0..3 {
        root.handle_event(key(KeyCode::Down), of(&workspace), &[]);
    }
    root.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
    terminal.draw(|frame| root.render(frame, &[]))?;
    assert!(rendered(&terminal).contains("The project file appeared on disk."));
    assert!(root.home.editing());
    assert!(workspace.created.borrow().is_none());
    Ok(())
}

#[test]
fn config_reloads_instead_of_initializing_once_the_project_exists()
-> Result<(), Box<dyn std::error::Error>> {
    let workspace = HomeWorkspace::new(ProjectOverview::Uninitialized { root: fresh_root() });
    let mut stale = Root::new(InitialPage::Config, of(&workspace), &[]);
    let mut terminal = Terminal::new(TestBackend::new(87, 40))?;
    terminal.draw(|frame| stale.render(frame, &[]))?;
    assert!(!rendered(&terminal).contains("▸ conventional"));

    let (mut home, _) = home_screen(&workspace, 87, 40)?;
    home.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
    for _ in 0..3 {
        home.handle_event(key(KeyCode::Down), of(&workspace), &[]);
    }
    home.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
    let created = workspace
        .created
        .borrow()
        .clone()
        .ok_or("not initialized")?;

    assert_eq!(
        stale.handle_event(key(KeyCode::Enter), of(&workspace), &[]),
        None
    );
    assert_eq!(stale.section, Section::Config);
    assert!(!stale.home.editing());
    assert_eq!(workspace.created.borrow().as_ref(), Some(&created));
    terminal.draw(|frame| stale.render(frame, &[]))?;
    let text = rendered(&terminal);
    for name in ["conventional", "research", "infra-ops"] {
        assert!(text.contains(&format!("▸ {name}")), "{name}: {text}");
    }
    Ok(())
}
