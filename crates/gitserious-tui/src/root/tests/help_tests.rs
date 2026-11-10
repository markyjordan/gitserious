use super::*;

#[test]
fn question_mark_does_not_open_help_but_the_help_tab_still_works() {
    let services = Services;
    let mut root = Root::new(InitialPage::Home, of(&services), &[]);
    root.handle_event(key(KeyCode::Char('?')), of(&services), &[]);
    assert_eq!(root.section, Section::Home);
    assert!(root.overlay.is_none());
    root.handle_event(key(KeyCode::BackTab), of(&services), &[]);
    assert_eq!(root.section, Section::Help);
}

#[test]
fn the_overview_opens_on_its_help_and_bolds_only_headings_and_names()
-> Result<(), Box<dyn std::error::Error>> {
    let services = Services;
    let mut root = Root::new(InitialPage::Home, of(&services), &[]);
    let help = vec![HelpPage::new("Overview", None, |_| {
        vec![
            vec![span("Create durable commit-message policy", false)],
            vec![],
            vec![
                span("Usage:", true),
                span(" ", false),
                span("gitserious", true),
                span(" [COMMAND]", false),
            ],
            vec![],
            vec![span("Options:", true)],
            vec![
                span("  ", false),
                span("-h, --help", true),
                span("  Display the concise help", false),
            ],
        ]
    })];
    root.handle_event(key(KeyCode::BackTab), of(&services), &help);
    let mut terminal = Terminal::new(TestBackend::new(66, 18))?;
    terminal.draw(|frame| root.render(frame, &help))?;

    let text = rendered(&terminal);
    assert!(!text.contains("┌ Help"), "{text}");
    assert!(!text.contains("gitserious help:"), "{text}");
    let (x, y) = position(&terminal, "Create durable").ok_or("missing description")?;
    assert_eq!(y, 2, "one blank row under the header");
    let buffer = terminal.backend().buffer();
    assert_eq!(
        buffer[(x - 2, y)].symbol(),
        "│",
        "one column right of the rule"
    );
    assert_eq!(buffer[(x - 1, y)].symbol(), " ");
    assert_eq!(position(&terminal, "Usage:"), Some((x, y + 2)));
    for needle in ["Usage:", "gitserious [", "Options:", "-h, --help"] {
        let (x, y) = position(&terminal, needle).ok_or(needle)?;
        assert!(buffer[(x, y)].modifier.contains(Modifier::BOLD), "{needle}");
        assert_eq!(buffer[(x, y)].fg, theme().text, "{needle}");
    }
    for (needle, color) in [("[COMMAND]", theme().text), ("Display", theme().dim)] {
        let (x, y) = position(&terminal, needle).ok_or(needle)?;
        assert!(
            !buffer[(x, y)].modifier.contains(Modifier::BOLD),
            "{needle}"
        );
        assert_eq!(buffer[(x, y)].fg, color, "{needle}");
    }
    Ok(())
}

#[test]
fn command_pages_open_with_their_help_command_and_no_prompt()
-> Result<(), Box<dyn std::error::Error>> {
    let services = Services;
    let mut root = Root::new(InitialPage::Home, of(&services), &[]);
    let help = pages(1);
    root.handle_event(key(KeyCode::BackTab), of(&services), &help);
    let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
    for (digit, command) in [
        ('1', None),
        ('2', Some("init")),
        ('3', Some("config")),
        ('4', Some("commit")),
    ] {
        root.handle_event(key(KeyCode::Char(digit)), of(&services), &help);
        terminal.draw(|frame| root.render(frame, &help))?;
        let text = rendered(&terminal);
        assert!(!text.contains("$ "), "{text}");
        let Some(command) = command else {
            assert_eq!(top(&terminal).as_deref(), Some("line 1"), "{text}");
            continue;
        };

        let header = format!("gitserious help: {command}");
        assert_eq!(top(&terminal).as_deref(), Some(header.as_str()), "{text}");
        let (x, y) = position(&terminal, &header).ok_or("missing header")?;
        assert_eq!(position(&terminal, "line 1"), Some((x, y + 2)), "{text}");
        let buffer = terminal.backend().buffer();
        for (column, color) in [(x, theme().dim), (x + 17, theme().text)] {
            assert_eq!(buffer[(column, y)].fg, color, "{header} at {column}");
            assert!(!buffer[(column, y)].modifier.contains(Modifier::BOLD));
        }
    }
    Ok(())
}

