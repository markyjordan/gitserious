use super::tests::Workspace;
use super::*;
use gitserious_app::{ProjectRevision, StoredProject};
use gitserious_core::{
    CommitTypeDefinition, CommitTypeId, Description, PropertyDefinition, PropertyKey,
    PropertyRequirement, SchemaVersion, TaxonomyVersion,
};
use ratatui::{Terminal, backend::TestBackend};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn fixture() -> Result<(Workspace, ConfigView)> {
    let workspace = Workspace::new();
    let taxonomy = Taxonomy::new(
        TaxonomyId::new("local-team")?,
        TaxonomyVersion::new(3)?,
        Description::new("Only the project's installed definition.")?,
        None,
        vec![
            CommitTypeDefinition::new(
                SchemaVersion::new(1)?,
                CommitTypeId::new("change")?,
                "A project-only change.",
                vec![PropertyDefinition::new(
                    PropertyKey::new("reason")?,
                    "Project-only guidance.",
                    PropertyRequirement::Required,
                )?],
            )?,
            CommitTypeDefinition::new(
                SchemaVersion::new(1)?,
                CommitTypeId::new("empty")?,
                "A type without properties.",
                vec![],
            )?,
        ],
    )?;
    *workspace.project.borrow_mut() = ConfigurationSession::open_project(
        RepositoryRoot::new(std::env::temp_dir())?,
        Some(StoredProject {
            configuration: ProjectConfiguration::new(1, taxonomy.id().clone(), vec![taxonomy])?,
            revision: ProjectRevision::of_bytes(b"opened"),
        }),
    );
    let mut view = ConfigView::new(&workspace);
    view.state.global = None;
    view.state.global_error = "The Library is corrupt.".into();
    Ok((workspace, view))
}

fn press(view: &mut ConfigView, workspace: &Workspace, code: KeyCode) {
    assert!(
        view.handle_event(
            Event::Key(KeyEvent::new(code, KeyModifiers::NONE)),
            workspace
        )
        .is_none()
    );
}

fn rendered(view: &mut ConfigView, terminal: &mut Terminal<TestBackend>) -> Result<String> {
    terminal.draw(|frame| view.render(frame, frame.area()))?;
    let buffer = terminal.backend().buffer();
    Ok((0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

#[test]
fn installed_definitions_use_the_library_layout_even_without_a_readable_library() -> Result {
    let (workspace, mut view) = fixture()?;
    for (width, height, stacked) in [
        (87, 40, true),
        (125, 51, true),
        (100, 30, false),
        (66, 18, true),
    ] {
        let mut terminal = Terminal::new(TestBackend::new(width, height))?;
        let text = rendered(&mut view, &mut terminal)?;
        assert!(text.contains("local-team"), "{text}");
        assert!(
            text.contains("Only the project's") && text.contains("definition."),
            "{text}"
        );
        assert!(text.contains("✓ gitserious.toml"), "{text}");
        assert!(text.contains("◇ read-only"), "{text}");
        assert!(
            !text.contains("conventional"),
            "the global catalog must not leak into Project"
        );
        assert!(!text.contains("The Library is corrupt."));
        assert!(!text.contains("+ new taxonomy"));
        assert!(!text.contains("fork to customize"));
        let observed = view
            .state
            .project_taxonomies
            .observe(view.state.editor_draft());
        if stacked {
            assert!(observed.inspector_area.y > observed.outline_area.bottom());
        } else {
            assert!(observed.inspector_area.x > observed.outline_area.right());
            assert_eq!(observed.inspector_area.y, 1);
        }
        assert!(!view.dirty());
    }
    assert_eq!(workspace.saves(), 0);
    Ok(())
}

#[test]
fn project_browsing_expands_real_nodes_and_never_offers_or_performs_mutations() -> Result {
    let (workspace, mut view) = fixture()?;
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    press(&mut view, &workspace, KeyCode::Enter);
    assert!(
        matches!(view.state.screen, Screen::Home),
        "Enter from the rail browses rather than editing settings"
    );
    let text = rendered(&mut view, &mut terminal)?;
    assert!(!text.contains("+ add"));
    press(&mut view, &workspace, KeyCode::Down);
    let text = rendered(&mut view, &mut terminal)?;
    assert!(text.contains("Type change"));
    assert!(text.contains("Project-only guidance."));
    assert!(
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .contains("enter open")
    );
    for code in [
        KeyCode::Char('e'),
        KeyCode::Char('f'),
        KeyCode::Char('n'),
        KeyCode::Char('d'),
        KeyCode::Char('i'),
        KeyCode::Char('l'),
        KeyCode::Char('c'),
        KeyCode::Delete,
    ] {
        press(&mut view, &workspace, code);
    }
    for event in [
        Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT)),
        Event::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)),
        Event::Paste("unwanted edit".into()),
    ] {
        assert!(view.handle_event(event, &workspace).is_none());
    }
    assert!(!view.dirty());
    assert_eq!(workspace.saves(), 0);
    assert_eq!(text, rendered(&mut view, &mut terminal)?);
    press(&mut view, &workspace, KeyCode::Enter);
    press(&mut view, &workspace, KeyCode::Down);
    let text = rendered(&mut view, &mut terminal)?;
    assert!(text.contains("Property reason"));
    press(&mut view, &workspace, KeyCode::Enter);
    assert!(
        view.handle_event(
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT)),
            &workspace
        )
        .is_none()
    );
    assert!(
        matches!(view.state.screen, Screen::Home),
        "leaf Enter cannot fall through to legacy settings"
    );
    assert_eq!(text, rendered(&mut view, &mut terminal)?);
    let footer = text.lines().last().ok_or("missing footer")?;
    for absent in ["edit", "copy", "delete", "fork", "new", "reorder", "save"] {
        assert!(!footer.contains(absent), "{footer}");
    }
    press(&mut view, &workspace, KeyCode::Char('/'));
    let text = rendered(&mut view, &mut terminal)?;
    for absent in ["Configure project", "Initialize project", "Review changes"] {
        assert!(!text.contains(absent), "{text}");
    }
    press(&mut view, &workspace, KeyCode::Esc);
    press(&mut view, &workspace, KeyCode::Left);
    assert_eq!(view.state.home_focus, HomeFocus::Menu);
    press(&mut view, &workspace, KeyCode::Down);
    assert_eq!(view.state.category, Category::Taxonomies);
    press(&mut view, &workspace, KeyCode::Up);
    assert_eq!(view.state.category, Category::Project);
    let text = rendered(&mut view, &mut terminal)?;
    assert!(
        text.contains("Property reason"),
        "selection survives category visits"
    );
    press(&mut view, &workspace, KeyCode::Right);
    press(&mut view, &workspace, KeyCode::Down);
    let text = rendered(&mut view, &mut terminal)?;
    assert!(text.contains("Type empty"));
    assert!(
        !text
            .lines()
            .last()
            .ok_or("missing footer")?
            .contains("enter"),
        "a propertyless type has nothing to open"
    );
    assert!(!text.contains("+ add"));
    press(&mut view, &workspace, KeyCode::Enter);
    assert!(matches!(view.state.screen, Screen::Home));
    assert!(!view.dirty());
    Ok(())
}
