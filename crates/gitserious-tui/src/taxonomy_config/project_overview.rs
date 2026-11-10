use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{content_padding, library_header};
use crate::theme::{popup_selected_style, theme};
use gitserious_app::ProjectSnapshot;

/// The project identity and its missing configuration, without framed settings panels.
pub(super) fn render_empty(
    frame: &mut Frame<'_>,
    area: Rect,
    project: &ProjectSnapshot,
    home: Option<&std::path::Path>,
) -> Rect {
    let content = area.inner(Margin::new(0, content_padding(frame.area())));
    render_header(frame, content, project, home);
    frame.render_widget(
        Paragraph::new("No gitserious configuration found in this project."),
        Rect::new(
            content.x + 1,
            content.y + 3,
            content.width.saturating_sub(2),
            1,
        ),
    );
    let button = Rect::new(content.x, content.y + 5, content.width, 1).intersection(content);
    let label = " initialize gitserious";
    let key = "enter ";
    let gap = usize::from(button.width).saturating_sub(label.len() + key.len());
    frame.render_widget(
        Paragraph::new(format!("{label}{}{key}", " ".repeat(gap))).style(popup_selected_style()),
        button,
    );
    button
}

pub(super) fn render_header(
    frame: &mut Frame<'_>,
    area: Rect,
    project: &ProjectSnapshot,
    home: Option<&std::path::Path>,
) {
    let root = project.root().as_path();
    let width = usize::from(area.width.saturating_sub(2));
    let (badge, color) = if project.configuration().is_some() {
        ("✓ gitserious.toml", theme().done)
    } else {
        ("✗ gitserious.toml", theme().blocks)
    };
    let name = root
        .file_name()
        .unwrap_or(root.as_os_str())
        .to_string_lossy();
    let name = library_header::keep_end(&name, width.saturating_sub(badge.chars().count() + 1));
    let gap = width.saturating_sub(Span::raw(&name).width() + badge.chars().count());
    let title = Line::from(vec![
        Span::styled(
            name,
            Style::new().fg(theme().text).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(gap)),
        Span::styled(badge, Style::new().fg(color)),
    ]);
    frame.render_widget(
        Paragraph::new(title),
        Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), 1),
    );
    let path = library_header::keep_end(&library_header::home_relative(root, home), width);
    frame.render_widget(
        Paragraph::new(path).style(Style::new().fg(theme().dim)),
        Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), 1),
    );
}
