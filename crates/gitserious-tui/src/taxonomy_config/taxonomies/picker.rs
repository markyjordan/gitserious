use crate::theme::{
    centered_rect, popup_block, popup_heading_style, popup_hints, popup_selected_style,
};
use std::collections::BTreeSet;

use gitserious_app::{LibraryDraft, NodeId, NodeRef};
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Margin};
use ratatui::text::Line;
use ratatui::widgets::{Clear, Paragraph};

pub(super) const CHOICES: [(&str, &str); 3] =
    [("space", "toggle"), ("enter", "copy"), ("esc", "cancel")];

pub(super) fn render(
    frame: &mut Frame<'_>,
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
    let viewport = frame.area();
    let popup = centered_rect(58, rows.saturating_add(10).min(viewport.height), viewport);
    frame.render_widget(Clear, popup);
    frame.render_widget(popup_block(), popup);
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
            .style(popup_heading_style()),
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
                line.style(popup_selected_style())
            } else {
                line
            }
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), list);
    let labels = CHOICES.map(|(key, label)| format!(" {key} {label} "));
    let widths = labels
        .iter()
        .map(|label| u16::try_from(label.len()).unwrap_or(u16::MAX));
    let areas = Layout::horizontal(widths.map(Constraint::Length))
        .flex(Flex::Center)
        .spacing(1)
        .split(buttons);
    for ((key, label), area) in CHOICES.iter().zip(areas.iter()) {
        frame.render_widget(
            Paragraph::new(popup_hints(&[(key, label)])).centered(),
            *area,
        );
    }
}