#[test]
fn help_keeps_a_wrapped_description_under_its_column() -> Result<(), Box<dyn std::error::Error>> {
    const DESCRIPTION: &str = "Author and create a durable commit from the staged index";
    let listing = |width: u16| {
        let room = usize::from(width) - 10;
        let mut rows = vec![String::new()];
        for word in DESCRIPTION.split(' ') {
            let row = rows.last().map_or(0, String::len);
            if row > 0 && row + 1 + word.len() > room {
                rows.push(String::new());
            }
            if let Some(row) = rows.last_mut() {
                if !row.is_empty() {
                    row.push(' ');
                }
                row.push_str(word);
            }
        }
        rows.iter()
            .enumerate()
            .map(|(index, row)| {
                if index == 0 {
                    vec![
                        span("  ", false),
                        span("commit", true),
                        span("  ", false),
                        span(row, false),
                    ]
                } else {
                    vec![span(&format!("{:10}{row}", ""), false)]
                }
            })
            .collect()
    };
    let services = Services;
    let mut root = Root::new(InitialPage::Home, of(&services), &[]);
    let help = vec![HelpPage::new("Overview", None, listing)];
    root.handle_event(key(KeyCode::BackTab), of(&services), &help);
    let mut terminal = Terminal::new(TestBackend::new(66, 18))?;
    terminal.draw(|frame| root.render(frame, &help))?;

    let text = rendered(&terminal);
    let (left, _) = position(&terminal, "  commit  Author").ok_or("missing listing")?;
    let rows: Vec<String> = text
        .lines()
        .map(|row| row.chars().skip(usize::from(left)).collect())
        .collect();
    let (column, y) = position(&terminal, "Author").ok_or("missing description")?;
    let continuation = &rows[usize::from(y) + 1];
    assert_eq!(
        continuation.len() - continuation.trim_start().len(),
        usize::from(column - left),
        "{text}"
    );
    assert_eq!(
        format!(
            "{} {}",
            rows[usize::from(y)].trim_end(),
            continuation.trim()
        ),
        format!("  commit  {DESCRIPTION}"),
        "{text}"
    );
    Ok(())
}

#[test]
fn help_lists_its_pages_beside_a_rule_and_highlights_the_chosen_one()
-> Result<(), Box<dyn std::error::Error>> {
    let services = Services;
    let mut root = Root::new(InitialPage::Home, of(&services), &[]);
    let help = pages(1);
    root.handle_event(key(KeyCode::BackTab), of(&services), &help);
    let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
    terminal.draw(|frame| root.render(frame, &help))?;

    let text = rendered(&terminal);
    for (index, label) in ["1  Overview", "2  Init", "3  Config", "4  Commit"]
        .into_iter()
        .enumerate()
    {
        let y = 2 + u16::try_from(index)?;
        assert_eq!(position(&terminal, label), Some((1, y)), "{text}");
    }
    // The widest label and one column on each side.
    let rule = 1 + 11 + 1;
    let fifth: String = text
        .lines()
        .nth(6)
        .unwrap_or_default()
        .chars()
        .take(rule)
        .collect();
    assert_eq!(fifth.trim(), "", "no fifth page: {text}");
    let rule = u16::try_from(rule)?;
    let buffer = terminal.backend().buffer();
    for y in 1..MINIMUM_HEIGHT - FOOTER_HEIGHT {
        assert_eq!(buffer[(rule, y)].symbol(), "│", "row {y}");
        assert_eq!(buffer[(rule, y)].fg, theme().dim, "row {y}");
    }
    assert_eq!(position(&terminal, "line 1"), Some((rule + 2, 2)));

    for x in 0..rule {
        let cell = &buffer[(x, 2)];
        assert_eq!((cell.fg, cell.bg), (theme().ground, theme().text), "{x}");
        assert!(cell.modifier.contains(Modifier::BOLD), "{x}");
    }
    let (x, y) = position(&terminal, "2  Init").ok_or("missing Init")?;
    assert_eq!(
        (buffer[(x, y)].fg, buffer[(x, y)].bg),
        (theme().dim, theme().ground)
    );
    assert!(
        footer(&terminal).starts_with(" → read  ↑↓ pages  1-4 jump "),
        "{}",
        footer(&terminal)
    );
    Ok(())
}

