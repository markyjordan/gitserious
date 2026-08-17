use gitserious_core::{CommitTypeDefinition, PropertyRequirement};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, Widget, Wrap,
};
use tui_textarea::{CursorMove, TextArea, WrapMode};

use super::state::{
    AuthoringSession, ConfirmationAction, ConfirmationButtons, FieldId, FieldKind, FieldStatus,
    SCOPE_VALUE_LINE, Stage,
};

const MINIMUM_WIDTH: u16 = 60;
const MINIMUM_HEIGHT: u16 = 18;
const COMPOSER_MINIMUM_HEIGHT: u16 = 22;
const JET_BLACK: Color = Color::Rgb(0, 0, 0);
const ZEBRA_BACKGROUND: Color = Color::Rgb(16, 16, 16);
const MAX_EDITOR_INNER_WIDTH: u16 = 80;
const FIELD_COLUMN_SPACING: u16 = 2;
const FIELD_MARKER_WIDTH: u16 = 1;
const FIELD_REQUIREMENT_WIDTH: u16 = 11;
const MINIMUM_EDITOR_HEIGHT: u16 = 3;
const TERMINAL_EDGE_CURSOR: &str = "█";
const COMPOSER_NON_CONTEXT_ROWS: u16 = 8;
const COMPOSER_FRAME_WIDTH_OVERHEAD: u16 = 10;
const WIDE_COMPOSER_BREAKPOINT: u16 = 101;
const SCROLLBAR_WIDTH: u16 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Pane {
    region: Rect,
    content: Rect,
}

impl Pane {
    const fn new(region: Rect) -> Self {
        Self {
            region,
            content: inset_horizontally(region, 1),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ComposerFrame {
    outer: Rect,
    properties_heading: Pane,
    description_heading: Pane,
    properties: Pane,
    description: Pane,
    editor: Pane,
    validation: Pane,
    chrome: ComposerChrome,
    editor_rule_outer: Rect,
    validation_separator_y: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ComposerChrome {
    Compact {
        split_x: u16,
        heading_separator_y: u16,
        context_separator_y: u16,
    },
    Wide {
        split_x: u16,
        properties_separator_y: u16,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CursorStatus {
    column: u16,
    wrap_width: u16,
    content_rows: u16,
    viewport_top: u16,
    viewport_height: u16,
}

pub(crate) fn render(frame: &mut Frame<'_>, session: &mut AuthoringSession<'_>) {
    let area = frame.area();
    session.confirmation_buttons = None;
    frame.render_widget(Block::default().style(Style::default().bg(JET_BLACK)), area);
    let minimum_height = if session.visible_stage() == Stage::Compose {
        COMPOSER_MINIMUM_HEIGHT
    } else {
        MINIMUM_HEIGHT
    };
    let minimum_width = if session.visible_stage() == Stage::Compose {
        composer_minimum_width(session.definition())
    } else {
        MINIMUM_WIDTH
    };
    session.too_small = area.width < minimum_width || area.height < minimum_height;
    if session.too_small {
        if session.stage == Stage::Confirm {
            session.confirmation_buttons = Some(render_discard_confirmation(
                frame,
                area,
                "Discard this draft?",
            ));
        } else {
            render_centered_notice(
                frame,
                area,
                "Terminal too small",
                "Resize or press esc/q to cancel",
                None,
            );
        }
        normalize_background(frame, area);
        return;
    }

    match session.visible_stage() {
        Stage::SelectType => render_picker(frame, area, session),
        Stage::Compose => render_composer(frame, area, session),
        Stage::Review => render_review(frame, area, session),
        Stage::Confirm => unreachable!("confirmation resolves to its underlying stage"),
    }
    if session.stage == Stage::Confirm {
        render_confirmation(frame, area, session);
    }
    normalize_background(frame, area);
}

fn normalize_background(frame: &mut Frame<'_>, area: Rect) {
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let cell = &mut frame.buffer_mut()[(x, y)];
            if matches!(cell.bg, Color::Reset | Color::Black) {
                cell.set_bg(JET_BLACK);
            }
        }
    }
}

fn render_picker(frame: &mut Frame<'_>, area: Rect, session: &AuthoringSession<'_>) {
    let sections = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);
    render_stage_header(frame, sections[0], "Select commit type", 1);
    let outer = inset_horizontally(sections[2], 1);
    let list_area = Rect::new(
        outer.x.saturating_add(2),
        outer.y.saturating_add(1),
        outer.width.saturating_sub(4),
        outer.height.saturating_sub(2),
    );
    frame.render_widget(Block::bordered().border_style(frame_style()), outer);
    let items = session
        .definitions
        .iter()
        .map(|definition| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    definition.id().as_str(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(" — "),
                Span::raw(definition.description()),
            ]))
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .highlight_symbol("› ")
        .highlight_style(navigation_key_style());
    let mut state = ListState::default();
    state.select(Some(session.selected_type));
    frame.render_stateful_widget(list, list_area, &mut state);
    render_navigation_row(
        frame,
        sections[4],
        &[("↑/↓", "move"), ("enter", "select"), ("esc/q", "cancel")],
    );
}

fn render_composer(frame: &mut Frame<'_>, area: Rect, session: &mut AuthoringSession<'_>) {
    let sections = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);
    let header =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(sections[0]);
    render_stage_header(frame, header[0], "Compose commit message", 2);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("Type: "),
            Span::styled(
                session.definition().id().as_str(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ])),
        header[1],
    );

    let composer_frame = composer_frame(
        inset_horizontally(sections[2], 1),
        session,
        area.width >= WIDE_COMPOSER_BREAKPOINT,
    );
    render_composer_frame(frame, composer_frame);
    render_section_heading(
        frame,
        composer_frame.properties_heading.content,
        "Message Properties",
    );
    render_section_heading(
        frame,
        composer_frame.description_heading.content,
        "Property Description",
    );
    render_field_hud(frame, composer_frame.properties.content, session);
    render_field_description(frame, composer_frame.description.content, session);
    let editor_content = Rect::new(
        composer_frame.editor.content.x,
        composer_frame.editor.content.y,
        composer_frame
            .editor
            .content
            .width
            .saturating_sub(SCROLLBAR_WIDTH),
        composer_frame.editor.content.height,
    );
    let scrollbar_area = Rect::new(
        composer_frame
            .editor
            .content
            .right()
            .saturating_sub(SCROLLBAR_WIDTH),
        composer_frame.editor.content.y,
        SCROLLBAR_WIDTH.min(composer_frame.editor.content.width),
        composer_frame.editor.content.height,
    );
    let cursor_status = render_document_editor(
        frame,
        editor_content,
        composer_frame.editor_rule_outer,
        session,
    );
    render_editor_scrollbar(frame, scrollbar_area, cursor_status);

    let help: &[_] = &[("↑/↓", "move"), ("esc", "back"), ("ctrl+s", "review")];
    render_validation_row(
        frame,
        composer_frame.validation.content,
        session.composer.issues.first(),
        cursor_status,
    );
    render_navigation_row(frame, sections[4], help);
}

