//! Filesystem paths and local-storage adapters for gitserious.

mod directory;
mod global;
mod platform;

pub use directory::LocalDirectoryCreator;
pub use global::{GlobalPathError, SystemGlobalPathResolver};

#[cfg(test)]
#[path = "../tests/unit/mod.rs"]
mod tests;
