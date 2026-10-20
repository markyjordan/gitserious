#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_gitserious")
}

fn repository() -> Result<TempDir, Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    git(directory.path(), &["init", "-q"])?;
    Ok(directory)
}

fn isolation() -> Result<TempDir, Box<dyn Error>> {
    Ok(tempfile::tempdir()?)
}

fn run(directory: &Path, home: &Path, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
    Ok(Command::new(binary())
        .args(arguments)
        .current_dir(directory)
        .env("XDG_CONFIG_HOME", home)
        .env("XDG_DATA_HOME", home)
        .env("XDG_STATE_HOME", home)
        .env("XDG_CACHE_HOME", home)
        .output()?)
}

fn git(directory: &Path, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .output()?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!(
            "git {} failed: {}{}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn toplevel(directory: &Path) -> Result<String, Box<dyn Error>> {
    let output = git(directory, &["rev-parse", "--show-toplevel"])?;
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn tamper_lock_fingerprint(contents: &str) -> Result<String, Box<dyn Error>> {
    let marker = "config-fingerprint = \"sha256:";
    let index = contents
        .find(marker)
        .ok_or("lock is missing config-fingerprint")?
        + marker.len();
    let digit = contents
        .as_bytes()
        .get(index)
        .ok_or("fingerprint digit missing")?;
    if !digit.is_ascii_hexdigit() {
        return Err("fingerprint digit is not hexadecimal".into());
    }
    let replacement = if *digit == b'0' { "1" } else { "0" };
    let mut tampered = contents.to_owned();
    tampered.replace_range(index..=index, replacement);
    Ok(tampered)
}

#[test]
fn config_list_prints_built_ins_with_tabs_and_suffixes() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let home = isolation()?;
    let output = run(directory.path(), home.path(), &["config", "list"])?;
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).is_empty());
    assert_eq!(
        stdout(&output),
        "\
conventional\tbuilt-in\tv1 default available
research\tbuilt-in\tv1 available
infra-ops\tbuilt-in\tv1 available
"
    );
    Ok(())
}

#[test]
fn config_show_prints_human_requirement_labels() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let home = isolation()?;
    let research = run(
        directory.path(),
        home.path(),
        &["config", "show", "research"],
    )?;
    assert!(research.status.success(), "{}", stderr(&research));
    assert!(stderr(&research).is_empty());
    let shown = stdout(&research);
    assert!(shown.starts_with("research\nv1\n"));
    assert!(shown.contains("  claim\trequired\tsingle\n"));
    assert!(shown.contains("  falsifier\trecommended\tsingle\n"));
    assert!(!shown.contains("PropertyCondition"));
    assert!(!shown.contains("Required"));
    assert!(!shown.contains("Single"));

    let conventional = run(
        directory.path(),
        home.path(),
        &["config", "show", "conventional"],
    )?;
    assert!(conventional.status.success(), "{}", stderr(&conventional));
    let shown = stdout(&conventional);
    assert!(shown.contains(
        "  constraints\tconditional\tsingle\tnon-obvious-requirements\tRequired when non-obvious requirements bounded the change.\n"
    ));
    assert!(!shown.contains("PropertyCondition"));
    Ok(())
}