fn composer_frame(area: Rect, session: &AuthoringSession<'_>, wide: bool) -> ComposerFrame {
    if wide {
        wide_composer_frame(area, session)
    } else {
        compact_composer_frame(area, session)
    }
}

fn compact_composer_frame(area: Rect, session: &AuthoringSession<'_>) -> ComposerFrame {
    let inner = Rect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(1),
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );
    let context_width = inner.width.saturating_sub(1);
    let properties_pane_width = field_hud_content_width(session.definition()).saturating_add(2);
    let description_pane_width = context_width.saturating_sub(properties_pane_width);
    let description_width = description_pane_width.saturating_sub(2);
    let desired_context_height = context_content_height(description_width, session);
    let maximum_context_height = inner.height.saturating_sub(COMPOSER_NON_CONTEXT_ROWS);
    let context_height = desired_context_height.min(maximum_context_height).max(1);
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(context_height),
        Constraint::Length(1),
        Constraint::Min(MINIMUM_EDITOR_HEIGHT),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);
    let headings = split_context_row(rows[0], properties_pane_width);
    let context = split_context_row(rows[2], properties_pane_width);
    let split_x = headings[1].x;
    ComposerFrame {
        outer: area,
        properties_heading: Pane::new(headings[0]),
        description_heading: Pane::new(headings[2]),
        properties: Pane::new(context[0]),
        description: Pane::new(context[2]),
        editor: Pane::new(rows[4]),
        validation: Pane::new(rows[6]),
        chrome: ComposerChrome::Compact {
            split_x,
            heading_separator_y: rows[1].y,
            context_separator_y: rows[3].y,
        },
        editor_rule_outer: area,
        validation_separator_y: rows[5].y,
    }
}

