use std::collections::BTreeSet;

use gitserious_app::{LibraryDraft, NodeId, NodeRef};
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

use crate::theme::{
    JET_BLACK, centered_rect, frame_style, navigation_style, section_heading_style, selected_style,
};

pub(super) const CHOICES: [(&str, &str); 3] =
    [("space", "toggle"), ("enter", "copy"), ("esc", "cancel")];

pub(super) fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    library: &LibraryDraft,
    property: &NodeId,
    cursor: usize,
    chosen: &BTreeSet<NodeId>,
) {
    let Some(NodeRef::Property {
        commit_type,
        property: source,
        ..
    }) = library.node(property)
    else {
        return;
    };
    let targets = library.copy_targets(property);
    let rows = u16::try_from(targets.len()).unwrap_or(u16::MAX);
    let popup = centered_rect(58, rows.saturating_add(10).min(area.height), area);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Block::bordered()
            .border_type(BorderType::Double)
            .border_style(frame_style())
            .style(Style::default().fg(Color::White).bg(JET_BLACK)),
        popup,
    );
    let [title, _, message, _, list, _, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(popup.inner(Margin::new(2, 2)));
    frame.render_widget(
        Paragraph::new("Copy Property")
            .centered()
            .style(section_heading_style()),
        title,
    );
    frame.render_widget(
        Paragraph::new(format!(
            "Copy \"{}\" from {} to:",
            source.name(),
            commit_type.name()
        )),
        message,
    );
    let offset = cursor.saturating_sub(usize::from(list.height).saturating_sub(1));
    let lines = targets
        .iter()
        .enumerate()
        .skip(offset)
        .map(|(index, target)| {
            let mark = if chosen.contains(target.id()) {
                'x'
            } else {
                ' '
            };
            let line = Line::from(format!("[{mark}] {}", target.name()));
            if index == cursor {
                line.style(selected_style(true))
            } else {
                line
            }
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), list);
    let labels = CHOICES.map(|(key, label)| format!(" {key}: {label} "));
    let widths = labels
        .iter()
        .map(|label| u16::try_from(label.len()).unwrap_or(u16::MAX));
    let areas = Layout::horizontal(widths.map(Constraint::Length))
        .flex(Flex::Center)
        .spacing(1)
        .split(buttons);
    for (label, area) in labels.iter().zip(areas.iter()) {
        frame.render_widget(
            Paragraph::new(label.as_str()).style(navigation_style()),
            *area,
        );
    }
}
