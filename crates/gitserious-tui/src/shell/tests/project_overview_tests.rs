use super::*;

#[test]
fn an_uninitialized_project_matches_the_open_bone_overview()
-> Result<(), Box<dyn std::error::Error>> {
    let workspace = HomeWorkspace::new(HomeState::Uninitialized { root: fresh_root() });
    let mut shell = Shell::new(InteractiveEntry::Config, &workspace, &[]);
    for (width, height) in [(87, 40), (100, 30), (125, 51), (66, 18)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
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
        let workspace = HomeWorkspace::new(HomeState::Uninitialized { root: fresh_root() });
        let mut project = Shell::new(InteractiveEntry::Config, &workspace, &[]);
        let mut home = Shell::new(InteractiveEntry::Home, &workspace, &[]);
        let mut terminal = Terminal::new(TestBackend::new(width, height))?;
        project.handle_event(key(KeyCode::Enter), &workspace, &[]);
        home.handle_event(key(KeyCode::Enter), &workspace, &[]);
        assert_eq!(project.section, Section::Home);
        terminal.draw(|frame| project.render(frame, &[]))?;
        let project_frame = terminal.backend().buffer().clone();
        terminal.draw(|frame| home.render(frame, &[]))?;
        assert_eq!(&project_frame, terminal.backend().buffer());
        assert!(workspace.created.borrow().is_none());
        project.handle_event(key(KeyCode::Esc), &workspace, &[]);
        assert!(project.init_editor.is_none());
        assert!(workspace.created.borrow().is_none());
        project.execute(Action::Select(Section::Config), &workspace);
        project.handle_event(key(KeyCode::Enter), &workspace, &[]);
        // Exclude conventional and make infra-ops the default; only those choices may
        // appear in Project after the shell reloads the initialized configuration.
        project.handle_event(key(KeyCode::Char(' ')), &workspace, &[]);
        project.handle_event(key(KeyCode::Down), &workspace, &[]);
        project.handle_event(key(KeyCode::Down), &workspace, &[]);
        project.handle_event(key(KeyCode::Enter), &workspace, &[]);
        for _ in 0..3 {
            project.handle_event(key(KeyCode::Down), &workspace, &[]);
        }
        project.handle_event(key(KeyCode::Enter), &workspace, &[]);
        assert!(project.init_editor.is_none());
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
                .map(|taxonomy| taxonomy.versioned().to_string())
                .collect::<Vec<_>>(),
            ["infra-ops-v1", "research-v1"]
        );
        project.execute(Action::Select(Section::Config), &workspace);
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
        }
        assert!(
            !text.contains("conventional"),
            "excluded taxonomy must stay absent"
        );
        assert!(!project.config.dirty());
    }
    Ok(())
}

#[test]
fn the_project_initialize_row_and_palette_share_the_editor_and_its_errors()
-> Result<(), Box<dyn std::error::Error>> {
    let mut workspace = HomeWorkspace::new(HomeState::Uninitialized { root: fresh_root() });
    workspace.failure = Some("The project file appeared on disk.");
    let mut shell = Shell::new(InteractiveEntry::Config, &workspace, &[]);
    let mut terminal = Terminal::new(TestBackend::new(87, 40))?;
    terminal.draw(|frame| shell.render(frame, &[]))?;
    shell.handle_event(click(Rect::new(14, 7, 73, 1)), &workspace, &[]);
    assert!(shell.init_editor.is_some());
    shell.handle_event(key(KeyCode::Esc), &workspace, &[]);
    shell.execute(Action::Select(Section::Config), &workspace);
    shell.handle_event(key(KeyCode::Char('/')), &workspace, &[]);
    terminal.draw(|frame| shell.render(frame, &[]))?;
    let text = rendered(&terminal);
    assert!(!text.contains("Configure project"));
    assert!(!text.contains("Review changes"));
    shell.handle_event(Event::Paste("initialize".into()), &workspace, &[]);
    shell.handle_event(key(KeyCode::Enter), &workspace, &[]);
    assert!(shell.init_editor.is_some());
    for _ in 0..3 {
        shell.handle_event(key(KeyCode::Down), &workspace, &[]);
    }
    shell.handle_event(key(KeyCode::Enter), &workspace, &[]);
    terminal.draw(|frame| shell.render(frame, &[]))?;
    assert!(rendered(&terminal).contains("The project file appeared on disk."));
    assert!(shell.init_editor.is_some());
    assert!(workspace.created.borrow().is_none());
    Ok(())
}