fn wide_composer_frame(area: Rect, session: &AuthoringSession<'_>) -> ComposerFrame {
    let inner = Rect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(1),
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );
    let vertical = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);
    let main = vertical[0];
    let properties_pane_width = field_hud_content_width(session.definition()).saturating_add(2);
    let columns = split_context_row(main, properties_pane_width);
    let split_x = columns[1].x;
    let properties_height = u16::try_from(session.composer.hud_fields(session.definition()).len())
        .unwrap_or(u16::MAX)
        .saturating_add(1)
        .min(main.height.saturating_sub(3))
        .max(1);
    let left_rows = Layout::vertical([
        Constraint::Length(properties_height),
        Constraint::Length(1),
        Constraint::Min(2),
    ])
    .split(columns[0]);
    let properties_rows =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(left_rows[0]);
    let description_rows =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(left_rows[2]);
    let editor_rule_outer = Rect::new(
        split_x,
        area.y,
        area.right().saturating_sub(split_x),
        vertical[1].y.saturating_sub(area.y).saturating_add(1),
    );
    ComposerFrame {
        outer: area,
        properties_heading: Pane::new(properties_rows[0]),
        description_heading: Pane::new(description_rows[0]),
        properties: Pane::new(properties_rows[1]),
        description: Pane::new(description_rows[1]),
        editor: Pane::new(columns[2]),
        validation: Pane::new(vertical[2]),
        chrome: ComposerChrome::Wide {
            split_x,
            properties_separator_y: left_rows[1].y,
        },
        editor_rule_outer,
        validation_separator_y: vertical[1].y,
    }
}

