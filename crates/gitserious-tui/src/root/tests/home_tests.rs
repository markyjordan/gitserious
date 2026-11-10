use super::*;

#[test]
fn home_names_the_directory_and_offers_only_the_step_its_state_allows()
-> Result<(), Box<dyn std::error::Error>> {
    let root = fresh_root();
    let path = root.as_path().display().to_string();
    let initialized = |policy_untracked| ProjectOverview::Initialized {
        root: fresh_root(),
        default: gitserious_core::built_in_taxonomies()[0].id().clone(),
        version: Some(TaxonomyVersion::V1),
        policy_untracked,
    };
    for (width, height) in [(87, 40), (100, 30)] {
        let workspace = HomeWorkspace::new(ProjectOverview::NoRepository {
            directory: root.as_path().to_path_buf(),
        });
        let (_, terminal) = home_screen(&workspace, width, height)?;
        let text = rendered(&terminal);
        let hint = row_of(&text, "no git repository found").ok_or("missing hint")?;
        let column = text
            .lines()
            .nth(hint)
            .unwrap_or_default()
            .find("no git")
            .unwrap_or(0);
        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer[(u16::try_from(column)?, u16::try_from(hint)?)].fg,
            theme().pending
        );
        assert!(row_of(&text, &path).is_some(), "{text}");
        let action = row_of(&text, " initialize git and gitserious ").ok_or("missing action")?;
        assert_eq!(action, hint + 2, "{text}");
        assert!(
            text.lines()
                .nth(action)
                .unwrap_or_default()
                .trim_end()
                .ends_with("enter")
        );
        assert!(
            text.lines()
                .last()
                .unwrap_or_default()
                .starts_with(" enter initialize")
        );

        let workspace = HomeWorkspace::new(ProjectOverview::Uninitialized { root: fresh_root() });
        let (_, terminal) = home_screen(&workspace, width, height)?;
        let text = rendered(&terminal);
        let rows: Vec<&str> = text.lines().collect();
        let name = row_of(&text, "fresh").ok_or("missing name")?;
        assert!(rows[name + 1].contains(&path));
        let action = row_of(&text, " initialize gitserious ").ok_or("missing action")?;
        assert_eq!(action, name + 3);
        assert!(rows[action].trim_end().ends_with("enter"));
        let wordmark = row_of(&text, "▄████ ███").ok_or("missing wordmark")?;
        let left = rows[wordmark].chars().take_while(|c| *c == ' ').count();
        assert_eq!(rows[name].chars().take_while(|c| *c == ' ').count(), left);
        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer[(u16::try_from(left)?, u16::try_from(action)?)].bg,
            theme().cursor
        );
        assert_eq!(
            buffer[(u16::try_from(left + 48)?, u16::try_from(action)?)].bg,
            theme().cursor,
            "the action row spans the wordmark"
        );
        assert!(
            !text.contains("gitserious.toml"),
            "no hint about the missing file"
        );
        assert!(!text.contains("no git repository"));
        let content_rows = usize::from(height) - 2;
        let block = action - wordmark + 1;
        assert!(
            (wordmark - 1).abs_diff((content_rows - block) / 2) <= 1,
            "the block is centered vertically"
        );
        assert!(rows[usize::from(height) - 1].starts_with(" enter initialize"));

        let workspace = HomeWorkspace::new(initialized(true));
        let (_, terminal) = home_screen(&workspace, width, height)?;
        let text = rendered(&terminal);
        let rows: Vec<&str> = text.lines().collect();
        let done = row_of(&text, "✓ fresh is initialized with conventional-v1")
            .ok_or("missing initialized line")?;
        assert_eq!(
            row_of(&text, "◐ gitserious.toml is untracked."),
            Some(done + 2)
        );
        assert!(rows[done + 3].contains("Commit it so the policy travels with the repo:"));
        assert!(rows[done + 4].contains("git add gitserious.toml"));
        assert_eq!(row_of(&text, " Go to Commit "), Some(done + 6));
        assert!(rows[usize::from(height) - 1].starts_with(" enter commit"));

        let workspace = HomeWorkspace::new(initialized(false));
        let (_, terminal) = home_screen(&workspace, width, height)?;
        let text = rendered(&terminal);
        let done = row_of(&text, "is initialized with").ok_or("missing line")?;
        assert!(!text.contains("untracked"));
        assert_eq!(row_of(&text, " Go to Commit "), Some(done + 2));

        let workspace = HomeWorkspace::new(ProjectOverview::Unavailable {
            directory: root.as_path().to_path_buf(),
            error: "git is not installed".into(),
        });
        let (_, terminal) = home_screen(&workspace, width, height)?;
        let text = rendered(&terminal);
        let error = row_of(&text, "✗ git is not installed").ok_or("missing error")?;
        let column = text
            .lines()
            .nth(error)
            .unwrap_or_default()
            .find('✗')
            .unwrap_or(0);
        let column = text.lines().nth(error).unwrap_or_default()[..column]
            .chars()
            .count();
        let buffer = terminal.backend().buffer();
        assert_eq!(
            buffer[(u16::try_from(column)?, u16::try_from(error)?)].fg,
            theme().blocks
        );
        assert!(!text.contains("enter "));
    }
    Ok(())
}

