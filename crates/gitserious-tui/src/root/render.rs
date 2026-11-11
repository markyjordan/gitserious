use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Paragraph};

use super::Shell;
use super::navigation::{Section, Tabs};
use crate::help::HelpPage;
use crate::theme::{
    FOOTER_HEIGHT, MINIMUM_HEIGHT, MINIMUM_WIDTH, SHELL_HINTS, centered_rect, normalize_background,
    render_footer, render_footer_block, theme,
};

impl Shell {
    pub(crate) fn render(&mut self, frame: &mut Frame<'_>, help: &[HelpPage]) {
        let area = frame.area();
        self.too_small = area.width < MINIMUM_WIDTH || area.height < MINIMUM_HEIGHT;
        self.tabs = Tabs::default();
        frame.render_widget(
            Block::default().style(Style::default().fg(theme().text).bg(theme().ground)),
            area,
        );
        if self.too_small {
            let message = if self.section == Section::Config && self.config.modal() {
                "Terminal too small\nResize to continue"
            } else {
                "Terminal too small\nResize or press esc/q to quit"
            };
            frame.render_widget(
                Paragraph::new(message).centered(),
                centered_rect(54, 3, area),
            );
        } else {
            let [header, body] =
                Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
            self.render_header(frame, header);
            if self.section == Section::Config {
                self.config.render(frame, body);
                if self.config.modal() {
                    crate::theme::dim_region(frame, header);
                }
            } else {
                let [content, footer] =
                    Layout::vertical([Constraint::Min(1), Constraint::Length(FOOTER_HEIGHT)])
                        .areas(body);
                let footer = render_footer_block(frame, footer);
                match self.section {
                    Section::Home => self.home.render(frame, content),
                    Section::Commit => self.commit.render(frame, content),
                    Section::Help => self.help.render(frame, content, help),
                    Section::Config => {}
                }
                let commit_hints = self.commit.hints();
                let home_hints = self.home.hints();
                let help_hints = self.help.hints(help.len());
                let hints: &[(&str, &str)] = match self.section {
                    Section::Commit => &commit_hints,
                    Section::Help => &help_hints,
                    Section::Home => &home_hints,
                    Section::Config => &[],
                };
                render_footer(frame, footer, hints, &SHELL_HINTS);
            }
        }
        self.render_overlay(frame, area);
        normalize_background(frame, area);
    }

    fn render_header(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let band = Style::new().fg(theme().dim).bg(theme().band);
        frame.render_widget(Block::new().style(band), area);
        let version = format!("gitserious v{} ", env!("CARGO_PKG_VERSION"));
        frame.render_widget(Paragraph::new(version).style(band).right_aligned(), area);
        self.tabs.render(frame, area, self.section);
    }
}