fn split_context_row(area: Rect, properties_width: u16) -> std::rc::Rc<[Rect]> {
    Layout::horizontal([
        Constraint::Length(properties_width),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .split(area)
}

fn render_composer_frame(frame: &mut Frame<'_>, composer: ComposerFrame) {
    let style = frame_style();
    frame.render_widget(Block::bordered().border_style(style), composer.outer);
    match composer.chrome {
        ComposerChrome::Compact {
            split_x,
            heading_separator_y,
            context_separator_y,
        } => {
            set_rule_cell(frame, split_x, composer.outer.y, "┬", style);
            for y in composer.outer.y.saturating_add(1)..context_separator_y {
                set_rule_cell(frame, split_x, y, "│", style);
            }
            render_frame_rule(
                frame,
                composer.outer,
                heading_separator_y,
                Some((split_x, "┼")),
            );
            render_frame_rule(
                frame,
                composer.outer,
                context_separator_y,
                Some((split_x, "┴")),
            );
            render_frame_rule(frame, composer.outer, composer.validation_separator_y, None);
        }
        ComposerChrome::Wide {
            split_x,
            properties_separator_y,
        } => {
            set_rule_cell(frame, split_x, composer.outer.y, "┬", style);
            for y in composer.outer.y.saturating_add(1)..composer.validation_separator_y {
                set_rule_cell(frame, split_x, y, "│", style);
            }
            render_partial_frame_rule(frame, composer.outer.x, split_x, properties_separator_y);
            render_frame_rule(
                frame,
                composer.outer,
                composer.validation_separator_y,
                Some((split_x, "┴")),
            );
        }
    }
}

fn render_partial_frame_rule(frame: &mut Frame<'_>, start: u16, end: u16, y: u16) {
    let style = frame_style();
    merge_rule_cell(frame, start, y, "├", style);
    for x in start.saturating_add(1)..end {
        merge_rule_cell(frame, x, y, "─", style);
    }
    merge_rule_cell(frame, end, y, "┤", style);
}

fn render_field_hud(frame: &mut Frame<'_>, area: Rect, session: &AuthoringSession<'_>) {
    let definition = session.definition();
    let current = session
        .composer
        .current_field(definition)
        .map(FieldKind::id);
    let fields = session.composer.hud_fields(definition);
    let name_width = field_hud_name_width(definition);
    let rows = fields
        .into_iter()
        .enumerate()
        .map(|(index, field)| {
            let marker = match field.status {
                FieldStatus::Invalid => Cell::from("!").style(Style::default().fg(Color::Red)),
                FieldStatus::Complete => Cell::from("✓").style(Style::default().fg(Color::Green)),
                FieldStatus::Incomplete => {
                    Cell::from("○").style(Style::default().fg(Color::DarkGray))
                }
            };
            let row_style = if current == Some(field.id) {
                navigation_key_style()
            } else {
                Style::default().bg(if index % 2 == 0 {
                    JET_BLACK
                } else {
                    ZEBRA_BACKGROUND
                })
            };
            let (name, requirement) = field_columns(field.id, definition);
            Row::new(vec![marker, Cell::from(name), Cell::from(requirement)]).style(row_style)
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(FIELD_MARKER_WIDTH),
                Constraint::Length(name_width),
                Constraint::Length(FIELD_REQUIREMENT_WIDTH),
            ],
        )
        .column_spacing(FIELD_COLUMN_SPACING),
        area,
    );
}

fn render_field_description(frame: &mut Frame<'_>, area: Rect, session: &AuthoringSession<'_>) {
    let definition = session.definition().clone();
    let current = session.composer.current_field(&definition);
    let (title, description) = current.map_or_else(
        || {
            (
                " Commit form ".to_owned(),
                "Edit values beneath the schema-generated field headers.".to_owned(),
            )
        },
        |kind| field_metadata(kind, &definition),
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                title.trim().to_owned(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Line::from(description),
        ])
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_document_editor(
    frame: &mut Frame<'_>,
    area: Rect,
    outer: Rect,
    session: &mut AuthoringSession<'_>,
) -> CursorStatus {
    session.composer.editor.remove_block();

    // Render against a stable 80-column editing surface, then copy its
    // cursor-following portion directly into the unpadded visible area.
    let virtual_area = Rect::new(0, 0, MAX_EDITOR_INNER_WIDTH, area.height);
    let mut virtual_buffer = Buffer::empty(virtual_area);
    Widget::render(&session.composer.editor, virtual_area, &mut virtual_buffer);
    if subject_context_fits(&session.composer.editor, area.height) {
        // The first render updates tui-textarea's private viewport dimensions.
        // Resetting after that render cannot move the cursor because the complete
        // subject prefix is known to fit in the current viewport.
        session.composer.editor.scroll((-i16::MAX, 0));
        virtual_buffer = Buffer::empty(virtual_area);
        Widget::render(&session.composer.editor, virtual_area, &mut virtual_buffer);
    }

    let cursor_position = session.composer.editor.rendered_cursor_position();
    let column = cursor_position
        .map_or(1, |position| position.x + 1)
        .clamp(1, MAX_EDITOR_INNER_WIDTH);
    let viewport_width = area.width.clamp(1, MAX_EDITOR_INNER_WIDTH);
    let horizontal_offset = column.saturating_sub(viewport_width);
    for row in 0..area.height.min(virtual_area.height) {
        for viewport_column in 0..viewport_width {
            let source = (horizontal_offset + viewport_column, row);
            let destination = (area.x + viewport_column, area.y + row);
            let mut cell = virtual_buffer[source].clone();
            let remaining_width = viewport_width.saturating_sub(viewport_column);
            if u16::try_from(Line::from(cell.symbol()).width()).unwrap_or(u16::MAX)
                > remaining_width
            {
                cell.set_symbol(" ");
            }
            frame.buffer_mut()[destination] = cell;
        }
    }
    render_document_rules(
        frame,
        area,
        outer,
        &virtual_buffer,
        horizontal_offset,
        session.definition(),
    );
    if let Some(position) = cursor_position
        && let Some(viewport_column) = position.x.checked_sub(horizontal_offset)
        && viewport_column < viewport_width
        && position.y < area.height
    {
        let destination = (area.x + viewport_column, area.y + position.y);
        let cursor_cell = &mut frame.buffer_mut()[destination];
        if destination.0 == 0 && cursor_cell.symbol().trim().is_empty() {
            let style = cursor_cell.style().remove_modifier(Modifier::REVERSED);
            cursor_cell
                .set_symbol(TERMINAL_EDGE_CURSOR)
                .set_style(style);
        } else {
            let cursor_style = cursor_cell
                .style()
                .patch(session.composer.editor.cursor_style());
            cursor_cell.set_style(cursor_style);
        }
    }

    CursorStatus {
        column,
        wrap_width: MAX_EDITOR_INNER_WIDTH,
        content_rows: meaningful_editor_rows(&session.composer.editor),
        viewport_top: absolute_viewport_top(
            &session.composer.editor,
            cursor_position.map_or(0, |position| position.y),
        ),
        viewport_height: area.height,
    }
}

fn absolute_viewport_top(editor: &TextArea<'_>, visible_cursor_row: u16) -> u16 {
    let mut top_reset = editor.clone();
    let content_rows = top_reset
        .measure(MAX_EDITOR_INNER_WIDTH)
        .content_rows
        .max(1);
    let cursor = top_reset.cursor();
    top_reset.scroll((-i16::MAX, 0));
    top_reset.move_cursor(CursorMove::Jump(
        u16::try_from(cursor.0).unwrap_or(u16::MAX),
        u16::try_from(cursor.1).unwrap_or(u16::MAX),
    ));
    let area = Rect::new(0, 0, MAX_EDITOR_INNER_WIDTH, content_rows);
    let mut buffer = Buffer::empty(area);
    Widget::render(&top_reset, area, &mut buffer);
    top_reset
        .rendered_cursor_position()
        .map_or(0, |position| position.y.saturating_sub(visible_cursor_row))
}

fn render_editor_scrollbar(frame: &mut Frame<'_>, area: Rect, status: CursorStatus) {
    if area.is_empty() {
        return;
    }
    let track_style = Style::default().fg(Color::DarkGray);
    for y in area.y..area.bottom() {
        set_rule_cell(frame, area.x, y, "│", track_style);
        if area.width > 1 {
            set_rule_cell(
                frame,
                area.x.saturating_add(1),
                y,
                " ",
                Style::default().bg(JET_BLACK),
            );
        }
    }
    if status.content_rows <= status.viewport_height {
        for y in area.y..area.bottom() {
            render_scrollbar_thumb_row(frame, area, y);
        }
        return;
    }
    let track_length = area.height;
    let thumb_length = u16::try_from(
        u32::from(status.viewport_height)
            .saturating_mul(u32::from(track_length))
            .div_ceil(u32::from(status.content_rows)),
    )
    .unwrap_or(track_length)
    .clamp(1, track_length);
    let maximum_position = status.content_rows.saturating_sub(status.viewport_height);
    let position = status.viewport_top.min(maximum_position);
    let maximum_thumb_offset = track_length.saturating_sub(thumb_length);
    let thumb_offset = if maximum_position == 0 {
        0
    } else {
        u16::try_from(
            (u32::from(position)
                .saturating_mul(u32::from(maximum_thumb_offset))
                .saturating_add(u32::from(maximum_position) / 2))
                / u32::from(maximum_position),
        )
        .unwrap_or(maximum_thumb_offset)
        .min(maximum_thumb_offset)
    };
    for row in thumb_offset..thumb_offset.saturating_add(thumb_length) {
        render_scrollbar_thumb_row(frame, area, area.y.saturating_add(row));
    }
}

fn render_scrollbar_thumb_row(frame: &mut Frame<'_>, area: Rect, y: u16) {
    set_rule_cell(frame, area.x, y, "┃", Style::default().fg(Color::Yellow));
    if area.width > 1 {
        set_rule_cell(
            frame,
            area.x.saturating_add(1),
            y,
            "█",
            Style::default().fg(Color::Gray),
        );
    }
}

fn meaningful_editor_rows(editor: &TextArea<'_>) -> u16 {
    let mut lines = editor.lines().to_vec();
    if lines.len() > 1 {
        lines.pop();
    }
    let mut meaningful = TextArea::new(lines);
    meaningful.set_wrap_mode(editor.wrap_mode());
    meaningful
        .measure(MAX_EDITOR_INNER_WIDTH)
        .content_rows
        .max(1)
}

fn render_validation_row(
    frame: &mut Frame<'_>,
    area: Rect,
    issue: Option<&super::state::ValidationIssue>,
    cursor: CursorStatus,
) {
    let status = format!("col {}/{}", cursor.column, cursor.wrap_width);
    let status_width = u16::try_from(Line::from(status.as_str()).width()).unwrap_or(u16::MAX);
    let columns =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(status_width)]).split(area);
    if let Some(issue) = issue {
        frame.render_widget(
            Paragraph::new(issue.message.as_str()).style(Style::default().fg(Color::Red)),
            columns[0],
        );
    }
    frame.render_widget(Paragraph::new(status).right_aligned(), columns[1]);
}

