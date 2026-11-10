use super::*;

#[test]
fn config_overview_tab_switches_sections_and_keeps_menu_selection()
-> Result<(), Box<dyn std::error::Error>> {
    let services = Services;
    let mut shell = Shell::new(InitialPage::Home, of(&services), &[]);
    shell.handle_event(key(KeyCode::Tab), of(&services), &[]);
    assert_eq!(shell.section, Section::Commit);
    assert!(shell.commit.blocker.is_some());
    shell.handle_event(key(KeyCode::Tab), of(&services), &[]);
    assert_eq!(shell.section, Section::Config);
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| shell.render(frame, &[]))?;
    let text = rendered(&terminal);
    assert!(text.contains("1  Project"));
    assert!(text.contains("tab nav bar  / commands  q quit"));
    shell.handle_event(key(KeyCode::Down), of(&services), &[]);
    terminal.draw(|frame| shell.render(frame, &[]))?;
    assert!(!rendered(&terminal).contains("saves as you go"));
    assert!(rendered(&terminal).contains("config.toml"));
    shell.handle_event(key(KeyCode::BackTab), of(&services), &[]);
    assert_eq!(shell.section, Section::Commit);
    shell.handle_event(key(KeyCode::Tab), of(&services), &[]);
    shell.handle_event(key(KeyCode::Tab), of(&services), &[]);
    assert_eq!(shell.section, Section::Help);
    shell.handle_event(key(KeyCode::BackTab), of(&services), &[]);
    assert_eq!(shell.section, Section::Config);
    terminal.draw(|frame| shell.render(frame, &[]))?;
    assert!(!rendered(&terminal).contains("saves as you go"));
    assert!(rendered(&terminal).contains("config.toml"));
    shell.handle_event(key(KeyCode::Esc), of(&services), &[]);
    assert_eq!(shell.section, Section::Home);
    Ok(())
}

#[test]
fn embedded_config_places_the_library_inspector_by_terminal_shape()
-> Result<(), Box<dyn std::error::Error>> {
    let services = Services;
    let mut shell = Shell::new(InitialPage::Config, of(&services), &[]);
    shell.handle_event(key(KeyCode::Char('2')), of(&services), &[]);
    let mut terminal = Terminal::new(TestBackend::new(87, 40))?;
    // Content starts right of the 13-column rail and its rule.
    let content = 14;
    for (width, height, stacked) in [
        (87, 40, true),
        (125, 51, true),
        (100, 30, false),
        (125, 42, true),
        (125, 41, false),
        (100, 34, true),
        (100, 33, false),
        (99, 18, true),
        (MINIMUM_WIDTH, MINIMUM_HEIGHT, true),
        (87, 23, true),
        (87, 40, true),
    ] {
        terminal.backend_mut().resize(width, height);
        terminal.resize(Rect::new(0, 0, width, height))?;
        terminal.draw(|frame| shell.render(frame, &[]))?;
        let buffer = terminal.backend().buffer();
        let text = |x: u16, y: u16, end: u16| {
            (x..end)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        };
        let dim = |x: u16, y: u16, symbol: &str| {
            buffer[(x, y)].symbol() == symbol && buffer[(x, y)].fg == theme().dim
        };
        // The shell header, a blank row from 24 rows, then the Library's title and file
        // level with the rail, and a blank row.
        let title = 1 + u16::from(height >= 24);
        assert_eq!(text(0, title, 13), " 1  Project  ", "{width}x{height}");
        let top = title + 3;
        assert!(text(content, title, width).starts_with(" Commit message taxonomy library "));
        assert!(text(content, top, width).starts_with(" ▸ conventional"));
        for y in 1..height - 1 {
            let row = text(content, y, width);
            assert!(
                !row.contains(['┌', '┐', '└', '┘']),
                "{width}x{height}: {row}"
            );
        }
        if stacked {
            // Three collapsed taxonomies and the new taxonomy row, then the rule across the
            // content.
            let rule = top + 4;
            assert!(
                (content..width).all(|x| dim(x, rule, "─")),
                "{width}x{height}"
            );
            assert!(text(content, rule + 1, width).starts_with(" Taxonomy conventional "));
        } else {
            // The outline column takes half the content and the Inspector starts at the top.
            let rule = content + (width - content) / 2;
            assert!((1..height - 1).all(|y| dim(rule, y, "│")));
            assert_eq!(
                text(content, title, rule).trim(),
                "Commit message taxonomy library"
            );
            assert!(text(rule + 1, title, width).starts_with(" Taxonomy conventional "));
        }
        assert_eq!(buffer[(1, height - 1)].symbol(), " ");
        assert!(!text(0, height - 1, width).contains("NAV"));
    }
    Ok(())
}

