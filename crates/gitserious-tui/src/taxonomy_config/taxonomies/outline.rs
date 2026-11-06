use std::collections::BTreeSet;

use gitserious_app::{LibraryDraft, NodeId, NodeRef, TaxonomyNode, TaxonomyOrigin, TypeNode};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::summary_scroll::{SummaryScroll, Target};
use super::{Chip, Chips, DIM, fit, panes, policy_style};
use crate::theme::theme;

pub(super) const ADD_TYPE: &str = "+ add type";
pub(super) const ADD_PROPERTY: &str = "+ add property";
const NEW_TAXONOMY: &str = "+ new taxonomy";
const TEXT: Style = Style::new().fg(theme().text);

/// What a row stands for. The add rows are not drafts, so they name the node they add to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in super::super) enum RowKey {
    Node(NodeId),
    AddType(NodeId),
    AddProperty(NodeId),
    NewTaxonomy,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Row<'a> {
    Node(NodeRef<'a>),
    AddType(&'a TaxonomyNode),
    AddProperty(&'a TaxonomyNode, &'a TypeNode),
    /// The Library's last row, which creates a blank taxonomy.
    NewTaxonomy,
}

impl<'a> Row<'a> {
    pub(super) fn key(self) -> RowKey {
        match self {
            Self::Node(node) => RowKey::Node(node.id().clone()),
            Self::AddType(taxonomy) => RowKey::AddType(taxonomy.id().clone()),
            Self::AddProperty(_, commit_type) => RowKey::AddProperty(commit_type.id().clone()),
            Self::NewTaxonomy => RowKey::NewTaxonomy,
        }
    }

    /// The row's node, or for an add row the node it adds to. The new taxonomy row has none.
    pub(super) const fn node(self) -> Option<NodeRef<'a>> {
        match self {
            Self::Node(node) => Some(node),
            Self::AddType(taxonomy) => Some(NodeRef::Taxonomy(taxonomy)),
            Self::AddProperty(taxonomy, commit_type) => Some(NodeRef::Type {
                taxonomy,
                commit_type,
            }),
            Self::NewTaxonomy => None,
        }
    }

    /// Names from the taxonomy down to this row; an add row ends with its label.
    #[cfg(test)]
    pub(super) fn names(self) -> Vec<String> {
        let mut names: Vec<String> = self
            .node()
            .map(|node| node.names().into_iter().map(str::to_owned).collect())
            .unwrap_or_default();
        match self {
            Self::Node(_) => {}
            Self::AddType(_) => names.push(ADD_TYPE.into()),
            Self::AddProperty(..) => names.push(ADD_PROPERTY.into()),
            Self::NewTaxonomy => names.push(NEW_TAXONOMY.into()),
        }
        names
    }
}

/// Visible rows in display order: every taxonomy, the types of each expanded taxonomy, and the
/// properties of each expanded type. A custom taxonomy or type ends with its add row, and the
/// Library ends with its new taxonomy row. Read-only views omit all creation rows.
pub(super) fn rows<'a>(
    library: &'a LibraryDraft,
    expanded: &BTreeSet<NodeId>,
    read_only: bool,
) -> Vec<Row<'a>> {
    let mut rows = Vec::new();
    for taxonomy in library.taxonomies() {
        rows.push(Row::Node(NodeRef::Taxonomy(taxonomy)));
        if !expanded.contains(taxonomy.id()) {
            continue;
        }
        let custom = !read_only && taxonomy.origin() == TaxonomyOrigin::Custom;
        for commit_type in taxonomy.types() {
            rows.push(Row::Node(NodeRef::Type {
                taxonomy,
                commit_type,
            }));
            if expanded.contains(commit_type.id()) {
                rows.extend(commit_type.properties().iter().map(|property| {
                    Row::Node(NodeRef::Property {
                        taxonomy,
                        commit_type,
                        property,
                    })
                }));
                if custom {
                    rows.push(Row::AddProperty(taxonomy, commit_type));
                }
            }
        }
        if custom {
            rows.push(Row::AddType(taxonomy));
        }
    }
    if !read_only && !library.is_project() {
        rows.push(Row::NewTaxonomy);
    }
    rows
}