fn render_document_rules(
    frame: &mut Frame<'_>,
    area: Rect,
    outer: Rect,
    virtual_buffer: &Buffer,
    horizontal_offset: u16,
    definition: &CommitTypeDefinition,
) {
    let field_headings = ["scope:".to_owned(), "description:".to_owned()]
        .into_iter()
        .chain(
            definition
                .properties()
                .iter()
                .map(|property| format!("{}:", property.key())),
        )
        .chain(std::iter::once("breaking-change:".to_owned()))
        .collect::<Vec<_>>();
    for row in 0..area.height {
        if horizontal_offset == 0
            && let Some(heading) = field_headings
                .iter()
                .find(|heading| bold_heading_at(virtual_buffer, row, heading))
        {
            let visible_heading = heading.trim_end_matches(':');
            let heading_width =
                u16::try_from(Line::from(visible_heading).width()).unwrap_or(u16::MAX);
            let colon = area.x.saturating_add(heading_width);
            let colon_style = frame.buffer_mut()[(colon, area.y + row)].style();
            frame.buffer_mut()[(colon, area.y + row)]
                .set_symbol(" ")
                .set_style(colon_style);
            let start = colon.saturating_add(1);
            render_inner_rule(frame, start, area.right(), area.y + row);
        }
        if row > 0
            && ["Message Body", "Message Footer"]
                .iter()
                .any(|heading| bold_heading_at(virtual_buffer, row, heading))
        {
            render_frame_rule(frame, outer, area.y + row - 1, None);
        }
    }
}

fn bold_heading_at(buffer: &Buffer, row: u16, heading: &str) -> bool {
    heading.chars().enumerate().all(|(column, character)| {
        u16::try_from(column).ok().is_some_and(|column| {
            let cell = &buffer[(column, row)];
            cell.symbol() == character.to_string() && cell.modifier.contains(Modifier::BOLD)
        })
    })
}

fn render_inner_rule(frame: &mut Frame<'_>, start: u16, end: u16, y: u16) {
    let style = frame_style();
    for x in start..end {
        set_rule_cell(frame, x, y, "⠒", style);
    }
}

