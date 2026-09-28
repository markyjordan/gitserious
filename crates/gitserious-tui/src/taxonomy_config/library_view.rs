use gitserious_app::{TaxonomyCatalog, TaxonomyOrigin};
use gitserious_core::{CommitTypeDefinition, Taxonomy};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};

use super::{contains, pane_columns, requirement_label};
use crate::theme::{
    JET_BLACK, ZEBRA_BACKGROUND, frame_style, navigation_key_style, section_heading_style,
};

const WIDE_LIBRARY_WIDTH: u16 = 96;
const MAX_SUMMARY_ROWS: u16 = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LibraryFocus {
    Taxonomies,
    Types,
}

#[derive(Debug)]
pub(super) struct LibraryViewState {
    pub(super) focus: LibraryFocus,
    pub(super) browse_selected: usize,
    pub(super) browser_offset: usize,
    pub(super) type_selected: usize,
    pub(super) type_offset: usize,
    pub(super) property_scroll: u16,
    pub(super) browser_area: Rect,
    pub(super) type_area: Rect,
    pub(super) metadata_area: Rect,
    pub(super) pane_areas: [Rect; 3],
}

impl Default for LibraryViewState {
    fn default() -> Self {
        Self {
            focus: LibraryFocus::Taxonomies,
            browse_selected: 0,
            browser_offset: 0,
            type_selected: 0,
            type_offset: 0,
            property_scroll: 0,
            browser_area: Rect::default(),
            type_area: Rect::default(),
            metadata_area: Rect::default(),
            pane_areas: [Rect::default(); 3],
        }
    }
}

enum LibraryLayout {
    Wide {
        taxonomies: Rect,
        types: Rect,
        metadata: Rect,
    },
    Compact {
        taxonomies: Rect,
        details: Rect,
    },
}

impl LibraryLayout {
    fn new(area: Rect) -> Self {
        if area.width >= WIDE_LIBRARY_WIDTH {
            let usable = area.width.saturating_sub(2);
            let taxonomy_width = usable.saturating_mul(33) / 100;
            let type_width = usable.saturating_mul(23) / 100;
            let taxonomies = Rect::new(area.x, area.y, taxonomy_width, area.height);
            let types = Rect::new(
                taxonomies.right().saturating_add(1),
                area.y,
                type_width,
                area.height,
            );
            let metadata = Rect::new(
                types.right().saturating_add(1),
                area.y,
                usable.saturating_sub(taxonomy_width + type_width),
                area.height,
            );
            Self::Wide {
                taxonomies,
                types,
                metadata,
            }
        } else {
            let columns = pane_columns(area);
            Self::Compact {
                taxonomies: columns[0],
                details: columns[1],
            }
        }
    }
}

impl LibraryViewState {
    pub(super) fn taxonomy_at(&self, x: u16, y: u16, count: usize) -> Option<usize> {
        if !contains(self.browser_area, x, y) {
            return None;
        }
        let index = self.browser_offset + usize::from(y.saturating_sub(self.browser_area.y));
        (index < count).then_some(index)
    }

    pub(super) fn type_at(&self, x: u16, y: u16, count: usize) -> Option<usize> {
        if !contains(self.type_area, x, y) {
            return None;
        }
        let index = self.type_offset + usize::from(y.saturating_sub(self.type_area.y));
        (index < count).then_some(index)
    }

    pub(super) fn select_taxonomy(&mut self, index: usize) {
        if self.browse_selected != index {
            self.browse_selected = index;
            self.type_selected = 0;
            self.type_offset = 0;
            self.property_scroll = 0;
        }
        self.focus = LibraryFocus::Taxonomies;
    }

    pub(super) fn select_type(&mut self, index: usize) {
        if self.type_selected != index {
            self.type_selected = index;
            self.property_scroll = 0;
        }
        self.focus = LibraryFocus::Types;
    }