/// Whether the row opens. Editable custom nodes also open when empty to show their add row.
pub(super) fn has_children(row: Row<'_>, read_only: bool) -> bool {
    let Row::Node(node) = row else {
        return false;
    };
    let custom = !read_only && node.taxonomy().origin() == TaxonomyOrigin::Custom;
    match node {
        NodeRef::Taxonomy(taxonomy) => custom || !taxonomy.types().is_empty(),
        NodeRef::Type { commit_type, .. } => custom || !commit_type.properties().is_empty(),
        NodeRef::Property { .. } => false,
    }
}

/// The cursor's row, else the row of its nearest visible ancestor, else the first row.
pub(super) fn resolve(rows: &[Row<'_>], cursor: Option<&RowKey>, library: &LibraryDraft) -> usize {
    let Some(cursor) = cursor else {
        return 0;
    };
    if let Some(index) = rows.iter().position(|row| &row.key() == cursor) {
        return index;
    }
    let mut id = match cursor {
        RowKey::Node(id) | RowKey::AddType(id) | RowKey::AddProperty(id) => Some(id.clone()),
        RowKey::NewTaxonomy => None,
    };
    while let Some(current) = id {
        if let Some(index) = rows
            .iter()
            .position(|row| row.key() == RowKey::Node(current.clone()))
        {
            return index;
        }
        id = library.parent(&current);
    }
    0
}

/// The cursor row's style: a bone bar with ground-coloured bold text while the outline has
/// focus, else a raised row that keeps each word's colour.
fn cursor_style(focused: bool) -> Style {
    if focused {
        Style::new()
            .fg(theme().ground)
            .bg(theme().cursor)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new().bg(theme().band)
    }
}

/// Draws the rows and returns the area they fill, for mouse hit-testing. When they overflow,
/// the last row counts the rows below.
#[expect(
    clippy::too_many_arguments,
    reason = "the outline draws from the view's state and the Library's derived rows and chips"
)]
pub(super) fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    rows: &[Row<'_>],
    expanded: &BTreeSet<NodeId>,
    chips: &Chips<'_>,
    selected: usize,
    offset: &mut usize,
    focused: bool,
    summary_scroll: &mut SummaryScroll,
    animation_viewport: Option<Rect>,
) -> Rect {
    let inner = Rect {
        height: panes::shown(rows.len(), area.height),
        ..area
    };
    let height = usize::from(inner.height);
    if selected < *offset {
        *offset = selected;
    } else if height > 0 && selected >= *offset + height {
        *offset = selected + 1 - height;
    }
    *offset = (*offset).min(rows.len().saturating_sub(height));
    panes::render_more_below(frame, area, rows.len().saturating_sub(*offset + height));
    let width = usize::from(inner.width).saturating_sub(1);
    let columns = Columns::new(chips.library);
    let cursor = cursor_style(focused);
    let mut target = None;
    let lines = rows
        .iter()
        .enumerate()
        .skip(*offset)
        .take(height)
        .map(|(index, &row)| {
            let open = has_children(row, chips.read_only)
                && row.node().is_some_and(|node| expanded.contains(node.id()));
            let line = row_line(row, open, chips, &columns, index == selected, width);
            if index != selected {
                return line;
            }
            let row_area = Rect::new(
                inner.x,
                inner.y + u16::try_from(index - *offset).unwrap_or(0),
                inner.width,
                1,
            );
            frame.buffer_mut().set_style(row_area, cursor);
            if let Some(viewport) = animation_viewport {
                let summary_style = if focused { cursor } else { cursor.patch(DIM) };
                target = summary_target(
                    row,
                    open,
                    Rect {
                        width: inner.width.saturating_sub(1),
                        ..row_area
                    },
                    viewport,
                    summary_style,
                    chips.read_only,
                );
            }
            if !focused {
                return line;
            }
            Line::from(
                line.spans
                    .into_iter()
                    .map(|span| {
                        let style = span.style.patch(cursor);
                        span.style(style)
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), inner);
    summary_scroll.render(frame.buffer_mut(), target);
    inner
}

/// Widths that line up the taxonomy rows' origins, type counts, and versions across the Library.
struct Columns {
    name: usize,
    origin: usize,
    version: usize,
    count: usize,
}

impl Columns {
    fn new(library: &LibraryDraft) -> Self {
        let taxonomies = library.taxonomies();
        let widest = |width: &dyn Fn(&TaxonomyNode) -> usize| {
            taxonomies.iter().map(width).max().unwrap_or(0)
        };
        Self {
            name: widest(&|taxonomy| Span::raw(taxonomy.name()).width()),
            origin: widest(&|taxonomy| origin_label(library, taxonomy).len()),
            version: widest(&|taxonomy| version(library, taxonomy).len()),
            count: widest(&|taxonomy| taxonomy.types().len().to_string().len()),
        }
    }
}

fn version(library: &LibraryDraft, taxonomy: &TaxonomyNode) -> String {
    library
        .version(taxonomy.id())
        .map_or_else(String::new, |version| format!("v{version}"))
}

/// Pads `name` to the widest of `names` plus two spaces.
fn column<'a>(name: &str, names: impl Iterator<Item = &'a str>) -> String {
    let width = names.map(|name| Span::raw(name).width()).max().unwrap_or(0) + 2;
    pad(name, width)
}

fn pad(text: &str, width: usize) -> String {
    format!(
        "{text}{}",
        " ".repeat(width.saturating_sub(Span::raw(text).width()))
    )
}

fn summary(commit_type: &TypeNode) -> String {
    commit_type
        .properties()
        .iter()
        .map(gitserious_app::PropertyNode::name)
        .collect::<Vec<_>>()
        .join(" · ")
}

fn last_type(taxonomy: &TaxonomyNode, commit_type: &TypeNode, read_only: bool) -> bool {
    (read_only || taxonomy.origin() == TaxonomyOrigin::BuiltIn)
        && taxonomy
            .types()
            .last()
            .is_some_and(|last| last.id() == commit_type.id())
}

const fn branch(last: bool) -> &'static str {
    if last { "└─ " } else { "├─ " }
}

/// Project rows identify the configured default in the existing ownership column.
fn origin_label(library: &LibraryDraft, taxonomy: &TaxonomyNode) -> &'static str {
    if library.is_project() {
        if library.default_taxonomy().as_str() == taxonomy.name() {
            "default"
        } else {
            "project"
        }
    } else {
        taxonomy.origin().as_str()
    }
}

