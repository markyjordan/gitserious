mod support;

use std::error::Error;
use std::fs;

use support::{Home, assert_project_shape, git, repository, stderr, stdout};

#[test]
fn nested_first_run_creates_exact_version_control_artifacts() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;
    let nested = repository.path().join("one/two");
    fs::create_dir_all(&nested)?;

    let output = home.run(&nested, &["init"])?;

    let git_root =
        String::from_utf8(git(repository.path(), &["rev-parse", "--show-toplevel"])?.stdout)?;
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        format!(
            "Initialized gitserious in {} (conventional-v1).\n",
            git_root.trim()
        )
    );
    assert!(stderr(&output).is_empty());
    let config = fs::read_to_string(repository.path().join("gitserious.toml"))?;
    assert!(config.starts_with("config-version = 1\ndefault-taxonomy = \"conventional\"\n"));
    assert_eq!(config.matches("\n[[taxonomies]]\n").count(), 3);
    assert_project_shape(repository.path(), &["one"])?;
    assert_eq!(
        fs::read_to_string(repository.path().join(".gitserious/.gitignore"))?,
        "*\n"
    );
    fs::create_dir(repository.path().join(".gitserious/cache"))?;
    fs::write(
        repository.path().join(".gitserious/cache/state"),
        "ignored\n",
    )?;
    git(
        repository.path(),
        &["check-ignore", "-q", ".gitserious/cache/state"],
    )?;
    assert!(!home.library_path().exists());
    Ok(())
}

#[test]
fn linked_worktree_initializes_only_its_own_root() -> Result<(), Box<dyn Error>> {
    let primary = repository()?;
    let home = Home::new()?;
    git(
        primary.path(),
        &["commit", "--allow-empty", "-q", "-m", "initial"],
    )?;
    let worktrees = tempfile::tempdir()?;
    let linked = worktrees.path().join("linked");
    git(
        primary.path(),
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            linked.to_string_lossy().as_ref(),
        ],
    )?;
    let nested = linked.join("one/two");
    fs::create_dir_all(&nested)?;

    let output = home.run(&nested, &["init"])?;

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(linked.join("gitserious.toml").is_file());
    assert!(linked.join(".gitserious/.gitignore").is_file());
    assert!(!primary.path().join("gitserious.toml").exists());
    assert!(!primary.path().join(".gitserious").exists());
    Ok(())
}

#[test]
fn repeat_init_never_rewrites_and_invalid_projects_are_not_regenerated()
-> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;
    let config = repository.path().join("gitserious.toml");
    home.init(repository.path())?;
    assert_project_shape(repository.path(), &[])?;
    let authored = format!("# reviewed\n{}", fs::read_to_string(&config)?);
    fs::write(&config, &authored)?;

    let repeated = home.run(repository.path(), &["init"])?;
    assert!(repeated.status.success());
    assert!(stdout(&repeated).starts_with("gitserious is already initialized in "));
    assert!(stdout(&repeated).ends_with(" (conventional-v1).\n"));
    assert_eq!(fs::read_to_string(&config)?, authored);
    assert_project_shape(repository.path(), &[])?;

    let broken = authored.replace("[[taxonomies]]", "[[taxonomy]]");
    fs::write(&config, &broken)?;
    let invalid = home.run(repository.path(), &["init"])?;
    assert_eq!(invalid.status.code(), Some(1));
    assert!(
        stderr(&invalid).contains("invalid project configuration"),
        "{}",
        stderr(&invalid)
    );
    assert_eq!(fs::read_to_string(&config)?, broken);
    Ok(())
}