#[test]
fn enter_opens_the_editor_whose_initialize_row_writes_the_chosen_taxonomies()
-> Result<(), Box<dyn std::error::Error>> {
    let workspace = HomeWorkspace::new(ProjectOverview::Uninitialized { root: fresh_root() });
    let (mut root, mut terminal) = home_screen(&workspace, 87, 40)?;
    root.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
    terminal.draw(|frame| root.render(frame, &[]))?;
    let text = rendered(&terminal);
    assert!(text.contains(" initialize fresh"), "{text}");
    assert!(text.contains("esc back  space include  enter default"));
    root.handle_event(key(KeyCode::Esc), of(&workspace), &[]);
    assert!(workspace.created.borrow().is_none(), "esc writes nothing");
    assert_eq!(
        root.section,
        Section::Home,
        "esc closes the editor, not the root"
    );
    terminal.draw(|frame| root.render(frame, &[]))?;
    assert!(rendered(&terminal).contains(" initialize gitserious "));

    root.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
    root.handle_event(key(KeyCode::Tab), of(&workspace), &[]);
    assert_eq!(root.section, Section::Commit, "tab leaves the open editor");
    root.handle_event(key(KeyCode::BackTab), of(&workspace), &[]);
    terminal.draw(|frame| root.render(frame, &[]))?;
    assert!(
        rendered(&terminal).contains(" initialize fresh"),
        "the editor survives a visit to another section"
    );
    for _ in 0..3 {
        root.handle_event(key(KeyCode::Down), of(&workspace), &[]);
    }
    terminal.draw(|frame| root.render(frame, &[]))?;
    assert!(rendered(&terminal).contains("enter initialize"));
    root.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
    let created = workspace
        .created
        .borrow()
        .clone()
        .ok_or("nothing initialized")?;
    assert_eq!(created.default_taxonomy().as_str(), "conventional");
    assert_eq!(
        created
            .taxonomies()
            .iter()
            .map(|taxonomy| taxonomy.reference().to_string())
            .collect::<Vec<_>>(),
        ["conventional-v1", "research-v1", "infra-ops-v1"]
    );
    assert_eq!(root.section, Section::Home);
    assert_eq!(root.home.action(), Some(HomeAction::GoToCommit));
    root.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
    assert_eq!(root.section, Section::Commit);
    Ok(())
}

