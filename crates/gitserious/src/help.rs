use gitserious_cli::{help_topics, styled_help};
use gitserious_tui::HelpPage;

/// The CLI's Help topics as TUI pages.
pub fn help_pages() -> Vec<HelpPage> {
    help_topics()
        .into_iter()
        .map(|topic| {
            HelpPage::styled(topic.title, topic.command, move |width| {
                styled_help(topic.command, width)
            })
        })
        .collect()
}
