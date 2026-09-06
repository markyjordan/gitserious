//! Ratatui-backed terminal interaction adapters for gitserious.

mod author;
mod config;

pub use author::{RatatuiCommitDraftAuthor, RatatuiCommitDraftAuthorError};
pub use config::RatatuiConfigurationEditor;

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod tests;
