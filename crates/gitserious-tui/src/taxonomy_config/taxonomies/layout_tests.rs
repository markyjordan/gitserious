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
    press(view, library, KeyCode::Right);
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
fn switching_roots_anchors_once_and_preserves_child_expansion() -> Result {
    let (mut view, library) = fixture()?;
    let first = open(&mut view, &library, "conventional")?;
    let child = library.find_path(&["conventional", "feat"]).ok_or("feat")?;
    press(&mut view, &library, KeyCode::Right);
    press(&mut view, &library, KeyCode::Right);
    assert!(view.expanded.contains(&child));
    draw(&mut view, &library, 14)?;
    assert_eq!(view.outline_offset, 0);
    let research = open(&mut view, &library, "research")?;
    draw(&mut view, &library, 14)?;
    one_root(&view, &library, &research);
    assert_eq!(view.outline_offset, 0);
    assert_eq!(
        view.observe(Some(&library)).visible[0..2],
        [Some(first.clone()), Some(research.clone())]
    );
    assert!(view.expanded.contains(&child));
    let infra = open(&mut view, &library, "infra-ops")?;
    draw(&mut view, &library, 14)?;
    one_root(&view, &library, &infra);
    assert_eq!(view.outline_offset, 1);
    assert_eq!(
        view.observe(Some(&library)).visible[0..2],
        [Some(research), Some(infra)]
    );
    // Type expansion and navigation retain the root's anchor until selection needs scrolling.
    press(&mut view, &library, KeyCode::Right);
    press(&mut view, &library, KeyCode::Right);
    draw(&mut view, &library, 14)?;
    assert_eq!(view.outline_offset, 1);
    for _ in 0..10 {
        press(&mut view, &library, KeyCode::Down);
        draw(&mut view, &library, 14)?;
    }
    assert!(view.outline_offset > 1);
    let offset = view.outline_offset;
    draw(&mut view, &library, 14)?;
    assert_eq!(view.outline_offset, offset);
    let first_visible = view.observe(Some(&library)).visible[0]
        .clone()
        .ok_or("first row")?;
    assert!(view.click(view.outline_area.x, view.outline_area.y, &library));
    assert_eq!(
        view.selected(&library).map(NodeRef::id),
        Some(&first_visible)
    );
    open(&mut view, &library, "conventional")?;
    draw(&mut view, &library, 14)?;
    one_root(&view, &library, &first);
    assert_eq!(view.outline_offset, 0);
    assert!(view.expanded.contains(&child));
    assert!(
        view.observe(Some(&library))
            .visible
            .contains(&library.find_path(&["conventional", "feat", "intent"]))
    );
    Ok(())
}

#[test]
fn new_fork_and_revealed_children_open_only_their_root() -> Result {
    let (mut view, mut library) = fixture()?;
    open(&mut view, &library, "conventional")?;
    view.act(LibraryAction::Fork, &mut library)?;
    let fork = view.selected(&library).ok_or("fork")?.id().clone();
    draw(&mut view, &library, 36)?;
    one_root(&view, &library, &fork);
    assert_eq!(view.outline_offset, 2);
    view.leave_edit();
    view.act(LibraryAction::Create, &mut library)?;
    let new = view.selected(&library).ok_or("new")?.id().clone();
    for height in [36, 14, 37, 36] {
        draw(&mut view, &library, height)?;
        one_root(&view, &library, &new);
        assert_eq!(view.outline_offset, 3);
        let visible = view.observe(Some(&library)).visible;
        assert_eq!(visible, [Some(fork.clone()), Some(new.clone()), None]);
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
    assert_eq!(view.outline_offset, 2);
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