fn property_branch(
    taxonomy: &TaxonomyNode,
    commit_type: &TypeNode,
    last: bool,
    read_only: bool,
) -> String {
    let ancestor = if last_type(taxonomy, commit_type, read_only) {
        "   "
    } else {
        "│  "
    };
    format!("{ancestor}{}", branch(last))
}

fn summary_target(
    row: Row<'_>,
    open: bool,
    row_area: Rect,
    viewport: Rect,
    style: Style,
    read_only: bool,
) -> Option<Target> {
    let Row::Node(NodeRef::Type {
        taxonomy,
        commit_type,
    }) = row
    else {
        return None;
    };
    if open {
        return None;
    }
    let name = column(
        commit_type.name(),
        taxonomy.types().iter().map(TypeNode::name),
    );
    let prefix = format!(
        " {}▸ {name}",
        branch(last_type(taxonomy, commit_type, read_only))
    );
    let prefix_width = Span::raw(&prefix).width();
    let room = usize::from(row_area.width).saturating_sub(prefix_width);
    let summary = summary(commit_type);
    if room == 0 || Span::raw(&summary).width() <= room {
        return None;
    }
    Some(Target {
        node: commit_type.id().clone(),
        prefix,
        summary,
        viewport,
        area: Rect::new(
            row_area.x + u16::try_from(prefix_width).ok()?,
            row_area.y,
            u16::try_from(room).ok()?,
            1,
        ),
        style,
    })
}

/// A taxonomy row's right side: its type count and version, or that it is invalid. Narrow
/// rows drop the count, then the version, before the name gives way.
fn taxonomy_status(
    library: &LibraryDraft,
    taxonomy: &TaxonomyNode,
    chip: Option<Chip>,
    columns: &Columns,
    selected: bool,
    room: usize,
) -> Vec<Span<'static>> {
    let fits = |spans: &[Span<'static>]| Line::from(spans.to_vec()).width() < room;
    if let Some(chip) = chip {
        let invalid = vec![chip.span()];
        return if fits(&invalid) { invalid } else { Vec::new() };
    }
    let style = if selected { TEXT } else { DIM };
    let count = taxonomy.types().len();
    let version = Span::styled(
        format!(
            "{:>width$}",
            version(library, taxonomy),
            width = columns.version
        ),
        style,
    );
    let full = vec![
        Span::styled(
            format!(
                "{count:>width$} {}",
                if count == 1 { "type " } else { "types" },
                width = columns.count
            ),
            style,
        ),
        Span::raw("  "),
        version.clone(),
    ];
    [full, vec![version]]
        .into_iter()
        .find(|spans| fits(spans))
        .unwrap_or_default()
}

