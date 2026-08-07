//! Filesystem paths and local-storage adapters for gitserious.

mod directory;
mod global;
mod platform;

pub use directory::{StorageDirectory, ensure_directory};
pub use global::{GlobalPathError, GlobalPaths};