#[test]
fn pages_focus_arrows_digits_and_clicks_choose_the_page_without_scrolling()
-> Result<(), Box<dyn std::error::Error>> {
    let services = Services;
    let mut root = Root::new(InitialPage::Home, of(&services), &[]);
    let help = pages(100);
    root.handle_event(key(KeyCode::BackTab), of(&services), &help);
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| root.render(frame, &help))?;
    assert_eq!(top(&terminal).as_deref(), Some("line 1"));

    for code in [KeyCode::Right, KeyCode::Down, KeyCode::Down, KeyCode::Left] {
        root.handle_event(key(code), of(&services), &help);
    }
    terminal.draw(|frame| root.render(frame, &help))?;
    let scrolled = rendered(&terminal);
    assert_eq!(
        top(&terminal).as_deref(),
        Some("line 3"),
        "reading scrolled the page"
    );
    root.handle_event(key(KeyCode::Up), of(&services), &help);
    terminal.draw(|frame| root.render(frame, &help))?;
    assert_eq!(
        rendered(&terminal),
        scrolled,
        "up on the first page scrolls nothing"
    );

    root.handle_event(key(KeyCode::Down), of(&services), &help);
    terminal.draw(|frame| root.render(frame, &help))?;
    assert_eq!(top(&terminal).as_deref(), Some("gitserious help: init"));
    root.handle_event(key(KeyCode::Up), of(&services), &help);
    terminal.draw(|frame| root.render(frame, &help))?;
    assert_eq!(
        top(&terminal).as_deref(),
        Some("line 1"),
        "a new page starts at its top"
    );

    root.handle_event(key(KeyCode::Char('3')), of(&services), &help);
    terminal.draw(|frame| root.render(frame, &help))?;
    assert_eq!(top(&terminal).as_deref(), Some("gitserious help: config"));
    root.handle_event(key(KeyCode::Char('5')), of(&services), &help);
    terminal.draw(|frame| root.render(frame, &help))?;
    assert_eq!(top(&terminal).as_deref(), Some("gitserious help: config"));

    root.handle_event(key(KeyCode::Right), of(&services), &help);
    let (x, y) = position(&terminal, "4  Commit").ok_or("missing Commit")?;
    root.handle_event(click(Rect::new(x, y, 1, 1)), of(&services), &help);
    terminal.draw(|frame| root.render(frame, &help))?;
    assert_eq!(top(&terminal).as_deref(), Some("gitserious help: commit"));
    assert!(footer(&terminal).contains("→ read  ↑↓ pages  1-4 jump"));
    assert_eq!(terminal.backend().buffer()[(x, y)].bg, theme().text);
    root.handle_event(key(KeyCode::Down), of(&services), &help);
    terminal.draw(|frame| root.render(frame, &help))?;
    assert_eq!(
        top(&terminal).as_deref(),
        Some("gitserious help: commit"),
        "the last page stays chosen"
    );
    Ok(())
}

#[test]
fn reading_focus_scrolls_by_line_and_by_screen() -> Result<(), Box<dyn std::error::Error>> {
    let services = Services;
    let mut root = Root::new(InitialPage::Home, of(&services), &[]);
    let help = pages(100);
    root.handle_event(key(KeyCode::BackTab), of(&services), &help);
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| root.render(frame, &help))?;
    // The numbered line at the top of the content, if any.
    let mut press = |code| -> Result<Option<u16>, Box<dyn std::error::Error>> {
        root.handle_event(key(code), of(&services), &help);
        terminal.draw(|frame| root.render(frame, &help))?;
        Ok(top(&terminal)
            .as_deref()
            .and_then(|row| row.strip_prefix("line "))
            .and_then(|number| number.parse().ok()))
    };

    assert_eq!(press(KeyCode::Right)?, Some(1), "reading starts at the top");
    assert_eq!(press(KeyCode::Down)?, Some(2));
    // 30 rows less the header, the row under it, and the footer.
    assert_eq!(press(KeyCode::PageDown)?, Some(2 + 27));
    assert_eq!(press(KeyCode::PageUp)?, Some(2));
    assert_eq!(press(KeyCode::Up)?, Some(1));
    assert_eq!(press(KeyCode::Up)?, Some(1), "the top stops scrolling");

    let buffer = terminal.backend().buffer();
    let (x, y) = position(&terminal, "1  Overview").ok_or("missing Overview")?;
    assert_eq!(
        (buffer[(x, y)].fg, buffer[(x, y)].bg),
        (theme().text, theme().ground)
    );
    assert!(
        buffer[(x, y)]
            .modifier
            .contains(Modifier::BOLD | Modifier::UNDERLINED)
    );
    assert_eq!(buffer[(x - 1, y)].bg, theme().ground, "no highlight bar");
    let (x, y) = position(&terminal, "2  Init").ok_or("missing Init")?;
    assert_eq!(buffer[(x, y)].fg, theme().dim);
    assert!(
        footer(&terminal).starts_with(" ↑↓ scroll  pgup/pgdn screen  ← pages "),
        "{}",
        footer(&terminal)
    );

    root.handle_event(key(KeyCode::Left), of(&services), &help);
    terminal.draw(|frame| root.render(frame, &help))?;
    assert!(footer(&terminal).contains("→ read  ↑↓ pages  1-4 jump"));
    root.handle_event(key(KeyCode::Enter), of(&services), &help);
    terminal.draw(|frame| root.render(frame, &help))?;
    assert!(footer(&terminal).contains("↑↓ scroll"), "enter reads too");
    Ok(())
}

