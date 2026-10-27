use std::cell::Cell;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;

use gitserious_app::{GlobalPathResolver, GlobalPaths, resolve_global_paths};

#[derive(Clone, Debug, Eq, PartialEq)]
enum FakeError {
    Unavailable,
}

impl Display for FakeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("storage adapter unavailable")
    }
}

impl Error for FakeError {}

struct RecordingResolver {
    result: Result<GlobalPaths, FakeError>,
    calls: Cell<usize>,
}

impl RecordingResolver {
    fn returning(paths: GlobalPaths) -> Self {
        Self {
            result: Ok(paths),
            calls: Cell::new(0),
        }
    }

    fn failing() -> Self {
        Self {
            result: Err(FakeError::Unavailable),
            calls: Cell::new(0),
        }
    }
}

impl GlobalPathResolver for RecordingResolver {
    type Error = FakeError;

    fn resolve(&self) -> Result<GlobalPaths, Self::Error> {
        self.calls.set(self.calls.get() + 1);
        self.result.clone()
    }
}

fn sample_paths() -> GlobalPaths {
    GlobalPaths::new(
        PathBuf::from("config"),
        PathBuf::from("data"),
        PathBuf::from("state"),
        PathBuf::from("cache"),
    )
}

#[test]
fn global_paths_preserve_owned_purpose_specific_directories() {
    let mut source = PathBuf::from("original");
    let paths = GlobalPaths::new(
        source.join("config"),
        source.join("data"),
        source.join("state"),
        source.join("cache"),
    );

    source.push("changed");

    assert_eq!(paths.config().as_path(), PathBuf::from("original/config"));
    assert_eq!(paths.data().as_path(), PathBuf::from("original/data"));
    assert_eq!(paths.state().as_path(), PathBuf::from("original/state"));
    assert_eq!(paths.cache().as_path(), PathBuf::from("original/cache"));
}

#[test]
fn resolve_use_case_returns_the_adapter_snapshot_and_calls_once() -> Result<(), Box<dyn Error>> {
    let expected = sample_paths();
    let resolver = RecordingResolver::returning(expected.clone());

    let actual = resolve_global_paths(&resolver)?;

    assert_eq!(actual, expected);
    assert_eq!(resolver.calls.get(), 1);
    Ok(())
}

#[test]
fn resolve_use_case_preserves_the_adapter_error() {
    let resolver = RecordingResolver::failing();

    assert_eq!(resolve_global_paths(&resolver), Err(FakeError::Unavailable));
    assert_eq!(resolver.calls.get(), 1);
}
