use std::collections::BTreeSet;

use gitserious_app::RepositoryStatus;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, List, ListItem};

use super::{StatusTree, group_rows};
use crate::theme::{block_title, frame_style, navigation_key_style, section_heading_style};

impl StatusTree {
    /// Draws the rows in a bordered list in `tree`, and keeps the list's inner
    /// area for the next click.
    pub(in crate::commit) fn render(&mut self, frame: &mut Frame<'_>, tree: Rect) {
        let block = Block::bordered()
            .border_style(frame_style())
            .title(block_title("Changes"));
        self.list_area = block.inner(tree);
        let items: Vec<_> = self
            .rows
            .iter()
            .map(|row| {
                let item = ListItem::new(row.label.clone());
                if row.directory.is_none() && row.detail.is_empty() {
                    item.style(section_heading_style())
                } else {
                    item
                }
            })
            .collect();
        frame.render_stateful_widget(
            List::new(items)
                .block(block)
                .highlight_style(navigation_key_style()),
            tree,
            &mut self.list,
        );
    }
}

/// The staged group with every directory open, for the staged popup.
pub(in crate::commit) fn staged_lines(snapshot: &RepositoryStatus) -> Vec<Line<'static>> {
    group_rows(snapshot, 0, &BTreeSet::new())
        .into_iter()
        .map(|row| {
            if row.directory.is_none() && row.detail.is_empty() {
                Line::styled(row.label, section_heading_style())
            } else {
                Line::from(row.label)
            }
        })
        .collect()
}
