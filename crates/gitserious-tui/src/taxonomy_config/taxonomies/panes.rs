use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph};

use super::super::content_padding;
use crate::theme::theme;

/// Terminals this wide put the Inspector beside the outline instead of below it.
const SIDE_BY_SIDE_WIDTH: u16 = 100;
const OUTLINE_MAX_WIDTH: u16 = 64;

/// Where the header, the outline, and the Inspector go, and the dim rule between the outline
/// and the Inspector.
pub(super) struct Panes {
    pub(super) lead: Rect,
    pub(super) outline: Rect,
    pub(super) inspector: Rect,
    rule: Rect,
    rule_side: Borders,
}

impl Panes {
    /// Splits `area` on a `terminal` this size. The outline column starts with `lead` rows for
    /// a header. Under 100 columns the outline takes its `outline_rows` up to 60% of the rows
    /// below the header, rule included, and the Inspector sits below it. From 100 columns the
    /// outline takes half the width, at most 64 columns, and the Inspector sits beside it.
    pub(super) fn new(area: Rect, terminal: Rect, lead: u16, outline_rows: usize) -> Self {
        let pad = content_padding(terminal);
        let padded = |column: Rect| {
            let [_, inside, _] = Layout::vertical([
                Constraint::Length(pad),
                Constraint::Min(0),
                Constraint::Length(pad),
            ])
            .areas(column);
            inside
        };
        if terminal.width >= SIDE_BY_SIDE_WIDTH {
            let [left, rule, right] = Layout::horizontal([
                Constraint::Length((area.width / 2).min(OUTLINE_MAX_WIDTH)),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .areas(area);
            let [lead, outline] = Layout::vertical([Constraint::Length(lead), Constraint::Min(0)])
                .areas(padded(left));
            return Self {
                lead,
                outline,
                inspector: padded(right),
                rule,
                rule_side: Borders::LEFT,
            };
        }
        let [lead, body] =
            Layout::vertical([Constraint::Length(lead), Constraint::Min(0)]).areas(padded(area));
        let share = u16::try_from(u32::from(body.height) * 3 / 5).unwrap_or(u16::MAX);
        let height = share
            .saturating_sub(1)
            .min(u16::try_from(outline_rows).unwrap_or(u16::MAX));
        let [outline, rule, inspector] = Layout::vertical([
            Constraint::Length(height),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .areas(body);
        Self {
            lead,
            outline,
            inspector,
            rule,
            rule_side: Borders::TOP,
        }
    }

    pub(super) fn render_rule(&self, frame: &mut Frame<'_>) {
        frame.render_widget(
            Block::new()
                .borders(self.rule_side)
                .border_style(Style::new().fg(theme().dim)),
            self.rule,
        );
    }
}

/// The rows a pane of `height` shows of `total`: all of them, or one fewer when they overflow,
/// leaving the last row to count the rows below.
pub(super) fn shown(total: usize, height: u16) -> u16 {
    if total > usize::from(height) {
        height.saturating_sub(1)
    } else {
        height
    }
}

/// Counts the `hidden` rows below in the last row of `pane`, right-aligned and dim.
pub(super) fn render_more_below(frame: &mut Frame<'_>, pane: Rect, hidden: usize) {
    if hidden == 0 || pane.height == 0 {
        return;
    }
    let row = Rect::new(pane.x, pane.bottom() - 1, pane.width.saturating_sub(1), 1);
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!("↓ {hidden} more"),
            Style::new().fg(theme().dim),
        ))
        .right_aligned(),
        row,
    );
}
