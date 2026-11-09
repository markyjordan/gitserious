use crate::library_lookup::LibraryLookup;
use crate::theme::theme;
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
    terminal.draw(|frame| {
        view.render(frame, frame.area(), 0, Some(library), true);
    })?;
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

/// The row of the buffer that is a dim rule from edge to edge, if any.
fn rule_row(buffer: &Buffer) -> Option<u16> {
    (0..buffer.area.height).find(|&y| {
        (0..buffer.area.width)
            .all(|x| buffer[(x, y)].symbol() == "─" && buffer[(x, y)].fg == theme().dim)
    })
}

fn text_in(buffer: &Buffer, area: Rect) -> String {
    (area.y..area.bottom())
        .map(|y| {
            (area.x..area.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn outline_counts_rows_below_in_its_last_row_without_a_box() -> Result {
    for width in [50, 71, 124] {
        let (mut view, library) = fixture()?;
        let mut terminal = Terminal::new(TestBackend::new(width, 10))?;
        for loaded in [false, true] {
            if loaded {
                open(&mut view, &library, "conventional")?;
                for _ in 0..8 {
                    press(&mut view, &library, KeyCode::Down);
                }
            }
            terminal.draw(|frame| {
                view.render(frame, frame.area(), 0, loaded.then_some(&library), true);
            })?;
            let buffer = terminal.backend().buffer();
            let screen = text_in(buffer, buffer.area);
            assert!(!screen.contains("Commit Message Taxonomies"), "{screen}");
            assert!(!screen.contains(['┌', '┐', '┘']), "{screen}");
            if loaded {
                let outline = view.outline_area;
                let below = outline::rows(&library, &view.expanded, false).len()
                    - view.outline_offset
                    - usize::from(outline.height);
                assert!(below > 0);
                let last = Rect::new(outline.x, outline.bottom(), outline.width, 1);
                let more = format!("↓ {below} more");
                assert!(
                    text_in(buffer, last).ends_with(&format!("{more} ")),
                    "{screen}"
                );
                let at = last.right() - 2;
                assert_eq!(buffer[(at, last.y)].symbol(), "e");
                assert_eq!(buffer[(at, last.y)].fg, theme().dim);
            } else {
                assert!(
                    screen.starts_with(" Taxonomy library unavailable."),
                    "{screen}"
                );
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
    assert!(outline::rows(&library, &view.expanded, false).iter().any(|row| {
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
    view.finish_edit();
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
    view.open(
        property.clone(),
        DraftField::Description,
        &library,
        view.edit_baseline(&library),
    );
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
                        assert!(outline::rows(&library, &view.expanded, false).iter().any(|row| {
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
fn a_rule_parts_the_outline_and_inspector_sixty_forty_or_below_a_short_outline() -> Result {
    let (mut view, library) = fixture()?;
    // From 24 rows a blank row pads the top and the bottom. The outline and the rule take 60%
    // of the rest; three collapsed taxonomies and the new taxonomy row take fewer rows than that.
    for (height, short, long) in [
        (36, 5, 16),
        (24, 5, 13),
        (23, 4, 12),
        (15, 4, 8),
        (14, 4, 7),
    ] {
        for (expanded, rule) in [(false, short), (true, long)] {
            let conventional = library.find_path(&["conventional"]).ok_or("conventional")?;
            if view.expanded.contains(&conventional) != expanded {
                view.select(conventional);
                press(&mut view, &library, KeyCode::Enter);
            }
            let buffer = draw(&mut view, &library, height)?;
            assert_eq!(rule_row(&buffer), Some(rule), "{height} rows");
            assert!(buffer[(1, rule + 1)].symbol() != " ", "{height} rows");
            let screen = text_in(&buffer, buffer.area);
            assert!(screen.contains("Taxonomy conventional"), "{screen}");
            // The tree draws └ and │ too, but never the other corners of a box.
            assert!(!screen.contains(['┌', '┐', '┘']), "{screen}");
        }
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
        view.open(
            property.clone(),
            field,
            &library,
            view.edit_baseline(&library),
        );
        for height in [14, 15, 36, 14] {
            let mut terminal = Terminal::new(TestBackend::new(51, height))?;
            terminal.draw(|frame| {
                view.render(frame, frame.area(), 0, Some(&library), true);
            })?;
            let buffer = terminal.backend().buffer();
            let below = rule_row(buffer).ok_or("missing the rule")? + 1;
            let inspector = text_in(buffer, Rect::new(0, below, 51, buffer.area.height - below));
            assert!(inspector.contains(label), "{inspector}");
            assert!(inspector.contains(problem), "{inspector}");
            assert!(inspector.contains("▲"), "{inspector}");
            assert!(!inspector.contains("0/80") && !inspector.contains("0/100"));
        }
    }
    Ok(())
}

#[test]
fn footer_keys_have_balanced_padding_and_a_separate_label_gap() -> Result {
    let (view, library) = fixture()?;
    for width in [64, 85, 138] {
        let line = view.status_line(Some(&library), true, width);
        for spans in line.spans.windows(2) {
            let [chip, label] = spans else { unreachable!() };
            if chip.style == footer_key_style() {
                assert_eq!(chip.content.starts_with(' '), chip.content.ends_with(' '));
                if width == 138 {
                    assert!(chip.content.starts_with(' '));
                }
                assert!(label.content.starts_with(' '));
                assert_eq!(label.style.bg, Some(theme().band));
            }
        }
    }
    Ok(())
}

#[test]
fn type_rows_omit_property_previews_and_expand_to_real_properties() -> Result {
    for read_only in [false, true] {
        for (width, height) in [(66, 24), (87, 40), (100, 30), (125, 40)] {
            let (mut view, library) = fixture()?;
            view.read_only = read_only;
            open(&mut view, &library, "conventional")?;
            let feat = library.find_path(&["conventional", "feat"]).ok_or("type")?;
            view.select(feat);
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| {
                view.render(frame, frame.area(), 0, Some(&library), true);
            })?;
            let outline = text_in(terminal.backend().buffer(), view.outline_area);
            let row = outline
                .lines()
                .find(|row| row.contains("▸ feat"))
                .ok_or("type row")?;
            assert!(row.trim_end().ends_with("feat"), "{row}");
            assert!(
                !outline.contains("intent"),
                "properties appear only after expansion"
            );
            press(&mut view, &library, KeyCode::Enter);
            terminal.draw(|frame| {
                view.render(frame, frame.area(), 0, Some(&library), true);
            })?;
            let outline = text_in(terminal.backend().buffer(), view.outline_area);
            assert!(outline.contains("intent"), "{outline}");
            assert!(outline.contains("required"), "{outline}");
            let row = outline
                .lines()
                .find(|row| row.contains("▾ feat"))
                .ok_or("expanded type row")?;
            assert!(row.trim_end().ends_with("feat"), "{row}");
        }
    }
    Ok(())
}
