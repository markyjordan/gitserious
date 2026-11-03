use gitserious_app::GlobalConfiguration;
use gitserious_core::{
    CommitTypeDefinition, CommitTypeId, Description, PropertyDefinition, PropertyKey,
    PropertyRequirement, SchemaVersion, TaxonomyVersion,
};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

use super::tests::Workspace;
use super::*;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
const HOLD: Duration = Duration::from_secs(3);
const PREFIX_WIDTH: u16 = 14;

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn fixture() -> Result<(Workspace, State, Terminal<TestBackend>)> {
    let types = ["long", "next", "fitting"]
        .into_iter()
        .map(|name| -> Result<_> {
            let properties = if name == "fitting" {
                vec!["one"]
            } else {
                vec![
                    "alpha-property-with-a-long-name",
                    "beta-property-with-a-long-name",
                    "gamma-property-with-a-long-name",
                ]
            }
            .into_iter()
            .map(|name| -> Result<_> {
                Ok(PropertyDefinition::new(
                    PropertyKey::new(name)?,
                    "Property description",
                    PropertyRequirement::Optional,
                )?)
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(CommitTypeDefinition::new(
                SchemaVersion::new(1)?,
                CommitTypeId::new(name)?,
                "Type description",
                properties,
            )?)
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let taxonomy = Taxonomy::new(
        TaxonomyId::new("motion")?,
        TaxonomyVersion::new(1)?,
        Description::new("Animation fixture")?,
        None,
        types,
    )?;
    let workspace = Workspace::new();
    *workspace.global.borrow_mut() = ConfigurationSession::open_global(GlobalConfiguration::new(
        1,
        TaxonomyId::new("conventional")?,
        vec![TaxonomyId::new("conventional")?],
        vec![taxonomy],
    )?);
    let mut state = State::new(&workspace);
    state.category = Category::Taxonomies;
    state.home_focus = HomeFocus::Content;
    let id = state
        .library()
        .and_then(|library| library.find_path(&["motion"]))
        .ok_or("motion")?;
    state.taxonomies.select(id);
    state.handle_event(key(KeyCode::Right), &workspace);
    state.handle_event(key(KeyCode::Right), &workspace);
    let terminal = Terminal::new(TestBackend::new(80, 30))?;
    Ok((workspace, state, terminal))
}

fn draw(state: &mut State, terminal: &mut Terminal<TestBackend>) -> Result<Buffer> {
    terminal.draw(|frame| state.render(frame))?;
    Ok(terminal.backend().buffer().clone())
}

fn summary_area(state: &State, badge_width: u16) -> Result<Rect> {
    let observation = state.taxonomies.observe(state.library());
    let selected = state
        .taxonomies
        .selected(state.library().ok_or("library")?)
        .ok_or("selected")?;
    let row = observation
        .visible
        .iter()
        .position(|id| id.as_ref() == Some(selected.id()))
        .ok_or("visible")?;
    let inner = observation.outline_area;
    Ok(Rect::new(
        inner.x + PREFIX_WIDTH,
        inner.y + u16::try_from(row)?,
        inner.width - 1 - PREFIX_WIDTH - badge_width - u16::from(badge_width > 0),
        1,
    ))
}

fn outside_unchanged(before: &Buffer, after: &Buffer, area: Rect) {
    for position in before
        .area
        .positions()
        .filter(|position| !area.contains(*position))
    {
        assert_eq!(
            before[position], after[position],
            "outside summary at {position:?}"
        );
    }
}

fn select(state: &mut State, name: &str) -> Result {
    let id = state
        .library()
        .and_then(|library| library.find_path(&["motion", name]))
        .ok_or("type")?;
    state.taxonomies.select(id);
    Ok(())
}

#[test]
fn selected_overflow_animates_without_changing_its_prefix_badge_or_neighbors() -> Result {
    let (_, mut state, mut terminal) = fixture()?;
    let selected = state
        .taxonomies
        .selected(state.library().ok_or("library")?)
        .ok_or("selected")?
        .id()
        .clone();
    state
        .global
        .as_mut()
        .and_then(ConfigurationSession::library_mut)
        .ok_or("library")?
        .edit(
            &selected,
            DraftEdit::Text(
                gitserious_app::DraftField::Description,
                "Changed description",
            ),
        )?;
    let initial = draw(&mut state, &mut terminal)?;
    let area = summary_area(&state, 12)?;
    assert_eq!(state.next_frame_in(), Some(HOLD));
    state.advance(HOLD.saturating_sub(Duration::from_millis(1)), true);
    assert_eq!(draw(&mut state, &mut terminal)?, initial);
    state.advance(Duration::from_millis(1), true);
    let moved = draw(&mut state, &mut terminal)?;
    assert_ne!(initial, moved);
    outside_unchanged(&initial, &moved, area);
    assert_eq!(draw(&mut state, &mut terminal)?, moved);
    select(&mut state, "fitting")?;
    let static_frame = draw(&mut state, &mut terminal)?;
    assert_eq!(state.next_frame_in(), None);
    state.advance(Duration::from_secs(50), true);
    assert_eq!(draw(&mut state, &mut terminal)?, static_frame);
    Ok(())
}

#[test]
fn built_in_summaries_use_the_full_unbadged_width() -> Result {
    let workspace = Workspace::new();
    let mut state = State::new(&workspace);
    state.category = Category::Taxonomies;
    state.home_focus = HomeFocus::Content;
    let root = state
        .library()
        .and_then(|library| library.find_path(&["infra-ops"]))
        .ok_or("infra-ops")?;
    state.taxonomies.select(root);
    for code in [KeyCode::Right, KeyCode::Right] {
        state.handle_event(key(code), &workspace);
    }
    for width in [MINIMUM_WIDTH, 87, 140, MINIMUM_WIDTH] {
        let mut terminal = Terminal::new(TestBackend::new(width, 40))?;
        let initial = draw(&mut state, &mut terminal)?;
        let observation = state.taxonomies.observe(state.library());
        let inner = observation.outline_area;
        let selected = state
            .taxonomies
            .selected(state.library().ok_or("library")?)
            .ok_or("selected")?
            .id();
        let y = inner.y
            + u16::try_from(
                observation
                    .visible
                    .iter()
                    .position(|id| id.as_ref() == Some(selected))
                    .ok_or("selected")?,
            )?;
        let line = (inner.x..inner.right())
            .map(|x| initial[(x, y)].symbol())
            .collect::<String>();
        let prefix = "▌  ▸ provision  ";
        assert!(line.starts_with(prefix), "{line}");
        let area = Rect::new(
            inner.x + u16::try_from(prefix.chars().count())?,
            y,
            inner.width - 1 - u16::try_from(prefix.chars().count())?,
            1,
        );
        assert!(
            !line.contains("read-only") && !line.contains("built-in"),
            "{line}"
        );
        if width < 140 {
            assert_eq!(state.next_frame_in(), Some(HOLD));
            assert_ne!(initial[(area.right() - 1, y)].symbol(), " ");
            state.advance(HOLD, true);
            let moved = draw(&mut state, &mut terminal)?;
            assert_ne!(initial, moved);
            outside_unchanged(&initial, &moved, area);
        } else {
            assert_eq!(state.next_frame_in(), None);
        }
    }
    Ok(())
}

#[test]
fn selection_expansion_edit_and_picker_restart_or_stop_the_clock() -> Result {
    let (workspace, mut state, mut terminal) = fixture()?;
    draw(&mut state, &mut terminal)?;
    state.advance(HOLD, true);
    state.handle_event(key(KeyCode::Down), &workspace);
    draw(&mut state, &mut terminal)?;
    assert_eq!(state.next_frame_in(), Some(HOLD));
    state.handle_event(key(KeyCode::Up), &workspace);
    draw(&mut state, &mut terminal)?;
    assert_eq!(state.next_frame_in(), Some(HOLD));
    state.handle_event(key(KeyCode::Enter), &workspace);
    assert_eq!(state.next_frame_in(), None);
    draw(&mut state, &mut terminal)?;
    assert_eq!(state.next_frame_in(), None);
    state.handle_event(key(KeyCode::Enter), &workspace);
    draw(&mut state, &mut terminal)?;
    assert_eq!(state.next_frame_in(), Some(HOLD));
    state.advance(HOLD, true);
    state.handle_event(key(KeyCode::Char('e')), &workspace);
    assert_eq!(state.next_frame_in(), None);
    draw(&mut state, &mut terminal)?;
    state.handle_event(key(KeyCode::Esc), &workspace);
    draw(&mut state, &mut terminal)?;
    assert_eq!(state.next_frame_in(), Some(HOLD));
    for code in [KeyCode::Enter, KeyCode::Right, KeyCode::Char('c')] {
        state.handle_event(key(code), &workspace);
    }
    assert!(state.taxonomies.picker_hints().is_some());
    let picker = draw(&mut state, &mut terminal)?;
    state.advance(Duration::from_secs(20), true);
    assert_eq!(state.next_frame_in(), None);
    assert_eq!(draw(&mut state, &mut terminal)?, picker);
    state.handle_event(key(KeyCode::Esc), &workspace);
    select(&mut state, "long")?;
    state.handle_event(key(KeyCode::Enter), &workspace);
    draw(&mut state, &mut terminal)?;
    assert_eq!(state.next_frame_in(), Some(HOLD));
    Ok(())
}

#[test]
fn own_overlays_and_every_inactive_view_stop_and_restart_the_hold() -> Result {
    for overlay_key in ['?', '/'] {
        let (workspace, mut state, mut terminal) = fixture()?;
        draw(&mut state, &mut terminal)?;
        state.advance(HOLD, true);
        state.handle_event(key(KeyCode::Char(overlay_key)), &workspace);
        let overlay = draw(&mut state, &mut terminal)?;
        assert_eq!(state.next_frame_in(), None);
        state.advance(Duration::from_secs(20), true);
        assert_eq!(draw(&mut state, &mut terminal)?, overlay);
        state.handle_event(key(KeyCode::Esc), &workspace);
        draw(&mut state, &mut terminal)?;
        assert_eq!(state.next_frame_in(), Some(HOLD));
    }
    for inactive in 0..4 {
        let (_, mut state, mut terminal) = fixture()?;
        draw(&mut state, &mut terminal)?;
        state.advance(HOLD, true);
        match inactive {
            0 => state.home_focus = HomeFocus::Menu,
            1 => state.category = Category::Project,
            2 => state.screen = Screen::Review(ConfigurationDestination::Global),
            _ => state.advance(Duration::ZERO, false),
        }
        let hidden = draw(&mut state, &mut terminal)?;
        state.advance(Duration::from_secs(20), inactive != 3);
        assert_eq!(state.next_frame_in(), None);
        assert_eq!(draw(&mut state, &mut terminal)?, hidden);
        state.home_focus = HomeFocus::Content;
        state.category = Category::Taxonomies;
        state.screen = Screen::Home;
        state.advance(Duration::ZERO, true);
        draw(&mut state, &mut terminal)?;
        assert_eq!(state.next_frame_in(), Some(HOLD));
    }
    Ok(())
}

#[test]
fn resize_and_small_viewports_clear_old_phase_in_standalone_config() -> Result {
    let (_, mut state, mut terminal) = fixture()?;
    draw(&mut state, &mut terminal)?;
    state.advance(HOLD, true);
    for (width, height) in [
        (80, 31),
        (80, 35),
        (MINIMUM_WIDTH - 1, MINIMUM_HEIGHT),
        (80, 30),
    ] {
        terminal.backend_mut().resize(width, height);
        terminal.resize(Rect::new(0, 0, width, height))?;
        draw(&mut state, &mut terminal)?;
        assert_eq!(
            state.next_frame_in(),
            if width < MINIMUM_WIDTH {
                None
            } else {
                Some(HOLD)
            }
        );
        state.advance(HOLD, true);
    }
    select(&mut state, "long")?;
    terminal.backend_mut().resize(200, 30);
    terminal.resize(Rect::new(0, 0, 200, 30))?;
    draw(&mut state, &mut terminal)?;
    assert_eq!(state.next_frame_in(), None);
    Ok(())
}

#[test]
fn a_prefix_that_leaves_no_summary_room_stays_static() -> Result {
    let (_, mut state, mut terminal) = fixture()?;
    let selected = state
        .taxonomies
        .selected(state.library().ok_or("library")?)
        .ok_or("selected")?
        .id()
        .clone();
    state
        .global
        .as_mut()
        .and_then(ConfigurationSession::library_mut)
        .ok_or("library")?
        .edit(
            &selected,
            DraftEdit::Text(gitserious_app::DraftField::Name, &"x".repeat(80)),
        )?;
    let initial = draw(&mut state, &mut terminal)?;
    assert_eq!(state.next_frame_in(), None);
    state.advance(Duration::from_secs(20), true);
    assert_eq!(draw(&mut state, &mut terminal)?, initial);
    Ok(())
}