#[test]
fn the_config_tab_has_no_library_change_badge() -> Result<(), Box<dyn std::error::Error>> {
    let services = Services;
    let mut shell = Shell::new(InitialPage::Config, of(&services), &[]);
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| shell.render(frame, &[]))?;
    assert!(!rendered(&terminal).contains('●'));
    shell.handle_event(key(KeyCode::Char('2')), of(&services), &[]);
    shell.handle_event(key(KeyCode::Char('n')), of(&services), &[]);
    terminal.draw(|frame| shell.render(frame, &[]))?;
    let header = rendered(&terminal)
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned();
    assert!(header.contains(" Config  Help "), "{header:?}");
    let config = shell.tabs.area(Section::Config);
    let help = shell.tabs.area(Section::Help);
    assert_eq!(config.width, 6);
    assert_eq!(help.x, config.right() + 2);
    assert_eq!(
        header
            .chars()
            .skip(usize::from(config.x))
            .take(6)
            .collect::<String>(),
        "Config"
    );
    Ok(())
}

fn hidden_config_edit_pauses(event: Event) -> Result<(), Box<dyn std::error::Error>> {
    for (width, height) in [(MINIMUM_WIDTH - 1, 40), (87, MINIMUM_HEIGHT - 1)] {
        let services = Services;
        let mut shell = Shell::new(InitialPage::Config, of(&services), &[]);
        for event in [
            key(KeyCode::Char('2')),
            key(KeyCode::Char('n')),
            Event::Paste("Paused taxonomy".into()),
            key(KeyCode::Enter),
            Event::Paste("Paused purpose".into()),
        ] {
            assert!(shell.handle_event(event, of(&services), &[]).is_none());
        }
        let mut large = Terminal::new(TestBackend::new(87, 40))?;
        large.draw(|frame| shell.render(frame, &[]))?;
        let original = rendered(&large);
        assert!(original.contains("Paused taxonomy"));
        assert!(original.contains("Paused purpose"));
        let mut small = Terminal::new(TestBackend::new(width, height))?;
        small.draw(|frame| shell.render(frame, &[]))?;
        let warning = rendered(&small);
        assert!(
            shell
                .handle_event(event.clone(), of(&services), &[])
                .is_none()
        );
        large.draw(|frame| shell.render(frame, &[]))?;
        let resumed = rendered(&large);
        assert!(
            !resumed.contains("Test save unavailable"),
            "hidden {event:?} at {width}x{height} attempted a save"
        );
        let draft_rows = |text: &str| {
            text.lines()
                .filter(|line| line.contains("Paused"))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            draft_rows(&resumed),
            draft_rows(&original),
            "hidden {event:?} at {width}x{height} changed the draft"
        );
        assert!(shell.config.modal());
        assert_eq!(shell.config.shell_keys(), ShellKeys::None);
        assert!(warning.contains("Resize to continue"), "{warning}");
        assert!(!warning.contains("esc"), "{warning}");
        assert!(!warning.contains("quit"), "{warning}");
        assert!(
            shell
                .handle_event(
                    Event::Key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
                    of(&services),
                    &[]
                )
                .is_none()
        );
        large.draw(|frame| shell.render(frame, &[]))?;
        assert!(rendered(&large).contains("Test save unavailable"));
    }
    Ok(())
}

