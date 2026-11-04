mod support;

use std::error::Error;
use std::fs;

use support::{Home, git, repository, stderr, stdout};

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
            "Initialized gitserious in {} (conventional@2).\n",
            git_root.trim()
        )
    );
    assert!(stderr(&output).is_empty());
    let config = fs::read_to_string(repository.path().join("gitserious.toml"))?;
    assert!(config.starts_with("config-version = 1\ndefault-taxonomy = \"conventional\"\n"));
    assert_eq!(config.matches("\n[[taxonomies]]\n").count(), 3);
    assert!(!repository.path().join("gitserious.lock").exists());
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
    let authored = format!("# reviewed\n{}", fs::read_to_string(&config)?);
    fs::write(&config, &authored)?;

    let repeated = home.run(repository.path(), &["init", "--taxonomy", "research"])?;
    assert!(repeated.status.success());
    assert!(stdout(&repeated).starts_with("gitserious is already initialized in "));
    assert!(stdout(&repeated).ends_with(" (conventional@2).\n"));
    assert_eq!(fs::read_to_string(&config)?, authored);

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
    let research = home.run(&linked, &["init", "--taxonomy", "research"])?;
    assert!(research.status.success(), "{}", stderr(&research));

    let primary_commit = home.run(primary.path(), &["commit", "--type", "symptom"])?;
    let linked_commit = home.run(&linked, &["commit", "--type", "feat"])?;

    assert!(stderr(&primary_commit).contains("commit type \"symptom\" is unavailable"));
    assert!(stderr(&linked_commit).contains("commit type \"feat\" is unavailable"));
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
