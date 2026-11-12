//! Command-line delivery and presentation for gitserious.

#![allow(clippy::missing_errors_doc)]

mod cli;
mod commands;
mod invocation;
mod output;

pub use cli::{HelpTopic, help_topics, parse, styled_help};
pub use invocation::{Answer, Destination, Invocation};
pub use output::{write_answer, write_error, write_init_outcome};
