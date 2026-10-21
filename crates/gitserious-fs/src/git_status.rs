use std::path::PathBuf;
use std::process::Command;

use gitserious_app::{
    BranchState, RepositoryRoot, RepositoryStatus, RepositoryStatusReader, StatusEntry,
};

/// Reads machine-stable, NUL-delimited status without optional index refresh locks.
#[derive(Clone, Copy, Debug, Default)]
pub struct GitRepositoryStatusReader;

impl RepositoryStatusReader for GitRepositoryStatusReader {
    fn read(&self, root: &RepositoryRoot) -> Result<RepositoryStatus, String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(root.as_path())
            .args([
                "status",
                "--porcelain=v2",
                "-z",
                "--branch",
                "--untracked-files=all",
            ])
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .map_err(|error| format!("could not execute git status: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
        }
        parse_status(root.clone(), &output.stdout)
    }
}

// Unix paths are infallible; the same interface reports decoding errors on Windows.
#[cfg_attr(unix, allow(clippy::unnecessary_wraps))]
fn path(bytes: &[u8]) -> Result<PathBuf, String> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(PathBuf::from(std::ffi::OsString::from_vec(bytes.to_vec())))
    }
    #[cfg(not(unix))]
    {
        String::from_utf8(bytes.to_vec())
            .map(PathBuf::from)
            .map_err(|error| format!("invalid path encoding: {error}"))
    }
}

fn parse_status(root: RepositoryRoot, bytes: &[u8]) -> Result<RepositoryStatus, String> {
    let mut records = bytes
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty());
    let mut head = None;
    let mut oid = None;
    let mut entries = Vec::new();
    while let Some(record) = records.next() {
        if let Some(value) = record.strip_prefix(b"# branch.head ") {
            head = Some(String::from_utf8_lossy(value).into_owned());
        } else if let Some(value) = record.strip_prefix(b"# branch.oid ") {
            oid = Some(String::from_utf8_lossy(value).into_owned());
        } else if record.starts_with(b"# ") {
            // Upstream/ahead-behind headers are optional metadata.
        } else if let Some(value) = record.strip_prefix(b"? ") {
            entries.push(StatusEntry {
                path: path(value)?,
                original_path: None,
                index: '.',
                worktree: '.',
                untracked: true,
                conflicted: false,
                submodule: "N...".into(),
            });
        } else {
            let kind = record[0];
            let fields = match kind {
                b'1' => 9,
                b'2' => 10,
                b'u' => 11,
                _ => return Err("unknown Git status record".into()),
            };
            let values: Vec<_> = record.splitn(fields, |byte| *byte == b' ').collect();
            if values.len() != fields || values[1].len() != 2 {
                return Err("malformed Git status record".into());
            }
            let original_path = if kind == b'2' {
                Some(path(
                    records
                        .next()
                        .ok_or("Git rename record is missing its original path")?,
                )?)
            } else {
                None
            };
            entries.push(StatusEntry {
                path: path(values[fields - 1])?,
                original_path,
                index: char::from(values[1][0]),
                worktree: char::from(values[1][1]),
                untracked: false,
                conflicted: kind == b'u',
                submodule: String::from_utf8_lossy(values[2]).into_owned(),
            });
        }
    }
    let head = head.ok_or("Git status is missing branch identity")?;
    let oid = oid.ok_or("Git status is missing branch object")?;
    let branch = if head == "(detached)" {
        BranchState::Detached(oid)
    } else if oid == "(initial)" {
        BranchState::Unborn(head)
    } else {
        BranchState::Named(head)
    };
    Ok(RepositoryStatus {
        root,
        branch,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_partial_staging_rename_conflict_and_literal_paths() -> Result<(), String> {
        let root = RepositoryRoot::new(std::env::temp_dir()).map_err(|error| error.to_string())?;
        let bytes = b"# branch.oid abc\0# branch.head main\0\
1 MM N... 100644 100644 100644 abc def dir/has spaces\0\
2 R. N... 100644 100644 100644 abc def R100 new\nname\0old name\0\
u UU N... 100644 100644 100644 100644 a b c conflict\0\
? untracked\nfile\0";
        let status = parse_status(root, bytes)?;
        assert_eq!(status.branch, BranchState::Named("main".into()));
        assert_eq!(status.entries[0].index, 'M');
        assert_eq!(status.entries[0].worktree, 'M');
        assert_eq!(status.entries[1].path, PathBuf::from("new\nname"));
        assert_eq!(
            status.entries[1].original_path,
            Some(PathBuf::from("old name"))
        );
        assert!(status.entries[2].conflicted);
        assert!(status.entries[3].untracked);
        assert!(status.has_staged_changes());
        assert!(status.has_conflicts());
        Ok(())
    }

    #[test]
    fn parses_unborn_detached_and_rejects_incomplete_renames() -> Result<(), String> {
        let root = RepositoryRoot::new(std::env::temp_dir()).map_err(|error| error.to_string())?;
        assert!(matches!(
            parse_status(
                root.clone(),
                b"# branch.oid (initial)\0# branch.head main\0"
            )?
            .branch,
            BranchState::Unborn(_)
        ));
        assert!(matches!(
            parse_status(
                root.clone(),
                b"# branch.oid abc\0# branch.head (detached)\0"
            )?
            .branch,
            BranchState::Detached(_)
        ));
        assert!(parse_status(root, b"2 R. N... 1 1 1 a b R100 new\0").is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn preserves_non_utf8_paths() -> Result<(), String> {
        use std::os::unix::ffi::OsStrExt;
        let root = RepositoryRoot::new(std::env::temp_dir()).map_err(|error| error.to_string())?;
        let status = parse_status(root, b"# branch.oid abc\0# branch.head main\0? raw-\xff\0")?;
        assert_eq!(status.entries[0].path.as_os_str().as_bytes(), b"raw-\xff");
        Ok(())
    }
}