#[test]
fn init_reports_fresh_repeat_missing_lock_and_refuses_a_differing_lock()
-> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = isolation()?;
    let nested = repository.path().join("one/two");
    fs::create_dir_all(&nested)?;
    let root = toplevel(repository.path())?;

    let created = run(&nested, home.path(), &["init"])?;
    assert!(created.status.success(), "{}", stderr(&created));
    assert!(stderr(&created).is_empty());
    assert!(stdout(&created).contains(&format!(
        "Initialized gitserious in {root} (conventional@1)."
    )));
    let config = repository.path().join("gitserious.toml");
    let lock = repository.path().join("gitserious.lock");
    let authored = fs::read(&config)?;
    let locked = fs::read_to_string(&lock)?;
    assert_eq!(String::from_utf8(authored.clone())?, "config-version = 1\n");
    assert!(locked.contains("[[taxonomies]]"));
    assert!(locked.contains("id = \"research\""));
    assert!(!locked.contains("resolved-template"));
    assert!(!locked.contains("templates"));
    assert_eq!(
        fs::read_to_string(repository.path().join(".gitserious/.gitignore"))?,
        "*\n"
    );

    let repeated = run(repository.path(), home.path(), &["init"])?;
    assert!(repeated.status.success(), "{}", stderr(&repeated));
    assert!(stdout(&repeated).contains(&format!(
        "gitserious is already initialized in {root} (conventional@1)."
    )));
    assert_eq!(fs::read(&config)?, authored);
    assert_eq!(fs::read_to_string(&lock)?, locked);

    fs::remove_file(&lock)?;
    let missing = run(repository.path(), home.path(), &["init"])?;
    assert!(missing.status.success(), "{}", stderr(&missing));
    assert!(
        stdout(&missing).contains(&format!("Created missing lock in {root} (conventional@1)."))
    );
    assert_eq!(fs::read(&config)?, authored);
    assert_eq!(fs::read_to_string(&lock)?, locked);

    let tampered = tamper_lock_fingerprint(&locked)?;
    fs::write(&lock, &tampered)?;
    let refused = run(repository.path(), home.path(), &["init"])?;
    assert_eq!(refused.status.code(), Some(1));
    assert!(stdout(&refused).is_empty());
    assert!(stderr(&refused).contains(
        "project lock differs from the current library; run `gitserious config`, open Project, and review Apply/Refresh"
    ));
    assert!(!stdout(&refused).contains("Refreshed"));
    assert_eq!(fs::read(&config)?, authored);
    assert_eq!(fs::read_to_string(&lock)?, tampered);
    Ok(())
}

#[test]
fn linked_worktree_stale_lock_blocks_commit_on_that_root() -> Result<(), Box<dyn Error>> {
    let primary = repository()?;
    git(
        primary.path(),
        &[
            "-c",
            "user.name=Git Serious",
            "-c",
            "user.email=gitserious@example.com",
            "commit",
            "--allow-empty",
            "-m",
            "initial",
        ],
    )?;
    let worktrees = tempfile::tempdir()?;
    let linked = worktrees.path().join("linked");
    git(
        primary.path(),
        &[
            "worktree",
            "add",
            "--detach",
            linked.to_str().ok_or("linked worktree path is not utf-8")?,
        ],
    )?;
    let home = isolation()?;
    let primary_init = run(primary.path(), home.path(), &["init"])?;
    assert!(primary_init.status.success(), "{}", stderr(&primary_init));
    assert!(!linked.join("gitserious.toml").exists());
    assert!(!linked.join("gitserious.lock").exists());

    let linked_init = run(&linked, home.path(), &["init"])?;
    assert!(linked_init.status.success(), "{}", stderr(&linked_init));
    let linked_root = toplevel(&linked)?;
    assert!(stdout(&linked_init).contains(&format!(
        "Initialized gitserious in {linked_root} (conventional@1)."
    )));
    assert!(linked.join("gitserious.toml").is_file());
    assert!(linked.join("gitserious.lock").is_file());

    let primary_config = fs::read(primary.path().join("gitserious.toml"))?;
    let primary_lock = fs::read(primary.path().join("gitserious.lock"))?;
    assert_eq!(primary_lock, fs::read(linked.join("gitserious.lock"))?);
    let tampered = tamper_lock_fingerprint(&String::from_utf8(primary_lock.clone())?)?;
    fs::write(linked.join("gitserious.lock"), &tampered)?;
    let head = String::from_utf8(git(&linked, &["rev-parse", "HEAD"])?.stdout)?;

    let commit = run(&linked, home.path(), &["commit", "--type", "feat"])?;
    assert_eq!(commit.status.code(), Some(1));
    assert!(stdout(&commit).is_empty());
    let error = stderr(&commit);
    assert!(error.contains(
        "gitserious.toml and gitserious.lock disagree; run `gitserious config`, open Project, and review Apply/Refresh"
    ));
    assert!(!error.contains("interactive terminal"));
    assert!(!error.contains("not initialized"));
    assert_eq!(
        String::from_utf8(git(&linked, &["rev-parse", "HEAD"])?.stdout)?,
        head
    );
    assert_eq!(
        fs::read(primary.path().join("gitserious.toml"))?,
        primary_config
    );
    assert_eq!(
        fs::read(primary.path().join("gitserious.lock"))?,
        primary_lock
    );
    assert_eq!(
        fs::read_to_string(linked.join("gitserious.lock"))?,
        tampered
    );
    Ok(())
}