#[test]
fn hidden_config_edit_pauses_escape() -> Result<(), Box<dyn std::error::Error>> {
    hidden_config_edit_pauses(key(KeyCode::Esc))
}

#[test]
fn hidden_config_edit_pauses_save() -> Result<(), Box<dyn std::error::Error>> {
    hidden_config_edit_pauses(Event::Key(KeyEvent::new(
        KeyCode::Char('w'),
        KeyModifiers::CONTROL,
    )))
}

#[test]
fn hidden_config_edit_pauses_q() -> Result<(), Box<dyn std::error::Error>> {
    hidden_config_edit_pauses(key(KeyCode::Char('q')))
}

#[test]
fn hidden_config_edit_pauses_text_key() -> Result<(), Box<dyn std::error::Error>> {
    hidden_config_edit_pauses(key(KeyCode::Char('x')))
}

#[test]
fn hidden_config_edit_pauses_paste() -> Result<(), Box<dyn std::error::Error>> {
    hidden_config_edit_pauses(Event::Paste("hidden paste".into()))
}

#[test]
fn config_edit_locks_tabs_and_gives_esc_and_q_to_the_field() {
    let services = Services;
    let mut shell = Shell::new(InitialPage::Config, of(&services), &[]);
    shell.handle_event(key(KeyCode::Char('2')), of(&services), &[]);
    shell.handle_event(key(KeyCode::Char('n')), of(&services), &[]);
    assert_eq!(shell.config.shell_keys(), ShellKeys::None);
    shell.handle_event(key(KeyCode::Char('q')), of(&services), &[]);
    assert_eq!(shell.section, Section::Config);
    assert!(shell.overlay.is_none());
    for code in [KeyCode::Tab, KeyCode::BackTab] {
        shell.handle_event(key(code), of(&services), &[]);
        assert_eq!(shell.section, Section::Config);
        assert_eq!(shell.config.shell_keys(), ShellKeys::None);
    }
    shell.handle_event(key(KeyCode::Esc), of(&services), &[]);
    assert_eq!(shell.section, Section::Config);
    assert_eq!(shell.config.shell_keys(), ShellKeys::Navigation);
    shell.handle_event(key(KeyCode::Tab), of(&services), &[]);
    assert_eq!(shell.section, Section::Help);
    shell.handle_event(key(KeyCode::BackTab), of(&services), &[]);
    assert_eq!(shell.section, Section::Config);
    assert_eq!(
        shell.handle_event(key(KeyCode::Char('q')), of(&services), &[]),
        Some(Action::Quit)
    );
}

#[test]
fn config_palette_quit_uses_one_shell_discard_confirmation()
-> Result<(), Box<dyn std::error::Error>> {
    let services = Services;
    let mut shell = rejected_review_on(Section::Config)?;
    shell.handle_event(key(KeyCode::Char('/')), of(&services), &[]);
    shell.handle_event(Event::Paste("quit".into()), of(&services), &[]);
    shell.handle_event(key(KeyCode::Enter), of(&services), &[]);
    assert!(matches!(shell.overlay, Some(Overlay::QuitConfirm)));
    assert_eq!(shell.config.shell_keys(), ShellKeys::Navigation);
    assert_eq!(
        shell.handle_event(key(KeyCode::Char('y')), of(&services), &[]),
        Some(Action::Quit)
    );
    Ok(())
}

#[test]
fn config_palette_quit_without_a_pending_review_quits() {
    let services = Services;
    let mut shell = Shell::new(InitialPage::Config, of(&services), &[]);
    shell.handle_event(key(KeyCode::Char('/')), of(&services), &[]);
    shell.handle_event(Event::Paste("quit".into()), of(&services), &[]);
    assert_eq!(
        shell.handle_event(key(KeyCode::Enter), of(&services), &[]),
        Some(Action::Quit)
    );
}
