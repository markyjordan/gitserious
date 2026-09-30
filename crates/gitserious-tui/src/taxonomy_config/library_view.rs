use gitserious_app::{TaxonomyCatalog, TaxonomyOrigin};
use gitserious_core::{CommitTypeDefinition, Taxonomy};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, List, ListItem, ListState, Paragraph, Row, Table, Wrap};

use super::{contains, pane_columns, requirement_label};
use crate::theme::{
    JET_BLACK, ZEBRA_BACKGROUND, frame_style, navigation_key_style, section_heading_style,
};

const WIDE_LIBRARY_WIDTH: u16 = 96;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LibraryFocus {
    Taxonomies,
    Types,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BrowserRow {
    Header,
    Spacer,
    Taxonomy(usize),
    CreateNew,
}

#[derive(Debug)]
pub(super) struct LibraryViewState {
    pub(super) focus: LibraryFocus,
    pub(super) browse_selected: usize,
    pub(super) create_selected: bool,
    pub(super) browser_offset: usize,
    browser_rows: Vec<BrowserRow>,
    pub(super) type_selected: usize,
    pub(super) type_offset: usize,
    pub(super) property_scroll: u16,
    pub(super) taxonomy_scroll: u16,
    pub(super) browser_area: Rect,
    pub(super) type_area: Rect,
    pub(super) metadata_area: Rect,
    pub(super) taxonomy_description_area: Rect,
    pub(super) taxonomy_content_area: Rect,
    pub(super) pane_areas: [Rect; 3],
}

impl Default for LibraryViewState {
    fn default() -> Self {
        Self {
            focus: LibraryFocus::Taxonomies,
            browse_selected: 0,
            create_selected: false,
            browser_offset: 0,
            browser_rows: Vec::new(),
            type_selected: 0,
            type_offset: 0,
            property_scroll: 0,
            taxonomy_scroll: 0,
            browser_area: Rect::default(),
            type_area: Rect::default(),
            metadata_area: Rect::default(),
            taxonomy_description_area: Rect::default(),
            taxonomy_content_area: Rect::default(),
            pane_areas: [Rect::default(); 3],
        }
    }
}

struct LibraryLayout {
    taxonomies: Rect,
    taxonomy_description: Rect,
    types: Rect,
    details: Rect,
}

impl LibraryLayout {
    fn new(area: Rect, taxonomy_count: usize, type_count: usize) -> Self {
        let (left, middle, right) = if area.width >= WIDE_LIBRARY_WIDTH {
            let usable = area.width.saturating_sub(2);
            let third = usable / 3;
            let remainder = usable % 3;
            let taxonomy_width = third + u16::from(remainder > 0);
            let type_width = third + u16::from(remainder > 1);
            let left = Rect::new(area.x, area.y, taxonomy_width, area.height);
            let middle = Rect::new(
                left.right().saturating_add(1),
                area.y,
                type_width,
                area.height,
            );
            let right = Rect::new(middle.right().saturating_add(1), area.y, third, area.height);
            (left, Some(middle), right)
        } else {
            let columns = pane_columns(area);
            (columns[0], None, columns[1])
        };
        let (taxonomies, taxonomy_description) =
            stack_column(left, taxonomy_count.saturating_add(3), 4);
        let (types, details) = if let Some(middle) = middle {
            (middle, right)
        } else {
            stack_column(right, type_count.max(1), 5)
        };
        Self {
            taxonomies,
            taxonomy_description,
            types,
            details,
        }
    }
}

fn stack_column(area: Rect, row_count: usize, minimum_lower_height: u16) -> (Rect, Rect) {
    let upper_height = u16::try_from(row_count)
        .unwrap_or(u16::MAX)
        .saturating_add(2)
        .min(area.height.saturating_sub(minimum_lower_height))
        .max(3)
        .min(area.height);
    (
        Rect::new(area.x, area.y, area.width, upper_height),
        Rect::new(
            area.x,
            area.y.saturating_add(upper_height),
            area.width,
            area.height.saturating_sub(upper_height),
        ),
    )
}

impl LibraryViewState {
    pub(super) fn taxonomy_at(&self, x: u16, y: u16, count: usize) -> Option<usize> {
        if !contains(self.browser_area, x, y) {
            return None;
        }
        let row = self.browser_offset + usize::from(y.saturating_sub(self.browser_area.y));
        self.browser_rows.get(row).and_then(|row| match row {
            BrowserRow::Taxonomy(index) if *index < count => Some(*index),
            _ => None,
        })
    }

    pub(super) fn create_at(&self, x: u16, y: u16) -> bool {
        contains(self.browser_area, x, y)
            && self
                .browser_rows
                .get(self.browser_offset + usize::from(y.saturating_sub(self.browser_area.y)))
                == Some(&BrowserRow::CreateNew)
    }

    pub(super) fn type_at(&self, x: u16, y: u16, count: usize) -> Option<usize> {
        if !contains(self.type_area, x, y) {
            return None;
        }
        let index = self.type_offset + usize::from(y.saturating_sub(self.type_area.y));
        (index < count).then_some(index)
    }

    pub(super) fn select_taxonomy(&mut self, index: usize) {
        self.create_selected = false;
        if self.browse_selected != index {
            self.browse_selected = index;
            self.type_selected = 0;
            self.type_offset = 0;
            self.property_scroll = 0;
            self.taxonomy_scroll = 0;
        }
        self.focus = LibraryFocus::Taxonomies;
    }

    pub(super) fn select_create(&mut self) {
        self.create_selected = true;
        self.type_selected = 0;
        self.type_offset = 0;
        self.property_scroll = 0;
        self.taxonomy_scroll = 0;
        self.focus = LibraryFocus::Taxonomies;
    }

    pub(super) fn move_taxonomy(&mut self, down: bool, count: usize, show_create: bool) {
        if self.create_selected {
            if !down && count > 0 {
                self.select_taxonomy(count - 1);
            }
        } else if down {
            if self.browse_selected + 1 < count {
                self.select_taxonomy(self.browse_selected + 1);
            } else if show_create {
                self.select_create();
            }
        } else {
            self.select_taxonomy(self.browse_selected.saturating_sub(1));
        }
    }

    pub(super) fn select_type(&mut self, index: usize) {
        if self.create_selected {
            return;
        }
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
        let show_create = catalog.is_some_and(|value| {
            value
                .taxonomies()
                .iter()
                .all(|taxonomy| value.origin(taxonomy.id()) != Some(TaxonomyOrigin::Custom))
        });
        if self.create_selected && !show_create {
            self.browse_selected = items.len().saturating_sub(1);
            self.create_selected = false;
            self.taxonomy_scroll = 0;
        }
        self.browse_selected = self.browse_selected.min(items.len().saturating_sub(1));
        let selected = (!self.create_selected)
            .then(|| items.get(self.browse_selected))
            .flatten();
        let definitions: &[CommitTypeDefinition] =
            selected.map_or(&[], |value| value.commit_types());
        self.type_selected = self.type_selected.min(definitions.len().saturating_sub(1));

        let layout = LibraryLayout::new(
            area,
            items.len() + usize::from(show_create),
            definitions.len(),
        );
        self.pane_areas = [layout.taxonomies, layout.types, layout.details];
        self.taxonomy_description_area = layout.taxonomy_description;
        self.render_taxonomies(frame, layout.taxonomies, items, catalog, error, show_create);
        self.render_taxonomy_description(frame, layout.taxonomy_description, selected);
        self.render_types(frame, layout.types, definitions);
        frame.render_widget(
            Block::bordered()
                .border_style(frame_style())
                .title("Type Details"),
            layout.details,
        );
        self.render_metadata(frame, layout.details, definitions.get(self.type_selected));
    }

    fn render_taxonomies(
        &mut self,
        frame: &mut Frame<'_>,
        pane: Rect,
        items: &[Taxonomy],
        catalog: Option<&TaxonomyCatalog>,
        error: Option<&str>,
        show_create: bool,
    ) {
        let style = focus_frame_style(self.focus == LibraryFocus::Taxonomies);
        frame.render_widget(
            Block::bordered()
                .border_style(style)
                .title_style(style)
                .title("Taxonomies"),
            pane,
        );
        let interior = inset(pane, 1);
        if items.is_empty() {
            self.browser_area = interior;
            self.browser_offset = 0;
            self.browser_rows.clear();
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

        let list_area = interior;
        self.browser_area = list_area;
        let mut rows = Vec::with_capacity(items.len().saturating_add(3 + usize::from(show_create)));
        self.browser_rows.clear();
        for (heading, origin) in [
            ("Built-in", TaxonomyOrigin::BuiltIn),
            ("Custom", TaxonomyOrigin::Custom),
        ] {
            rows.push(taxonomy_section(heading, list_area.width));
            self.browser_rows.push(BrowserRow::Header);
            for (index, taxonomy) in items.iter().enumerate().filter(|(_, taxonomy)| {
                catalog.and_then(|value| value.origin(taxonomy.id())) == Some(origin)
            }) {
                let id_width = usize::from(list_area.width).saturating_sub(2);
                let id = taxonomy
                    .id()
                    .to_string()
                    .chars()
                    .take(id_width)
                    .collect::<String>();
                rows.push(
                    ListItem::new(format!(
                        "{}{}",
                        if !self.create_selected && index == self.browse_selected {
                            "› "
                        } else {
                            "  "
                        },
                        id
                    ))
                    .style(row_style(index)),
                );
                self.browser_rows.push(BrowserRow::Taxonomy(index));
            }
            if origin == TaxonomyOrigin::BuiltIn {
                rows.push(ListItem::new(""));
                self.browser_rows.push(BrowserRow::Spacer);
            } else if show_create {
                rows.push(
                    ListItem::new(if self.create_selected {
                        "› + create taxonomy"
                    } else {
                        "  + create taxonomy"
                    })
                    .style(row_style(items.len())),
                );
                self.browser_rows.push(BrowserRow::CreateNew);
            }
        }
        let mut state = ListState::default();
        state.select(self.browser_rows.iter().position(|row| {
            *row == if self.create_selected {
                BrowserRow::CreateNew
            } else {
                BrowserRow::Taxonomy(self.browse_selected)
            }
        }));
        frame.render_stateful_widget(
            List::new(rows).highlight_style(selected_style(self.focus == LibraryFocus::Taxonomies)),
            list_area,
            &mut state,
        );
        self.browser_offset = state.offset();
    }

    fn render_taxonomy_description(
        &mut self,
        frame: &mut Frame<'_>,
        pane: Rect,
        selected: Option<&Taxonomy>,
    ) {
        frame.render_widget(
            Block::bordered()
                .border_style(frame_style())
                .title("Taxonomy Description"),
            pane,
        );
        let content = inset(pane, 1);
        self.taxonomy_content_area = content;
        let Some(selected) = selected else {
            self.taxonomy_scroll = 0;
            frame.render_widget(
                Paragraph::new(if self.create_selected {
                    "n/a"
                } else {
                    "No taxonomy selected."
                }),
                content,
            );
            return;
        };
        let mut summary = selected.description().to_string();
        if let Some(source) = selected.derived_from() {
            summary.push_str(&format!(
                "\nForked from {}@{}",
                source.id(),
                source.version()
            ));
        }
        let paragraph = Paragraph::new(summary).wrap(Wrap { trim: false });
        let total = u16::try_from(paragraph.line_count(content.width)).unwrap_or(u16::MAX);
        self.taxonomy_scroll = self
            .taxonomy_scroll
            .min(total.saturating_sub(content.height));
        frame.render_widget(paragraph.scroll((self.taxonomy_scroll, 0)), content);
    }

    fn render_types(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        definitions: &[CommitTypeDefinition],
    ) {
        let style = focus_frame_style(self.focus == LibraryFocus::Types);
        frame.render_widget(
            Block::bordered()
                .border_style(style)
                .title_style(style)
                .title("Types"),
            area,
        );
        let list_area = inset(area, 1);
        self.type_area = list_area;
        if definitions.is_empty() {
            self.type_offset = 0;
            frame.render_widget(
                Paragraph::new(if self.create_selected {
                    "n/a"
                } else {
                    "No commit types."
                }),
                list_area,
            );
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
        pane: Rect,
        definition: Option<&CommitTypeDefinition>,
    ) {
        let area = inset(pane, 1);
        self.metadata_area = area;
        let Some(definition) = definition else {
            self.property_scroll = 0;
            frame.render_widget(
                Paragraph::new(if self.create_selected {
                    "n/a"
                } else {
                    "No type selected."
                }),
                area,
            );
            return;
        };
        let description = Paragraph::new(definition.description()).wrap(Wrap { trim: false });
        let description_rows =
            u16::try_from(description.line_count(area.width)).unwrap_or(u16::MAX);
        let description_end = description_rows.saturating_add(1);
        let properties_heading = description_end.saturating_add(1);
        let table_start = properties_heading.saturating_add(1);
        let property_rows = u16::try_from(definition.properties().len().max(1)).unwrap_or(u16::MAX);
        let total = table_start.saturating_add(property_rows);
        let max_scroll = total.saturating_sub(area.height);
        self.property_scroll = self.property_scroll.min(max_scroll);
        let scroll = self.property_scroll;
        if scroll == 0 {
            frame.render_widget(
                Paragraph::new(section_line("Description", area.width)),
                Rect::new(area.x, area.y, area.width, 1),
            );
        }
        if scroll < description_end && 1 < scroll.saturating_add(area.height) {
            let first_row = scroll.max(1);
            let y = area.y.saturating_add(first_row.saturating_sub(scroll));
            frame.render_widget(
                description.scroll((first_row - 1, 0)),
                Rect::new(
                    area.x,
                    y,
                    area.width,
                    description_end
                        .saturating_sub(first_row)
                        .min(area.bottom().saturating_sub(y)),
                ),
            );
        }
        if properties_heading >= scroll && properties_heading - scroll < area.height {
            frame.render_widget(
                Paragraph::new(section_line("Properties", area.width)),
                Rect::new(area.x, area.y + properties_heading - scroll, area.width, 1),
            );
        }
        if table_start >= scroll.saturating_add(area.height) {
            return;
        }
        let table_y = area.y.saturating_add(table_start.saturating_sub(scroll));
        let table_area = Rect::new(
            area.x,
            table_y,
            area.width,
            area.bottom().saturating_sub(table_y),
        );
        let skipped = usize::from(scroll.saturating_sub(table_start));
        if definition.properties().is_empty() {
            if skipped == 0 {
                frame.render_widget(Paragraph::new("No durable properties."), table_area);
            }
            return;
        }
        let property_width = definition
            .properties()
            .iter()
            .map(|property| property.key().as_str().chars().count())
            .max()
            .unwrap_or(1);
        let requirement_width = definition
            .properties()
            .iter()
            .map(|property| requirement_label(property.requirement()).len())
            .max()
            .unwrap_or(1);
        let requirement_width = u16::try_from(requirement_width).unwrap_or(u16::MAX);
        let name_width = u16::try_from(property_width).unwrap_or(u16::MAX).min(
            area.width
                .saturating_sub(requirement_width.saturating_add(2)),
        );
        let rows = definition
            .properties()
            .iter()
            .skip(skipped)
            .map(|property| {
                Row::new(vec![
                    Cell::from(property.key().as_str()),
                    Cell::from(requirement_label(property.requirement())),
                ])
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(name_width),
                    Constraint::Length(requirement_width),
                ],
            )
            .column_spacing(2),
            table_area,
        );
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

fn taxonomy_section(title: &'static str, width: u16) -> ListItem<'static> {
    ListItem::new(section_line(title, width))
}

fn section_line(title: &'static str, width: u16) -> Line<'static> {
    let rule_width = usize::from(width).saturating_sub(title.len() + 1);
    Line::from(vec![
        Span::styled(title, section_heading_style()),
        Span::styled(format!(" {}", "⠒".repeat(rule_width)), frame_style()),
    ])
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

fn focus_frame_style(active: bool) -> Style {
    if active {
        Style::default().fg(Color::Yellow)
    } else {
        frame_style()
    }
}
