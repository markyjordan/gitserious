mod support;

use std::error::Error;
use std::fs;
use std::path::Path;

use support::{Home, repository, stderr, stdout};

type Tree = Vec<(String, Vec<u8>)>;

/// Every path under `directory` with its bytes, so a test can prove nothing was written.
fn snapshot(directory: &Path) -> Result<Tree, Box<dyn Error>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.file_name().is_some_and(|name| name == ".git") {
            continue;
        }
        let name = path.to_string_lossy().into_owned();
        if path.is_dir() {
            entries.push((name, Vec::new()));
            entries.extend(snapshot(&path)?);
        } else {
            entries.push((name, fs::read(&path)?));
        }
    }
    entries.sort();
    Ok(entries)
}

fn corrupt_library(home: &Home) -> Result<(), Box<dyn Error>> {
    let library = home.library_path();
    fs::create_dir_all(library.parent().ok_or("library has a parent")?)?;
    fs::write(library, "not = [toml\n")?;
    Ok(())
}

#[test]
fn valid_project_verifies_read_only_without_a_usable_library() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;
    home.init(repository.path())?;
    corrupt_library(&home)?;
    let before = snapshot(repository.path())?;

    let output = home.run(repository.path(), &["config", "verify", "--project"])?;

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "gitserious.toml is valid.\ndefault: conventional\n\
         taxonomies: conventional@2, research@2, infra-ops@2\n"
    );
    assert!(stderr(&output).is_empty());
    assert_eq!(snapshot(repository.path())?, before);
    Ok(())
}

#[test]
fn invalid_or_absent_projects_exit_one_with_their_defect() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;

    let absent = home.run(repository.path(), &["config", "verify", "--project"])?;
    assert_eq!(absent.status.code(), Some(1));
    assert!(
        stderr(&absent).contains("not initialized"),
        "{}",
        stderr(&absent)
    );
    assert!(!repository.path().join("gitserious.toml").exists());

    home.init(repository.path())?;
    let config = repository.path().join("gitserious.toml");
    let valid = fs::read_to_string(&config)?;
    let duplicated = format!(
        "{valid}\n{}",
        &valid[valid.find("[[taxonomies]]").ok_or("has taxonomies")?..]
    );
    for (contents, defect) in [
        (
            valid.replace(
                "default-taxonomy = \"conventional\"",
                "default-taxonomy = \"missing\"",
            ),
            "default taxonomy \"missing\"",
        ),
        (duplicated, "repeat \"conventional\""),
        (
            valid.replace("config-version = 1", "config-version = 9"),
            "unsupported project config version 9",
        ),
        ("not = [toml\n".to_owned(), "TOML parse error"),
    ] {
        fs::write(&config, &contents)?;
        let output = home.run(repository.path(), &["config", "verify", "--project"])?;
        assert_eq!(output.status.code(), Some(1), "{defect}");
        assert!(stdout(&output).is_empty());
        assert!(stderr(&output).contains(defect), "{}", stderr(&output));
        assert_eq!(fs::read_to_string(&config)?, contents);
    }
    Ok(())
}

#[test]
fn usage_and_environment_failures_use_their_own_exit_codes() -> Result<(), Box<dyn Error>> {
    let home = Home::new()?;
    let repository = repository()?;

    let usage = home.run(repository.path(), &["config", "verify"])?;
    assert_eq!(usage.status.code(), Some(2));
    assert!(stderr(&usage).contains("--project"), "{}", stderr(&usage));

    let outside = tempfile::tempdir()?;
    let unlocated = home.run(outside.path(), &["config", "verify", "--project"])?;
    assert_eq!(unlocated.status.code(), Some(3));
    assert!(
        stderr(&unlocated).contains("not inside a Git worktree"),
        "{}",
        stderr(&unlocated)
    );

    fs::create_dir(repository.path().join("gitserious.toml"))?;
    let unreadable = home.run(repository.path(), &["config", "verify", "--project"])?;
    assert_eq!(unreadable.status.code(), Some(3));
    assert!(
        stderr(&unreadable).contains("wrong file type"),
        "{}",
        stderr(&unreadable)
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn verification_needs_no_write_permission() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::PermissionsExt;

    let repository = repository()?;
    let home = Home::new()?;
    home.init(repository.path())?;
    let config = repository.path().join("gitserious.toml");
    fs::set_permissions(&config, fs::Permissions::from_mode(0o444))?;
    fs::set_permissions(repository.path(), fs::Permissions::from_mode(0o555))?;

    let output = home.run(repository.path(), &["config", "verify", "--project"]);

    fs::set_permissions(repository.path(), fs::Permissions::from_mode(0o755))?;
    let output = output?;
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    Ok(())
}
