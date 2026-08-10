//! Ratatui-backed terminal interaction adapters for gitserious.

mod author;

pub use author::{RatatuiCommitDraftAuthor, RatatuiCommitDraftAuthorError};

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod tests;
