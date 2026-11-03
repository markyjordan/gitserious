use crate::library_lookup::LibraryLookup;
use gitserious_app::{ConfigurationSession, GlobalConfiguration};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

use super::*;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn fixture() -> Result<(TaxonomiesView, LibraryDraft)> {
    let session = ConfigurationSession::open_global(GlobalConfiguration::default());
    Ok((
        TaxonomiesView::default(),
        session.library().ok_or("library")?.clone(),
    ))
}

fn draw(view: &mut TaxonomiesView, library: &LibraryDraft, height: u16) -> Result<Buffer> {
    let mut terminal = Terminal::new(TestBackend::new(70, height))?;
    terminal.draw(|frame| view.render(frame, frame.area(), Some(library), true, false))?;
    Ok(terminal.backend().buffer().clone())
}

fn press(view: &mut TaxonomiesView, library: &LibraryDraft, code: KeyCode) {
    view.key(KeyEvent::new(code, KeyModifiers::NONE), library);
}

fn open(view: &mut TaxonomiesView, library: &LibraryDraft, name: &str) -> Result<NodeId> {
    let id = library.find_path(&[name]).ok_or("taxonomy")?;
    view.select(id.clone());
    if view.expanded.contains(&id) {
        press(view, library, KeyCode::Enter);
    }
    press(view, library, KeyCode::Enter);
    Ok(id)
}

fn one_root(view: &TaxonomiesView, library: &LibraryDraft, expected: &NodeId) {
    let open = library
        .taxonomies()
        .iter()
        .filter(|taxonomy| view.expanded.contains(taxonomy.id()))
        .map(gitserious_app::TaxonomyNode::id)
        .collect::<Vec<_>>();
    assert_eq!(open, [expected]);
}

#[test]
fn outline_heading_matches_without_a_library_and_fits_beside_scroll_counts() -> Result {
    for width in [50, 71, 124] {
        let (mut view, library) = fixture()?;
        let mut terminal = Terminal::new(TestBackend::new(width, 14))?;
        for loaded in [false, true] {
            if loaded {
                open(&mut view, &library, "conventional")?;
                for _ in 0..8 {
                    press(&mut view, &library, KeyCode::Down);
                }
            }
            terminal.draw(|frame| {
                view.render(frame, frame.area(), loaded.then_some(&library), true, false);
            })?;
            let row = |y| {
                (0..width)
                    .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                    .collect::<String>()
            };
            assert!(row(0).starts_with("┌ Commit Message Taxonomies ─"));
            if loaded {
                assert!(view.outline_offset > 0);
                assert!(row(0).contains(&format!(" ↑ {} more ", view.outline_offset)));
                let below = outline::rows(&library, &view.expanded).len()
                    - view.outline_offset
                    - usize::from(view.outline_area.height);
                assert!(below > 0);
                assert!(row(6).contains(&format!(" ↓ {below} more ")));
            } else {
                assert!(row(1).contains("Taxonomy library unavailable."));
            }
        }
    }
    Ok(())
}

#[test]
fn opening_collapses_all_other_roots_and_normal_scrolling_keeps_selection_visible() -> Result {
    let (mut view, library) = fixture()?;
    let first = open(&mut view, &library, "conventional")?;
    let child = library.find_path(&["conventional", "feat"]).ok_or("feat")?;
    press(&mut view, &library, KeyCode::Down);
    press(&mut view, &library, KeyCode::Enter);
    assert!(view.expanded.contains(&child));
    let infra = open(&mut view, &library, "infra-ops")?;
    draw(&mut view, &library, 14)?;
    assert_eq!(
        view.outline_offset, 0,
        "opening does not force the root onto row two"
    );
    let research = open(&mut view, &library, "research")?;
    one_root(&view, &library, &research);
    assert!(view.expanded.contains(&child));
    open(&mut view, &library, "conventional")?;
    one_root(&view, &library, &first);
    assert!(outline::rows(&library, &view.expanded).iter().any(|row| {
        matches!(row, Row::Node(NodeRef::Property { commit_type, .. }) if commit_type.id() == &child)
    }));
    for _ in 0..12 {
        press(&mut view, &library, KeyCode::Down);
        draw(&mut view, &library, 14)?;
    }
    assert!(view.outline_offset > 0);
    let first_visible = view.observe(Some(&library)).visible[0]
        .clone()
        .ok_or("row")?;
    assert!(view.click(view.outline_area.x, view.outline_area.y, &library));
    assert_eq!(
        view.selected(&library).map(NodeRef::id),
        Some(&first_visible)
    );
    open(&mut view, &library, "infra-ops")?;
    one_root(&view, &library, &infra);
    draw(&mut view, &library, 100)?;
    assert_eq!(view.outline_offset, 0, "short branches clamp to the top");
    assert!(!view.expanded.contains(&first));
    assert!(!view.expanded.contains(&research));
    assert!(view.expanded.contains(&child));
    Ok(())
}

