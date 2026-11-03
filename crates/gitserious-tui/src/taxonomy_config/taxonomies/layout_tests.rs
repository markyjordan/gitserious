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
fn opening_collapses_only_earlier_roots_and_normal_scrolling_keeps_selection_visible() -> Result {
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
    assert!(!view.expanded.contains(&first));
    assert!(view.expanded.contains(&infra));
    assert!(view.expanded.contains(&research));
    assert!(view.expanded.contains(&child));
    open(&mut view, &library, "conventional")?;
    assert!(view.expanded.contains(&infra));
    assert!(view.expanded.contains(&research));
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
    draw(&mut view, &library, 100)?;
    assert_eq!(view.outline_offset, 0, "short branches clamp to the top");
    assert!(!view.expanded.contains(&first));
    assert!(!view.expanded.contains(&research));
    assert!(view.expanded.contains(&child));
    Ok(())
}

#[test]
fn new_fork_and_revealed_children_collapse_only_earlier_roots() -> Result {
    let (mut view, mut library) = fixture()?;
    open(&mut view, &library, "conventional")?;
    view.act(LibraryAction::Fork, &mut library)?;
    let fork = view.selected(&library).ok_or("fork")?.id().clone();
    draw(&mut view, &library, 36)?;
    one_root(&view, &library, &fork);
    view.leave_edit();
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
    assert!(view.expanded.contains(&fork));
    assert!(view.expanded.contains(&new));
    assert_eq!(view.selected(&library).map(NodeRef::id), Some(&property));
    assert!(
        view.observe(Some(&library))
            .visible
            .contains(&Some(property))
    );
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