#[test]
fn linked_worktrees_resolve_their_own_project_files() -> Result<(), Box<dyn Error>> {
    let primary = repository()?;
    let home = Home::new()?;
    git(
        primary.path(),
        &["commit", "--allow-empty", "-q", "-m", "initial"],
    )?;
    let worktrees = tempfile::tempdir()?;
    let linked = worktrees.path().join("linked");
    git(
        primary.path(),
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            linked.to_string_lossy().as_ref(),
        ],
    )?;
    home.init(primary.path())?;
    home.init(&linked)?;
    let linked_config = linked.join("gitserious.toml");
    let conventional = fs::read_to_string(&linked_config)?;
    fs::write(
        &linked_config,
        conventional.replace(
            "default-taxonomy = \"conventional\"",
            "default-taxonomy = \"research\"",
        ),
    )?;

    let primary_init = home.run(primary.path(), &["init"])?;
    let linked_init = home.run(&linked, &["init"])?;

    assert!(stdout(&primary_init).ends_with(" (conventional-v1).\n"));
    assert!(stdout(&linked_init).ends_with(" (research-v1).\n"));
    Ok(())
}

#[test]
fn local_state_contents_are_not_read_as_project_policy() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;
    let local = repository.path().join(".gitserious");
    fs::create_dir(&local)?;
    fs::write(local.join("config.toml"), "not project policy\n")?;
    fs::write(local.join("gitserious.lock"), "not project policy\n")?;

    let output = home.run(repository.path(), &["init"])?;

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(repository.path().join("gitserious.toml").is_file());
    assert!(!repository.path().join("gitserious.lock").exists());
    assert_eq!(
        fs::read_to_string(local.join("config.toml"))?,
        "not project policy\n"
    );
    assert_eq!(
        fs::read_to_string(local.join("gitserious.lock"))?,
        "not project policy\n"
    );
    Ok(())
}

#[test]
fn non_repository_fails_without_creating_project_state() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let home = Home::new()?;

    let output = home.run(directory.path(), &["init"])?;

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
    assert!(stderr(&output).contains("not inside a Git worktree"));
    assert!(!directory.path().join(".gitserious").exists());
    assert!(!directory.path().join("gitserious.toml").exists());
    assert!(!directory.path().join("gitserious.lock").exists());
    Ok(())
}

#[test]
fn unavailable_git_fails_before_creating_project_state() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let home = Home::new()?;

    let output = home
        .command(directory.path())
        .arg("init")
        .env("PATH", "")
        .output()?;

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
    assert_eq!(stderr(&output), "error: could not execute git\n");
    assert!(!directory.path().join(".gitserious").exists());
    assert!(!directory.path().join("gitserious.toml").exists());
    assert!(!directory.path().join("gitserious.lock").exists());
    Ok(())
}

// Windows resolves the Library under its own platform paths, not `Home::library_path`.
#[cfg(unix)]
#[test]
fn an_unreadable_library_fails_init_without_writing_project_state() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;
    let library = home.library_path();
    fs::create_dir_all(library.parent().ok_or("library path has no parent")?)?;
    fs::write(&library, "config-version = [\n")?;

    let output = home.run(repository.path(), &["init"])?;

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
    assert_eq!(
        stderr(&output),
        format!(
            "error: invalid global configuration {}: TOML parse error at line 1, column 19\n  \
             |\n1 | config-version = [\n  |                   ^\nunclosed array, expected `]`\n\n",
            library.display()
        )
    );
    let entries: Vec<_> = fs::read_dir(repository.path())?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<_, _>>()?;
    assert_eq!(entries, [".git"]);
    Ok(())
}

#[test]
fn help_version_and_usage_exit_codes_match_the_public_contract() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let home = Home::new()?;
    for arguments in [&["--help"][..], &["--version"][..], &[][..]] {
        let output = home.run(directory.path(), arguments)?;
        assert!(output.status.success(), "{}", stderr(&output));
        assert!(!stdout(&output).is_empty());
        assert!(stderr(&output).is_empty());
    }
    for arguments in [&["unknown"][..], &["init", "extra"][..]] {
        let output = home.run(directory.path(), arguments)?;
        assert_eq!(output.status.code(), Some(2));
        assert!(stdout(&output).is_empty());
        assert!(stderr(&output).contains("Usage:"));
    }
    Ok(())
}