/// One row: a taxonomy names its origin and status, a closed type its property keys, and a
/// property its policy.
fn row_line(
    row: Row<'_>,
    open: bool,
    chips: &Chips<'_>,
    columns: &Columns,
    selected: bool,
    width: usize,
) -> Line<'static> {
    let gutter = Span::raw(" ");
    let marker = Span::styled(
        if chips.read_only && !has_children(row, true) {
            "  "
        } else if open {
            "▾ "
        } else {
            "▸ "
        },
        Style::new().fg(theme().dim),
    );
    let (left, right) = match row {
        Row::NewTaxonomy => (vec![gutter, Span::styled(NEW_TAXONOMY, DIM)], Vec::new()),
        Row::AddType(_) => (
            vec![
                gutter,
                Span::styled(branch(true), Style::new().fg(theme().dim)),
                Span::styled(ADD_TYPE, DIM),
            ],
            Vec::new(),
        ),
        Row::AddProperty(taxonomy, commit_type) => (
            vec![
                gutter,
                Span::styled(
                    property_branch(taxonomy, commit_type, true, chips.read_only),
                    Style::new().fg(theme().dim),
                ),
                Span::styled(ADD_PROPERTY, DIM),
            ],
            Vec::new(),
        ),
        Row::Node(NodeRef::Taxonomy(taxonomy)) => {
            let bold = TEXT.add_modifier(Modifier::BOLD);
            // The design sets the origin three columns past the widest name. A name too long
            // for the row gives way before its origin does.
            let name_width = (columns.name + 2).min(width.saturating_sub(4 + columns.origin));
            let mut left = vec![gutter, marker];
            left.extend(
                fit(
                    vec![Span::styled(taxonomy.name().to_owned(), bold)],
                    Vec::new(),
                    name_width,
                )
                .spans,
            );
            left.push(Span::styled(
                format!(
                    " {}",
                    pad(origin_label(chips.library, taxonomy), columns.origin)
                ),
                bold,
            ));
            let room = width.saturating_sub(Line::from(left.clone()).width());
            let right = taxonomy_status(
                chips.library,
                taxonomy,
                chips.of(taxonomy.id()),
                columns,
                selected,
                room,
            );
            (left, right)
        }
        Row::Node(NodeRef::Type {
            taxonomy,
            commit_type,
        }) => {
            let name = column(
                commit_type.name(),
                taxonomy.types().iter().map(TypeNode::name),
            );
            let mut left = vec![
                gutter,
                Span::styled(
                    branch(last_type(taxonomy, commit_type, chips.read_only)),
                    Style::new().fg(theme().dim),
                ),
                marker,
                Span::styled(name, TEXT),
            ];
            if !open {
                left.push(Span::styled(summary(commit_type), DIM));
            }
            (left, Vec::new())
        }
        Row::Node(NodeRef::Property {
            taxonomy,
            commit_type,
            property,
        }) => {
            let siblings = commit_type.properties();
            let last = (chips.read_only || taxonomy.origin() == TaxonomyOrigin::BuiltIn)
                && siblings
                    .last()
                    .is_some_and(|last| last.id() == property.id());
            let name = column(
                property.name(),
                siblings.iter().map(gitserious_app::PropertyNode::name),
            );
            (
                vec![
                    gutter,
                    Span::styled(
                        property_branch(taxonomy, commit_type, last, chips.read_only),
                        Style::new().fg(theme().dim),
                    ),
                    Span::styled(name, TEXT),
                    Span::styled(property.policy().label(), policy_style(property.policy())),
                ],
                Vec::new(),
            )
        }
    };
    fit(left, right, width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_columns_and_ellipsis_preserve_complete_graphemes() {
        assert_eq!(column("界", ["a", "界"].into_iter()), "界  ");
        assert_eq!(
            column("e\u{301}", ["👩‍💻", "e\u{301}"].into_iter()),
            "e\u{301}   "
        );
        assert_eq!(
            fit(vec![Span::raw("a👩‍💻e\u{301}界")], vec![], 3).to_string(),
            "a… "
        );
        assert_eq!(
            fit(vec![Span::raw("e\u{301}界abc")], vec![], 4).to_string(),
            "e\u{301}界…"
        );
    }
}