    pub(super) fn render(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        catalog: Option<&TaxonomyCatalog>,
        error: Option<&str>,
    ) {
        let items = catalog.map_or(&[][..], TaxonomyCatalog::taxonomies);
        self.browse_selected = self.browse_selected.min(items.len().saturating_sub(1));
        let selected = items.get(self.browse_selected);
        let definitions: &[CommitTypeDefinition] =
            selected.map_or(&[], |value| value.commit_types());
        self.type_selected = self.type_selected.min(definitions.len().saturating_sub(1));

        match LibraryLayout::new(area) {
            LibraryLayout::Wide {
                taxonomies,
                types,
                metadata,
            } => {
                self.pane_areas = [taxonomies, types, metadata];
                self.render_taxonomies(frame, taxonomies, items, catalog, error);
                self.render_types(frame, types, definitions, true);
                frame.render_widget(
                    Block::bordered()
                        .border_style(frame_style())
                        .title("Type Metadata"),
                    metadata,
                );
                self.render_metadata(
                    frame,
                    inset(metadata, 2),
                    definitions.get(self.type_selected),
                );
            }
            LibraryLayout::Compact {
                taxonomies,
                details,
            } => {
                self.pane_areas = [taxonomies, details, Rect::default()];
                self.render_taxonomies(frame, taxonomies, items, catalog, error);
                frame.render_widget(
                    Block::bordered().border_style(frame_style()).title("Types"),
                    details,
                );
                let content = inset(details, 1);
                let available = content.height.saturating_sub(4);
                let type_rows = (available / 2)
                    .max(1)
                    .min(u16::try_from(definitions.len().max(1)).unwrap_or(u16::MAX));
                let types_area = Rect::new(content.x, content.y, content.width, type_rows);
                self.render_types(frame, types_area, definitions, false);
                let heading_y = types_area.bottom().saturating_add(1);
                frame.render_widget(
                    Paragraph::new("Type Metadata").style(section_heading_style()),
                    Rect::new(
                        content.x.saturating_add(1),
                        heading_y,
                        content.width.saturating_sub(2),
                        1,
                    ),
                );
                let metadata = Rect::new(
                    content.x.saturating_add(1),
                    heading_y.saturating_add(1),
                    content.width.saturating_sub(2),
                    content.bottom().saturating_sub(heading_y.saturating_add(1)),
                );
                self.render_metadata(frame, metadata, definitions.get(self.type_selected));
            }
        }
    }

    fn render_taxonomies(
        &mut self,
        frame: &mut Frame<'_>,
        pane: Rect,
        items: &[Taxonomy],
        catalog: Option<&TaxonomyCatalog>,
        error: Option<&str>,
    ) {
        frame.render_widget(
            Block::bordered()
                .border_style(frame_style())
                .title("Taxonomies"),
            pane,
        );
        let interior = inset(pane, 1);
        if items.is_empty() {
            self.browser_area = interior;
            self.browser_offset = 0;
            frame.render_widget(
                Paragraph::new(if error.is_some() {
                    "Taxonomy library unavailable."
                } else {
                    "No taxonomies available."
                }),
                interior,
            );
            return;
        }

        let selected = &items[self.browse_selected];
        let summary_width = pane.width.saturating_sub(4);
        let description =
            Paragraph::new(selected.description().as_str()).wrap(Wrap { trim: false });
        let description_lines =
            u16::try_from(description.line_count(summary_width)).unwrap_or(u16::MAX);
        let lineage = selected
            .derived_from()
            .map(|source| format!("Forked from {}@{}", source.id(), source.version()));
        let lineage_paragraph = lineage
            .as_deref()
            .map(|text| Paragraph::new(text).wrap(Wrap { trim: false }));
        let lineage_lines = lineage_paragraph.as_ref().map_or(0, |paragraph| {
            u16::try_from(paragraph.line_count(summary_width)).unwrap_or(u16::MAX)
        });
        let available_summary_rows = MAX_SUMMARY_ROWS.min(interior.height.saturating_sub(4));
        let lineage_rows = lineage_lines.min(available_summary_rows.saturating_sub(1));
        let description_rows =
            description_lines.min(available_summary_rows.saturating_sub(lineage_rows));
        let summary_rows = description_rows + lineage_rows;
        let list_height = u16::try_from(items.len())
            .unwrap_or(u16::MAX)
            .min(
                interior
                    .height
                    .saturating_sub(summary_rows.saturating_add(1)),
            )
            .max(1);
        let list_area = Rect::new(interior.x, interior.y, interior.width, list_height);
        self.browser_area = list_area;
        let rows = items
            .iter()
            .enumerate()
            .map(|(index, taxonomy)| {
                let origin = catalog
                    .and_then(|value| value.origin(taxonomy.id()))
                    .map_or("unknown", |value| match value {
                        TaxonomyOrigin::BuiltIn => "built-in",
                        TaxonomyOrigin::Custom => "global",
                    });
                let id_width = usize::from(list_area.width).saturating_sub(origin.len() + 3);
                let id = taxonomy
                    .id()
                    .to_string()
                    .chars()
                    .take(id_width)
                    .collect::<String>();
                ListItem::new(format!(
                    "{}{id:<id_width$} {origin}",
                    if index == self.browse_selected {
                        "› "
                    } else {
                        "  "
                    }
                ))
                .style(row_style(index))
            })
            .collect::<Vec<_>>();
        let mut state = ListState::default();
        state.select(Some(self.browse_selected));
        frame.render_stateful_widget(
            List::new(rows).highlight_style(selected_style(self.focus == LibraryFocus::Taxonomies)),
            list_area,
            &mut state,
        );
        self.browser_offset = state.offset();
        if summary_rows > 0 {
            let description_area = Rect::new(
                pane.x.saturating_add(2),
                list_area.bottom().saturating_add(1),
                summary_width,
                description_rows,
            );
            frame.render_widget(description, description_area);
            if description_lines > description_rows {
                frame.buffer_mut()[(description_area.right() - 1, description_area.bottom() - 1)]
                    .set_symbol("…");
            }
            if let Some(paragraph) = lineage_paragraph {
                let lineage_area = Rect::new(
                    description_area.x,
                    description_area.bottom(),
                    summary_width,
                    lineage_rows,
                );
                frame.render_widget(paragraph, lineage_area);
                if lineage_lines > lineage_rows {
                    frame.buffer_mut()[(lineage_area.right() - 1, lineage_area.bottom() - 1)]
                        .set_symbol("…");
                }
            }
        }
    }

