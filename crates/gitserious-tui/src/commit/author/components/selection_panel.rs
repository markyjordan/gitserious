//! How a selection panel looks: a band with its text inset from the edges, a
//! selected row highlighted from edge to edge, and tabs. Each picker decides
//! where its panel goes, what its rows say, and what choosing one does.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Block;

use crate::theme::{popup_selected_style, theme};

/// Columns between the panel's edge and its text.
pub(crate) const INSET: u16 = 2;

/// A panel drawn in a parent-owned rectangle.
pub(crate) struct SelectionPanel {
    area: Rect,
}

impl SelectionPanel {
    /// Fills `area` with the band.
    pub(crate) fn draw(frame: &mut Frame<'_>, area: Rect) -> Self {
        frame.render_widget(
            Block::default().style(Style::new().fg(theme().text).bg(theme().band)),
            area,
        );
        Self { area }
    }

    /// The panel inside its inset, where rows put their text.
    pub(crate) const fn content(&self) -> Rect {
        Rect::new(
            self.area.x.saturating_add(INSET),
            self.area.y,
            self.area.width.saturating_sub(INSET * 2),
            self.area.height,
        )
    }

    /// The panel's full width at row `y`, which a click on that row hits.
    pub(crate) const fn row(&self, y: u16) -> Rect {
        Rect::new(self.area.x, y, self.area.width, 1)
    }

    /// Highlights row `y` from edge to edge, and returns the style for its text.
    pub(crate) fn select(&self, frame: &mut Frame<'_>, y: u16) -> Style {
        frame.render_widget(Block::default().style(popup_selected_style()), self.row(y));
        popup_selected_style()
    }
}

/// A tab's label: bold and underlined when active, dim otherwise.
pub(crate) fn tab_style(active: bool) -> Style {
    if active {
        Style::new()
            .fg(theme().text)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::new().fg(theme().dim)
    }
}

/// The first of `rows` visible rows, moved from `offset` only as far as it
/// takes to show `selected`, a position in the displayed rows.
pub(crate) const fn visible_offset(selected: usize, offset: usize, rows: usize) -> usize {
    if rows == 0 {
        return 0;
    }
    if selected < offset {
        selected
    } else if selected >= offset.saturating_add(rows) {
        selected + 1 - rows
    } else {
        offset
    }
}