#[test]
fn new_fork_and_revealed_children_collapse_all_other_roots() -> Result {
    let (mut view, mut library) = fixture()?;
    open(&mut view, &library, "conventional")?;
    view.act(LibraryAction::Fork, &mut library)?;
    let fork = view.selected(&library).ok_or("fork")?.id().clone();
    draw(&mut view, &library, 36)?;
    one_root(&view, &library, &fork);
    view.edit_key(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        &mut library,
    )?;
    view.act(LibraryAction::Create, &mut library)?;
    let new = view.selected(&library).ok_or("new")?.id().clone();
    for height in [36, 14, 37, 36] {
        draw(&mut view, &library, height)?;
        one_root(&view, &library, &new);
        let visible = view.observe(Some(&library)).visible;
        assert!(visible.contains(&Some(new.clone())));
    }
    let property = match library.node(&fork) {
        Some(NodeRef::Taxonomy(taxonomy)) => taxonomy.types()[0].properties()[0].id().clone(),
        _ => return Err("fork".into()),
    };
    view.open(property.clone(), DraftField::Description, &library);
    draw(&mut view, &library, 36)?;
    one_root(&view, &library, &fork);
    assert_eq!(view.selected(&library).map(NodeRef::id), Some(&property));
    assert!(
        view.observe(Some(&library))
            .visible
            .contains(&Some(property))
    );
    Ok(())
}

#[test]
fn switching_between_built_in_and_custom_roots_remembers_multiple_open_types() -> Result {
    let (mut view, mut library) = fixture()?;
    let conventional = open(&mut view, &library, "conventional")?;
    view.act(LibraryAction::Fork, &mut library)?;
    let fork = view.selected(&library).ok_or("fork")?.id().clone();
    press(&mut view, &library, KeyCode::Esc);
    let mut remembered = Vec::new();
    for root in [&conventional, &fork] {
        view.select(root.clone());
        if !view.expanded.contains(root) {
            press(&mut view, &library, KeyCode::Enter);
        }
        let Some(NodeRef::Taxonomy(taxonomy)) = library.node(root) else {
            return Err("taxonomy".into());
        };
        for commit_type in taxonomy.types().iter().take(2) {
            view.select(commit_type.id().clone());
            press(&mut view, &library, KeyCode::Enter);
            remembered.push(commit_type.id().clone());
        }
    }
    assert_eq!(remembered.len(), 4);
    let roots = library
        .taxonomies()
        .iter()
        .map(|taxonomy| taxonomy.id().clone())
        .collect::<Vec<_>>();
    for from in &roots {
        for to in &roots {
            for root in [from, to] {
                view.select(root.clone());
                if !view.expanded.contains(root) {
                    press(&mut view, &library, KeyCode::Enter);
                }
                one_root(&view, &library, root);
                assert!(remembered.iter().all(|id| view.expanded.contains(id)));
                draw(&mut view, &library, 14)?;
                assert!(
                    view.observe(Some(&library))
                        .visible
                        .contains(&Some(root.clone()))
                );
                for id in &remembered {
                    if library
                        .node(id)
                        .is_some_and(|node| node.taxonomy().id() == root)
                    {
                        assert!(outline::rows(&library, &view.expanded).iter().any(|row| {
                            matches!(row, Row::Node(NodeRef::Property { commit_type, .. }) if commit_type.id() == id)
                        }));
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn bordered_panels_split_evenly_and_round_up_for_taxonomies() -> Result {
    let (mut view, library) = fixture()?;
    for height in [36, 37, 14, 15, 35, 36] {
        let buffer = draw(&mut view, &library, height)?;
        let outline = height.div_ceil(2);
        assert_eq!(view.outline_area.height + 2, outline);
        assert_eq!(buffer[(0, outline - 1)].symbol(), "└");
        assert_eq!(buffer[(0, outline)].symbol(), "┌");
        assert_eq!(buffer[(0, height - 1)].symbol(), "└");
        assert_eq!(outline - (height - outline), height % 2);
    }
    Ok(())
}

#[test]
fn focused_fields_and_their_errors_remain_visible_in_short_inspectors() -> Result {
    use gitserious_app::Policy;
    let (mut view, mut library) = fixture()?;
    open(&mut view, &library, "conventional")?;
    view.act(LibraryAction::Fork, &mut library)?;
    let property = match view.selected(&library) {
        Some(NodeRef::Taxonomy(taxonomy)) => taxonomy.types()[0].properties()[0].id().clone(),
        _ => return Err("fork".into()),
    };
    library.edit(&property, DraftEdit::Policy(Policy::Conditional))?;
    for (field, label, problem) in [
        (DraftField::Condition, "Condition", "condition."),
        (DraftField::Rationale, "Rationale", "rationale."),
    ] {
        view.open(property.clone(), field, &library);
        for height in [14, 15, 36, 14] {
            let mut terminal = Terminal::new(TestBackend::new(51, height))?;
            terminal.draw(|frame| view.render(frame, frame.area(), Some(&library), true, false))?;
            let buffer = terminal.backend().buffer();
            let inspector = buffer
                .content
                .chunks(51)
                .skip(usize::from(height.div_ceil(2)))
                .map(|row| {
                    row.iter()
                        .map(ratatui::buffer::Cell::symbol)
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            assert!(inspector.contains(label), "{inspector}");
            assert!(inspector.contains(problem), "{inspector}");
            assert!(inspector.contains("▲"), "{inspector}");
            assert!(
                inspector.contains("0/80") || inspector.contains("0/100"),
                "{inspector}"
            );
        }
    }
    Ok(())
}