fn render_frame_rule(
    frame: &mut Frame<'_>,
    outer: Rect,
    y: u16,
    intersection: Option<(u16, &'static str)>,
) {
    let style = frame_style();
    merge_rule_cell(frame, outer.x, y, "├", style);
    for x in outer.x.saturating_add(1)..outer.right().saturating_sub(1) {
        merge_rule_cell(frame, x, y, "─", style);
    }
    if let Some((x, symbol)) = intersection {
        merge_rule_cell(frame, x, y, symbol, style);
    }
    merge_rule_cell(frame, outer.right().saturating_sub(1), y, "┤", style);
}

fn merge_rule_cell(frame: &mut Frame<'_>, x: u16, y: u16, incoming: &'static str, style: Style) {
    const LEFT: u8 = 1;
    const RIGHT: u8 = 2;
    const UP: u8 = 4;
    const DOWN: u8 = 8;

    fn connections(symbol: &str) -> u8 {
        match symbol {
            "─" => LEFT | RIGHT,
            "│" => UP | DOWN,
            "┌" => RIGHT | DOWN,
            "┐" => LEFT | DOWN,
            "└" => RIGHT | UP,
            "┘" => LEFT | UP,
            "├" => RIGHT | UP | DOWN,
            "┤" => LEFT | UP | DOWN,
            "┬" => LEFT | RIGHT | DOWN,
            "┴" => LEFT | RIGHT | UP,
            "┼" => LEFT | RIGHT | UP | DOWN,
            _ => 0,
        }
    }

    fn rule_symbol(connections: u8, fallback: &'static str) -> &'static str {
        match connections {
            value if value == LEFT | RIGHT => "─",
            value if value == UP | DOWN => "│",
            value if value == RIGHT | DOWN => "┌",
            value if value == LEFT | DOWN => "┐",
            value if value == RIGHT | UP => "└",
            value if value == LEFT | UP => "┘",
            value if value == RIGHT | UP | DOWN => "├",
            value if value == LEFT | UP | DOWN => "┤",
            value if value == LEFT | RIGHT | DOWN => "┬",
            value if value == LEFT | RIGHT | UP => "┴",
            value if value == LEFT | RIGHT | UP | DOWN => "┼",
            _ => fallback,
        }
    }

    let existing = frame.buffer_mut()[(x, y)].symbol().to_owned();
    let merged = connections(&existing) | connections(incoming);
    frame.buffer_mut()[(x, y)]
        .set_symbol(rule_symbol(merged, incoming))
        .set_style(style);
}

fn set_rule_cell(frame: &mut Frame<'_>, x: u16, y: u16, symbol: &'static str, style: Style) {
    frame.buffer_mut()[(x, y)]
        .set_symbol(symbol)
        .set_style(style);
}

fn subject_context_fits(editor: &TextArea<'_>, viewport_height: u16) -> bool {
    if editor.cursor().0 != SCOPE_VALUE_LINE {
        return false;
    }
    let Some(lines) = editor.lines().get(..=SCOPE_VALUE_LINE) else {
        return false;
    };
    let mut subject = TextArea::new(lines.to_vec());
    subject.set_wrap_mode(editor.wrap_mode());
    subject.measure(MAX_EDITOR_INNER_WIDTH).content_rows <= viewport_height
}

fn render_review(frame: &mut Frame<'_>, area: Rect, session: &mut AuthoringSession<'_>) {
    let sections = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);
    render_stage_header(frame, sections[0], "Review and commit", 3);
    let outer = inset_horizontally(sections[2], 1);
    frame.render_widget(Block::bordered().border_style(frame_style()), outer);
    let content = Rect::new(
        outer.x.saturating_add(2),
        outer.y.saturating_add(2),
        outer.width.saturating_sub(4),
        outer.height.saturating_sub(4),
    );
    let review_sections = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .split(content);
    render_section_heading(frame, review_sections[0], "Commit message");
    if let Some(review) = &mut session.review {
        let mut measured_message =
            TextArea::new(review.message.as_str().lines().map(str::to_owned).collect());
        measured_message.set_wrap_mode(WrapMode::WordOrGlyph);
        review.scrollable = measured_message
            .measure(review_sections[2].width)
            .content_rows
            > review_sections[2].height;
        if !review.scrollable {
            review.scroll = 0;
        }
        frame.render_widget(
            Paragraph::new(review.message.as_str())
                .wrap(Wrap { trim: false })
                .scroll((review.scroll, 0)),
            review_sections[2],
        );
    }
    render_navigation_row(
        frame,
        sections[4],
        &[
            ("enter", "commit"),
            ("esc", "edit"),
            ("↑/↓", "scroll"),
            ("q/ctrl+c", "cancel"),
        ],
    );
}

fn render_confirmation(frame: &mut Frame<'_>, area: Rect, session: &mut AuthoringSession<'_>) {
    let message = match session.confirmation {
        ConfirmationAction::Cancel => "Discard this draft and cancel the commit?",
        ConfirmationAction::ChangeType => "Discard this draft and choose another type?",
    };
    session.confirmation_buttons = Some(render_discard_confirmation(frame, area, message));
}

fn field_columns(id: FieldId, definition: &CommitTypeDefinition) -> (String, &'static str) {
    match id {
        FieldId::Scope => ("scope".to_owned(), "optional"),
        FieldId::Description => ("description".to_owned(), "required"),
        FieldId::Property(index) => {
            let property = &definition.properties()[index];
            let requirement = match property.requirement() {
                PropertyRequirement::Required => "required",
                PropertyRequirement::Recommended => "recommended",
                PropertyRequirement::Optional => "optional",
                PropertyRequirement::Conditional(_) => "conditional",
            };
            (property.key().to_string(), requirement)
        }
        FieldId::BreakingChange => ("breaking-change".to_owned(), "optional"),
    }
}

fn field_hud_name_width(definition: &CommitTypeDefinition) -> u16 {
    std::iter::once(FieldId::Scope)
        .chain(std::iter::once(FieldId::Description))
        .chain((0..definition.properties().len()).map(FieldId::Property))
        .chain(std::iter::once(FieldId::BreakingChange))
        .map(|id| field_columns(id, definition).0)
        .map(|name| u16::try_from(Line::from(name).width()).unwrap_or(u16::MAX))
        .max()
        .unwrap_or(1)
}

fn field_hud_content_width(definition: &CommitTypeDefinition) -> u16 {
    FIELD_MARKER_WIDTH
        .saturating_add(field_hud_name_width(definition))
        .saturating_add(FIELD_REQUIREMENT_WIDTH)
        .saturating_add(FIELD_COLUMN_SPACING.saturating_mul(2))
}

fn composer_minimum_width(definition: &CommitTypeDefinition) -> u16 {
    MINIMUM_WIDTH
        .max(field_hud_content_width(definition).saturating_add(COMPOSER_FRAME_WIDTH_OVERHEAD))
}

fn field_metadata(kind: FieldKind, definition: &CommitTypeDefinition) -> (String, String) {
    match kind {
        FieldKind::Scope => (
            " Scope · optional ".to_owned(),
            "Optional affected area in a Conventional Commit: type(scope): description. Leave blank for type: description."
                .to_owned(),
        ),
        FieldKind::Description => (
            " Description · required ".to_owned(),
            "Required concise description in a Conventional Commit: type(scope): description, or type: description without a scope."
                .to_owned(),
        ),
        FieldKind::Property {
            definition_index,
            value_index: _,
        } => {
            let property = &definition.properties()[definition_index];
            let requirement = match property.requirement() {
                PropertyRequirement::Required => "required".to_owned(),
                PropertyRequirement::Recommended => "recommended".to_owned(),
                PropertyRequirement::Optional => "optional".to_owned(),
                PropertyRequirement::Conditional(condition) => {
                    format!("conditional: {}", condition.rationale())
                }
            };
            (
                format!(" {} · {requirement} ", property.key()),
                property.description().to_owned(),
            )
        }
        FieldKind::BreakingChange => (
            " Breaking change · optional ".to_owned(),
            "Describe an incompatible change. Review adds ! before the header colon and renders this value as an uppercase BREAKING CHANGE footer."
                .to_owned(),
        ),
    }
}

fn context_content_height(description_width: u16, session: &AuthoringSession<'_>) -> u16 {
    let hud_height =
        u16::try_from(session.composer.hud_fields(session.definition()).len()).unwrap_or(u16::MAX);
    let definition = session.definition().clone();
    let (title, description) = session.composer.current_field(&definition).map_or_else(
        || {
            (
                " Commit form ".to_owned(),
                "Edit values beneath the schema-generated field headers.".to_owned(),
            )
        },
        |kind| field_metadata(kind, &definition),
    );
    let guidance_height = wrapped_line_count(title.trim(), description_width)
        .saturating_add(wrapped_line_count(&description, description_width));
    hud_height.max(guidance_height)
}

fn wrapped_line_count(text: &str, width: u16) -> u16 {
    let width = usize::from(width.max(1));
    let mut rows = 1_u16;
    let mut used = 0_usize;
    for word in text.split_whitespace() {
        let word_width = Line::from(word).width();
        if used > 0 && used.saturating_add(1).saturating_add(word_width) <= width {
            used += 1 + word_width;
            continue;
        }
        if used > 0 {
            rows = rows.saturating_add(1);
        }
        let word_rows = word_width.max(1).div_ceil(width);
        rows = rows.saturating_add(u16::try_from(word_rows.saturating_sub(1)).unwrap_or(u16::MAX));
        used = word_width % width;
        if used == 0 && word_width > 0 {
            used = width;
        }
    }
    rows
}

fn section_heading_style() -> Style {
    Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}

fn frame_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn render_section_heading(frame: &mut Frame<'_>, area: Rect, title: &'static str) {
    frame.render_widget(Paragraph::new(title).style(section_heading_style()), area);
}

fn render_centered_notice(
    frame: &mut Frame<'_>,
    area: Rect,
    heading: &'static str,
    message: &'static str,
    controls: Option<&'static str>,
) {
    let lines = if let Some(controls) = controls {
        vec![
            Line::styled(heading, section_heading_style()),
            Line::default(),
            Line::from(message),
            Line::default(),
            Line::from(controls),
        ]
    } else {
        vec![
            Line::styled(heading, section_heading_style()),
            Line::default(),
            Line::from(message),
        ]
    };
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let popup = centered_rect(54, height, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Block::default().style(Style::default().bg(JET_BLACK)),
        popup,
    );
    frame.render_widget(Paragraph::new(lines).centered(), popup);
}

fn render_discard_confirmation(
    frame: &mut Frame<'_>,
    area: Rect,
    message: &'static str,
) -> ConfirmationButtons {
    const DISCARD: &str = "y: discard";
    const KEEP_EDITING: &str = "enter/esc/n: keep editing";

    let popup = centered_rect(58, 9, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Block::bordered()
            .border_type(BorderType::Double)
            .border_style(frame_style())
            .style(Style::default().bg(JET_BLACK)),
        popup,
    );
    let content = Rect::new(
        popup.x.saturating_add(2),
        popup.y.saturating_add(2),
        popup.width.saturating_sub(4),
        popup.height.saturating_sub(4),
    );
    frame.render_widget(
        Paragraph::new("Discard Message")
            .centered()
            .style(section_heading_style()),
        Rect::new(content.x, content.y, content.width, 1),
    );
    frame.render_widget(
        Paragraph::new(message).centered(),
        Rect::new(content.x, content.y.saturating_add(2), content.width, 1),
    );

    let discard_width = text_button_width(DISCARD);
    let keep_width = text_button_width(KEEP_EDITING);
    let button_row = Rect::new(content.x, content.y.saturating_add(4), content.width, 1);
    let buttons = Layout::horizontal([
        Constraint::Length(discard_width),
        Constraint::Length(1),
        Constraint::Length(keep_width),
    ])
    .flex(Flex::Center)
    .split(button_row);
    let discard = buttons[0];
    let keep_editing = buttons[2];
    frame.render_widget(
        Paragraph::new(format!(" {DISCARD} ")).style(navigation_style()),
        discard,
    );
    frame.render_widget(
        Paragraph::new(format!(" {KEEP_EDITING} ")).style(navigation_style()),
        keep_editing,
    );
    ConfirmationButtons {
        discard,
        keep_editing,
    }
}

fn text_button_width(label: &str) -> u16 {
    u16::try_from(Line::from(label).width())
        .unwrap_or(u16::MAX)
        .saturating_add(2)
}

fn render_stage_header(frame: &mut Frame<'_>, area: Rect, title: &'static str, step: u8) {
    let columns = Layout::horizontal([Constraint::Min(0), Constraint::Length(8)]).split(area);
    frame.render_widget(
        Paragraph::new(title).style(Style::default().add_modifier(Modifier::BOLD)),
        columns[0],
    );
    frame.render_widget(
        Paragraph::new(format!("Step {step}/3"))
            .right_aligned()
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        columns[1],
    );
}

fn navigation_style() -> Style {
    Style::default().fg(JET_BLACK).bg(Color::Yellow)
}

fn navigation_key_style() -> Style {
    navigation_style().add_modifier(Modifier::BOLD)
}

fn navigation_line<'a>(hints: &'a [(&'a str, &'a str)]) -> Line<'a> {
    let mut spans = Vec::with_capacity(hints.len().saturating_mul(3));
    for (index, (key, action)) in hints.iter().copied().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" | "));
        }
        spans.push(Span::styled(key, navigation_key_style()));
        spans.push(Span::raw(": "));
        spans.push(Span::raw(action));
    }
    Line::from(spans)
}

fn render_navigation_row(frame: &mut Frame<'_>, area: Rect, hints: &[(&str, &str)]) {
    let line = navigation_line(hints);
    frame.render_widget(Paragraph::new(line).style(navigation_style()), area);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}

const fn inset_horizontally(area: Rect, amount: u16) -> Rect {
    Rect::new(
        area.x.saturating_add(amount),
        area.y,
        area.width.saturating_sub(amount.saturating_mul(2)),
        area.height,
    )
}