#[test]
fn library_taxonomies_start_excluded_and_an_unreadable_library_is_named()
-> Result<(), Box<dyn std::error::Error>> {
    let built_in = &gitserious_core::built_in_taxonomies()[0];
    let team = gitserious_core::Taxonomy::new(
        gitserious_core::TaxonomyId::new("team-changes")?,
        Some(TaxonomyVersion::new(3)?),
        built_in.description().clone(),
        None,
        built_in.commit_types()[..3].to_vec(),
    )?;
    let mut workspace = HomeWorkspace::new(ProjectOverview::Uninitialized { root: fresh_root() });
    workspace.library = Ok(vec![team]);
    let (mut root, mut terminal) = home_screen(&workspace, 87, 40)?;
    root.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
    terminal.draw(|frame| root.render(frame, &[]))?;
    let text = rendered(&terminal);
    let row = text
        .lines()
        .find(|row| row.contains("team-changes"))
        .ok_or("missing Library taxonomy")?;
    assert!(row.starts_with(" ○ –  team-changes  custom"), "{row}");
    assert!(row.trim_end().ends_with("3 types"));
    for code in [
        KeyCode::Down,
        KeyCode::Down,
        KeyCode::Down,
        KeyCode::Char(' '),
    ] {
        root.handle_event(key(code), of(&workspace), &[]);
    }
    for _ in 0..4 {
        root.handle_event(key(KeyCode::Down), of(&workspace), &[]);
    }
    root.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
    let created = workspace
        .created
        .borrow()
        .clone()
        .ok_or("nothing initialized")?;
    assert_eq!(
        created
            .taxonomies()
            .iter()
            .map(|taxonomy| taxonomy.reference().to_string())
            .collect::<Vec<_>>(),
        [
            "conventional-v1",
            "research-v1",
            "infra-ops-v1",
            "team-changes"
        ]
    );

    let mut workspace = HomeWorkspace::new(ProjectOverview::Uninitialized { root: fresh_root() });
    workspace.library = Err("config.toml is not valid TOML".into());
    let (mut root, mut terminal) = home_screen(&workspace, 87, 40)?;
    root.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
    terminal.draw(|frame| root.render(frame, &[]))?;
    let text = rendered(&terminal);
    assert!(
        text.contains("✗ Library taxonomies are unavailable: config.toml is not valid"),
        "{text}"
    );
    assert!(
        text.contains("✓ 3  infra-ops"),
        "the built-ins stay available"
    );
    Ok(())
}

#[test]
fn a_failed_initialization_shows_why_and_keeps_the_offer() -> Result<(), Box<dyn std::error::Error>>
{
    let mut workspace = HomeWorkspace::new(ProjectOverview::Uninitialized { root: fresh_root() });
    workspace.failure = Some("gitserious.toml already exists");
    let (mut root, mut terminal) = home_screen(&workspace, 100, 30)?;
    root.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
    for _ in 0..3 {
        root.handle_event(key(KeyCode::Down), of(&workspace), &[]);
    }
    root.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
    terminal.draw(|frame| root.render(frame, &[]))?;
    let text = rendered(&terminal);
    assert!(text.contains("✗ gitserious.toml already exists"), "{text}");
    assert!(text.contains(" initialize fresh"), "the editor stays open");
    root.handle_event(key(KeyCode::Esc), of(&workspace), &[]);
    terminal.draw(|frame| root.render(frame, &[]))?;
    assert!(rendered(&terminal).contains(" initialize gitserious "));
    assert!(workspace.created.borrow().is_none());
    root.execute(Action::Select(Section::Help), of(&workspace));
    root.execute(Action::Select(Section::Home), of(&workspace));
    terminal.draw(|frame| root.render(frame, &[]))?;
    assert!(
        !rendered(&terminal).contains("already exists"),
        "a revisit starts clean"
    );

    let workspace = HomeWorkspace::new(ProjectOverview::Unavailable {
        directory: fresh_root().as_path().to_path_buf(),
        error: "git is not installed".into(),
    });
    let (mut root, _) = home_screen(&workspace, 87, 40)?;
    root.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
    assert_eq!(root.section, Section::Home);
    assert!(!root.home.editing(), "enter has no step to take");

    let workspace = HomeWorkspace::new(ProjectOverview::NoRepository {
        directory: fresh_root().as_path().to_path_buf(),
    });
    let (mut root, mut terminal) = home_screen(&workspace, 87, 40)?;
    root.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
    terminal.draw(|frame| root.render(frame, &[]))?;
    assert!(
        rendered(&terminal).contains(" initialize fresh"),
        "a plain directory opens the editor too"
    );
    Ok(())
}

