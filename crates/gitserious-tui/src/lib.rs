//! Ratatui-backed terminal interaction adapters for gitserious.

#![allow(clippy::missing_errors_doc)]
#![allow(
    clippy::format_push_string,
    clippy::needless_pass_by_value,
    clippy::ref_option,
    clippy::semicolon_if_nothing_returned,
    clippy::too_many_lines,
    clippy::unnecessary_semicolon,
    clippy::unused_self
)]

mod author;
// Legacy template/typeset configuration TUI retained in src/config/.
// mod config;
mod taxonomy_config;
mod theme;

pub use author::{RatatuiCommitDraftAuthor, RatatuiCommitDraftAuthorError};
pub use taxonomy_config::RatatuiTaxonomyConfigurationEditor;

// Legacy template-oriented TUI tests remain in tests/unit/.
// #[cfg(test)]
// #[path = "../tests/unit/mod.rs"]
// mod tests;
