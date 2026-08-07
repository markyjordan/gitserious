use std::io;
use std::process::ExitCode;

use gitserious_fs::{GitRepositoryLocator, TomlProjectStateStore};

fn main() -> ExitCode {
    let locator = GitRepositoryLocator;
    let store = TomlProjectStateStore;
    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut stdout = stdout.lock();
    let mut stderr = stderr.lock();

    gitserious_cli::run(
        std::env::args_os(),
        &locator,
        &store,
        &mut stdout,
        &mut stderr,
    )
}