#[test]
fn help_reads_to_its_last_line_and_keeps_the_way_back_at_the_minimum_size()
-> Result<(), Box<dyn std::error::Error>> {
    let services = Services;
    let mut root = Root::new(InitialPage::Home, of(&services), &[]);
    let help = pages(40);
    root.handle_event(key(KeyCode::BackTab), of(&services), &help);
    let mut terminal = Terminal::new(TestBackend::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))?;
    terminal.draw(|frame| root.render(frame, &help))?;
    assert!(!rendered(&terminal).contains("line 40"));
    let (left, _) = position(&terminal, "line 1").ok_or("missing first line")?;
    assert!(footer(&terminal).contains("→ read  ↑↓ pages  1-4 jump"));

    root.handle_event(key(KeyCode::Right), of(&services), &help);
    for _ in 0..2 {
        root.handle_event(key(KeyCode::PageDown), of(&services), &help);
        terminal.draw(|frame| root.render(frame, &help))?;
    }
    let last = MINIMUM_HEIGHT - FOOTER_HEIGHT - 1;
    assert_eq!(position(&terminal, "line 40"), Some((left, last)));
    let hints = footer(&terminal);
    assert!(hints.contains("↑↓ scroll  ← pages"), "{hints}");
    assert!(hints.contains("tab nav bar  / commands  q quit"), "{hints}");

    root.handle_event(key(KeyCode::Up), of(&services), &help);
    terminal.draw(|frame| root.render(frame, &help))?;
    assert_eq!(position(&terminal, "line 39"), Some((left, last)));
    Ok(())
}

#[test]
fn a_help_entry_opens_help_in_the_sidebar_on_its_command_page()
-> Result<(), Box<dyn std::error::Error>> {
    let services = Services;
    let help = pages(1);
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    for (command, label, top_line) in [
        (Some("config"), "3  Config", "gitserious help: config"),
        (None, "1  Overview", "line 1"),
        (Some("help"), "1  Overview", "line 1"),
    ] {
        let entry = InitialPage::Help {
            command: command.map(str::to_owned),
        };
        let mut root = Root::new(entry, of(&services), &help);
        terminal.draw(|frame| root.render(frame, &help))?;

        assert_eq!(root.section, Section::Help, "{command:?}");
        assert_eq!(top(&terminal).as_deref(), Some(top_line), "{command:?}");
        let (x, y) = position(&terminal, label).ok_or(label)?;
        assert_eq!(terminal.backend().buffer()[(x, y)].bg, theme().text);
        assert!(
            footer(&terminal).starts_with(" → read  ↑↓ pages  1-4 jump "),
            "{}",
            footer(&terminal)
        );
        root.handle_event(key(KeyCode::Esc), of(&services), &help);
        assert_eq!(root.section, Section::Home, "esc goes Home");
        assert_eq!(
            root.handle_event(key(KeyCode::Char('q')), of(&services), &help),
            Some(Action::Quit)
        );
    }
    Ok(())
}

#[test]
fn returning_to_help_starts_in_the_sidebar_on_the_page_left_last()
-> Result<(), Box<dyn std::error::Error>> {
    let services = Services;
    let mut root = Root::new(InitialPage::Home, of(&services), &[]);
    let help = pages(1);
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    root.handle_event(key(KeyCode::BackTab), of(&services), &help);
    root.handle_event(key(KeyCode::Char('3')), of(&services), &help);
    root.handle_event(key(KeyCode::Right), of(&services), &help);
    root.handle_event(key(KeyCode::Esc), of(&services), &help);
    assert_eq!(
        root.section,
        Section::Home,
        "esc still goes Home while reading"
    );

    let palette = [KeyCode::Char('/')]
        .into_iter()
        .map(key)
        .chain([Event::Paste("open help".into()), key(KeyCode::Enter)]);
    for event in palette {
        root.handle_event(event, of(&services), &help);
    }
    assert_eq!(root.section, Section::Help);
    terminal.draw(|frame| root.render(frame, &help))?;
    assert_eq!(top(&terminal).as_deref(), Some("gitserious help: config"));
    assert!(footer(&terminal).contains("→ read  ↑↓ pages  1-4 jump"));
    let (x, y) = position(&terminal, "3  Config").ok_or("missing Config")?;
    assert_eq!(terminal.backend().buffer()[(x, y)].bg, theme().text);

    root.handle_event(key(KeyCode::Right), of(&services), &help);
    root.handle_event(key(KeyCode::Tab), of(&services), &help);
    assert_eq!(
        root.section,
        Section::Home,
        "tab still leaves while reading"
    );
    root.handle_event(click(root.tabs.area(Section::Help)), of(&services), &help);
    terminal.draw(|frame| root.render(frame, &help))?;
    assert!(footer(&terminal).contains("→ read  ↑↓ pages  1-4 jump"));
    root.handle_event(key(KeyCode::Right), of(&services), &help);
    assert_eq!(
        root.handle_event(key(KeyCode::Char('q')), of(&services), &help),
        Some(Action::Quit)
    );
    Ok(())
}