#[test]
fn home_draws_a_static_wordmark_and_tagline_in_bone() -> Result<(), Box<dyn std::error::Error>> {
    let services = Services;
    let mut root = Root::new(InitialPage::Home, of(&services), &[]);
    for (width, height) in [(87, 40), (100, 30), (120, 30), (66, 18)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height))?;
        terminal.draw(|frame| root.render(frame, &[]))?;
        let text = rendered(&terminal);
        let rows: Vec<&str> = text.lines().collect();
        let wordmark = rows
            .iter()
            .position(|row| row.contains("▄████ ███ █████"))
            .ok_or("missing wordmark")?;
        let tagline = rows
            .iter()
            .position(|row| row.contains(home::TAGLINE))
            .ok_or("missing tagline")?;
        assert_eq!(
            tagline,
            wordmark + 6,
            "one blank row under the five-row wordmark"
        );
        let start = rows[wordmark].chars().take_while(|c| *c == ' ').count();
        assert!(
            start.abs_diff((usize::from(width) - 49) / 2) <= 1,
            "wordmark is centered"
        );
        let footer = rows[usize::from(height) - 1];
        assert!(
            footer.ends_with("tab nav bar  / commands  q quit "),
            "{footer}"
        );

        let buffer = terminal.backend().buffer();
        for y in [0, height - 1] {
            assert_eq!(buffer[(0, y)].bg, theme().band);
        }
        assert_eq!(buffer[(0, 1)].bg, theme().ground);
        let tab = footer.find("tab nav").ok_or("missing tab hint")?;
        assert!(
            buffer[(u16::try_from(tab)?, height - 1)]
                .modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(
            buffer[(u16::try_from(tab + 4)?, height - 1)].fg,
            theme().dim
        );
        let palette = [
            theme().ground,
            theme().band,
            theme().text,
            theme().dim,
            theme().cursor,
        ];
        for cell in buffer.content() {
            assert!(
                palette.contains(&cell.fg),
                "{cell:?} uses a color outside Bone"
            );
            assert!(
                palette.contains(&cell.bg),
                "{cell:?} uses a color outside Bone"
            );
        }
    }
    Ok(())
}

#[test]
fn the_open_home_editor_leaves_q_slash_and_ctrl_c_to_the_root()
-> Result<(), Box<dyn std::error::Error>> {
    let workspace = HomeWorkspace::new(ProjectOverview::Uninitialized { root: fresh_root() });
    let (mut root, _) = home_screen(&workspace, 87, 40)?;
    root.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
    assert!(root.home.editing());
    root.handle_event(key(KeyCode::Char('/')), of(&workspace), &[]);
    assert!(matches!(root.overlay, Some(Overlay::Palette(_))));
    root.handle_event(key(KeyCode::Esc), of(&workspace), &[]);
    assert!(root.overlay.is_none());
    assert!(
        root.home.editing(),
        "esc closed the palette, not the editor"
    );
    for code in [KeyCode::Char('r'), KeyCode::Char('c'), KeyCode::Char('o')] {
        assert_eq!(
            root.handle_event(key(code), of(&workspace), &[]),
            None,
            "{code:?}"
        );
        assert_eq!(root.section, Section::Home);
        assert!(root.home.editing(), "{code:?} reached the editor");
    }
    let control_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert_eq!(
        root.handle_event(control_c, of(&workspace), &[]),
        Some(Action::Quit)
    );
    assert_eq!(
        root.handle_event(key(KeyCode::Char('q')), of(&workspace), &[]),
        Some(Action::Quit)
    );
    assert!(workspace.created.borrow().is_none());
    Ok(())
}
