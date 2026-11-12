use std::process::ExitCode;

/// The page an interactive command asks the TUI to open first.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Destination {
    Home,
    Commit,
    Config,
    /// Help, on the page of the named command, or the overview for none.
    Help {
        command: Option<String>,
    },
}

/// What an argument list asks gitserious to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Invocation {
    /// Open the unified TUI on this destination.
    Open(Destination),
    /// Initialize the project that contains the start directory.
    Init,
    /// Text the CLI writes without running anything.
    Answer(Answer),
}

/// A response the CLI writes in full, with `write_answer`.
#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    /// Help or the version, written to stdout.
    Print(String),
    /// A usage error: clap's message for stderr and the exit code it chose.
    Reject { message: String, code: ExitCode },
}