    fn render_types(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        definitions: &[CommitTypeDefinition],
        boxed: bool,
    ) {
        if boxed {
            frame.render_widget(
                Block::bordered().border_style(frame_style()).title("Types"),
                area,
            );
        }
        let list_area = if boxed { inset(area, 1) } else { area };
        self.type_area = list_area;
        if definitions.is_empty() {
            self.type_offset = 0;
            frame.render_widget(Paragraph::new("No commit types."), list_area);
            return;
        }
        let rows = definitions
            .iter()
            .enumerate()
            .map(|(index, definition)| {
                ListItem::new(format!(
                    "{}{}",
                    if index == self.type_selected {
                        "› "
                    } else {
                        "  "
                    },
                    definition.id()
                ))
                .style(row_style(index))
            })
            .collect::<Vec<_>>();
        let mut state = ListState::default();
        state.select(Some(self.type_selected));
        frame.render_stateful_widget(
            List::new(rows).highlight_style(selected_style(self.focus == LibraryFocus::Types)),
            list_area,
            &mut state,
        );
        self.type_offset = state.offset();
    }

    fn render_metadata(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        definition: Option<&CommitTypeDefinition>,
    ) {
        self.metadata_area = area;
        let Some(definition) = definition else {
            self.property_scroll = 0;
            frame.render_widget(Paragraph::new("No type selected."), area);
            return;
        };
        let mut detail = format!("{}\n\nProperties", definition.description());
        if definition.properties().is_empty() {
            detail.push_str("\n  No durable properties.");
        } else {
            for property in definition.properties() {
                detail.push_str(&format!(
                    "\n  {}  {}",
                    property.key(),
                    requirement_label(property.requirement())
                ));
            }
        }
        let paragraph = Paragraph::new(detail).wrap(Wrap { trim: false });
        let max_scroll = paragraph
            .line_count(area.width)
            .saturating_sub(usize::from(area.height));
        self.property_scroll = self
            .property_scroll
            .min(u16::try_from(max_scroll).unwrap_or(u16::MAX));
        frame.render_widget(paragraph.scroll((self.property_scroll, 0)), area);
    }
}

fn inset(area: Rect, horizontal: u16) -> Rect {
    Rect::new(
        area.x.saturating_add(horizontal),
        area.y.saturating_add(1),
        area.width.saturating_sub(horizontal.saturating_mul(2)),
        area.height.saturating_sub(2),
    )
}

fn row_style(index: usize) -> Style {
    Style::default().bg(if index.is_multiple_of(2) {
        JET_BLACK
    } else {
        ZEBRA_BACKGROUND
    })
}

fn selected_style(active: bool) -> Style {
    if active {
        navigation_key_style()
    } else {
        Style::default()
            .fg(Color::White)
            .bg(Color::Rgb(40, 40, 40))
            .add_modifier(Modifier::BOLD)
    }
}
