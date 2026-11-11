#![cfg(unix)]

mod support;
#[path = "support/terminal.rs"]
mod terminal;

use std::error::Error;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::time::{Duration, Instant};

use support::{Home, stdout};
use terminal::{Session, Streams};

fn pager(directory: &std::path::Path, exit: u8) -> Result<std::path::PathBuf, Box<dyn Error>> {
    let path = directory.join("pager");
    fs::write(
        &path,
        format!("#!/bin/sh\ncat >\"$PAGER_CAPTURE\"\nprintf 'pager-was-run\\n'\nexit {exit}\n"),
    )?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

#[test]
fn terminal_fixture_observes_input_resize_and_exit() -> Result<(), Box<dyn Error>> {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "stty -echo; stty size; read answer; stty size"]);
    let mut session = Session::spawn(&command, Streams::Terminal)?;
    session.wait_for(|screen| screen.contents().contains("40 120"))?;
    session.resize(45, 100)?;
    session.send(b"\n")?;
    assert!(session.finish()?.success());
    assert!(String::from_utf8_lossy(session.transcript()).contains("45 100"));
    Ok(())
}

#[test]
fn terminal_fixture_times_out_and_reaps_an_unfinished_child() -> Result<(), Box<dyn Error>> {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf ready; exec cat"]);
    let mut session = Session::spawn(&command, Streams::Terminal)?;
    session.wait_for(|screen| screen.contents().contains("ready"))?;
    let error = session
        .wait_for_within(Duration::from_millis(100), |screen| {
            screen.contents().contains("never emitted")
        })
        .err()
        .ok_or("a running child without the requested output must time out")?;
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(!session.terminate()?.success());
    Ok(())
}

#[test]
fn terminal_deadline_applies_even_when_output_is_already_queued() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let completed = temp.path().join("producer-complete");
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "printf ready; head -c 1048576 /dev/zero; : >\"$PRODUCER_COMPLETE\"",
        ])
        .env("PRODUCER_COMPLETE", &completed);
    let mut session = Session::spawn(&command, Streams::Terminal)?;
    session.wait_for(|screen| screen.contents().contains("ready"))?;
    let producer_deadline = Instant::now() + Duration::from_secs(5);
    while !completed.exists() {
        if Instant::now() >= producer_deadline {
            return Err("terminal output producer did not complete".into());
        }
        std::thread::yield_now();
    }
    assert!(
        session.transcript().len() < 1_048_576,
        "the producer output must still be queued"
    );
    let error = session
        .wait_for_within(Duration::ZERO, |_| false)
        .err()
        .ok_or("an expired deadline accepted queued output")?;
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    let _ = session.terminate()?;
    Ok(())
}

#[test]
fn terminal_short_help_uses_the_pager_and_preserves_its_exit_contract() -> Result<(), Box<dyn Error>>
{
    for pager_exit in [0, 17] {
        let temp = tempfile::tempdir()?;
        let home = Home::new()?;
        let capture = temp.path().join("captured-help");
        let mut command = home.command(temp.path());
        command
            .arg("-h")
            .env("PAGER", pager(temp.path(), pager_exit)?)
            .env("PAGER_CAPTURE", &capture);
        let mut session = Session::spawn(&command, Streams::Terminal)?;
        assert!(session.finish()?.success());
        assert!(session.contents().contains("pager-was-run"));
        let expected = home.run(temp.path(), &["-h"])?;
        assert_eq!(fs::read_to_string(capture)?, stdout(&expected));
    }
    Ok(())
}

#[test]
fn missing_pager_prints_short_help_and_succeeds() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let home = Home::new()?;
    let mut command = home.command(temp.path());
    command
        .arg("-h")
        .env("PAGER", temp.path().join("nonexistent-pager"));
    let mut session = Session::spawn(&command, Streams::Terminal)?;
    assert!(session.finish()?.success());
    let expected = home.run(temp.path(), &["-h"])?;
    assert_eq!(
        String::from_utf8(session.transcript().to_vec())?.replace("\r\n", "\n"),
        stdout(&expected)
    );
    Ok(())
}

#[test]
fn either_redirected_stream_prints_help_without_paging_or_starting_the_tui()
-> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let home = Home::new()?;
    let capture = temp.path().join("pager-output");
    let output = temp.path().join("stdout");
    for arguments in [
        ["-h"].as_slice(),
        ["--help"].as_slice(),
        ["help", "commit"].as_slice(),
    ] {
        for streams in [Streams::InputRedirected, Streams::OutputRedirected(&output)] {
            let mut command = home.command(temp.path());
            command
                .args(arguments)
                .env("PAGER", pager(temp.path(), 0)?)
                .env("PAGER_CAPTURE", &capture);
            let redirected_output = matches!(streams, Streams::OutputRedirected(_));
            let mut session = Session::spawn(&command, streams)?;
            assert!(session.finish()?.success());
            assert!(
                !capture.exists(),
                "a mixed-stream invocation started the pager"
            );
            let actual = if redirected_output {
                fs::read_to_string(&output)?
            } else {
                String::from_utf8(session.transcript().to_vec())?.replace("\r\n", "\n")
            };
            assert_eq!(actual, stdout(&home.run(temp.path(), arguments)?));
            assert!(
                !actual.contains("\x1b[?1049h"),
                "a mixed-stream invocation entered the TUI"
            );
        }
    }
    Ok(())
}

#[test]
fn terminal_long_and_topic_help_open_the_tui_without_paging() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let home = Home::new()?;
    let capture = temp.path().join("pager-output");
    for arguments in [["--help"].as_slice(), ["help", "commit"].as_slice()] {
        let mut command = home.command(temp.path());
        command
            .args(arguments)
            .env("PAGER", pager(temp.path(), 0)?)
            .env("PAGER_CAPTURE", &capture);
        let mut session = Session::spawn(&command, Streams::Terminal)?;
        session.wait_for(|screen| {
            screen.alternate_screen() && screen.contents().contains("gitserious v")
        })?;
        assert!(!capture.exists());
        session.send(b"q")?;
        assert!(session.finish()?.success());
        assert!(!session.screen().alternate_screen());
    }
    Ok(())
}

fn active_page(screen: &vt100::Screen, page: &str) -> bool {
    let row = screen.rows(0, screen.size().1).next().unwrap_or_default();
    row.find(page)
        .and_then(|column| u16::try_from(column).ok())
        .and_then(|column| screen.cell(0, column))
        .is_some_and(|cell| cell.bold() && cell.underline())
}

fn record(session: &Session, name: &str) -> io::Result<()> {
    if let Some(directory) = std::env::var_os("GITSERIOUS_TERMINAL_RECEIPTS") {
        let directory = std::path::PathBuf::from(directory);
        fs::create_dir_all(&directory)?;
        fs::write(directory.join(format!("{name}.txt")), session.contents())?;
        fs::write(directory.join(format!("{name}.ansi")), session.transcript())?;
    }
    Ok(())
}

fn initialized_repository() -> Result<(tempfile::TempDir, Home), Box<dyn Error>> {
    let repository = support::repository()?;
    let home = Home::new()?;
    home.init(repository.path())?;
    fs::write(repository.path().join("staged-file"), "reviewed later\n")?;
    support::git(repository.path(), &["add", "staged-file"])?;
    Ok((repository, home))
}

fn quit(session: &mut Session) -> Result<(), Box<dyn Error>> {
    session.send(b"q")?;
    assert!(session.finish()?.success());
    assert!(!session.screen().alternate_screen());
    assert!(!session.screen().hide_cursor());
    assert!(session.modes_restored(), "terminal modes were not restored");
    Ok(())
}

#[test]
fn executable_commands_select_the_initial_page_without_authoring_or_git_writes()
-> Result<(), Box<dyn Error>> {
    let (repository, home) = initialized_repository()?;
    let before = support::git(repository.path(), &["status", "--porcelain=v1"])?.stdout;
    for (arguments, page, content) in [
        (Vec::<&str>::new(), "Home", "Go to Commit"),
        (vec!["commit"], "Commit", "[enter] start commit"),
        (vec!["config"], "Config", "1  Project"),
        (
            vec!["help"],
            "Help",
            "Usage: gitserious [OPTIONS] [COMMAND]",
        ),
        (vec!["help", "commit"], "Help", "gitserious help: commit"),
    ] {
        let mut command = home.command(repository.path());
        command.args(&arguments);
        let mut session = Session::spawn(&command, Streams::Terminal)?;
        session
            .wait_for(|screen| active_page(screen, page) && screen.contents().contains(content))?;
        if page == "Commit" {
            assert!(session.contents().contains("Staged (1)"));
        }
        for other in ["Home", "Commit", "Config", "Help"] {
            assert_eq!(active_page(session.screen(), other), other == page);
        }
        record(&session, &[&["initial"][..], &arguments].concat().join("-"))?;
        quit(&mut session)?;
        assert_eq!(
            support::git(repository.path(), &["status", "--porcelain=v1"])?.stdout,
            before
        );
        assert!(!support::head_exists(repository.path())?);
    }
    Ok(())
}

#[test]
fn executable_commit_entry_matches_navigation_from_home() -> Result<(), Box<dyn Error>> {
    let (repository, home) = initialized_repository()?;
    let mut command = home.command(repository.path());
    command.arg("commit");
    let mut direct = Session::spawn(&command, Streams::Terminal)?;
    direct.wait_for(|screen| {
        active_page(screen, "Commit") && screen.contents().contains("[enter] start commit")
    })?;
    let expected = direct.contents();
    record(&direct, "commit-direct")?;
    quit(&mut direct)?;

    let mut navigated = Session::spawn(&home.command(repository.path()), Streams::Terminal)?;
    navigated.wait_for(|screen| {
        active_page(screen, "Home") && screen.contents().contains("Go to Commit")
    })?;
    navigated.send(b"\t")?;
    navigated.wait_for(|screen| {
        active_page(screen, "Commit") && screen.contents().contains("[enter] start commit")
    })?;
    for contract in [
        "Staged (1)",
        "staged-file",
        "Unstaged (0)",
        "[enter] start commit",
    ] {
        assert!(expected.contains(contract), "direct entry lacks {contract}");
        assert!(
            navigated.contents().contains(contract),
            "navigation lacks {contract}"
        );
    }
    record(&navigated, "commit-navigated")?;
    quit(&mut navigated)?;
    Ok(())
}

#[test]
fn missing_repository_blocks_commit_and_still_allows_config_navigation()
-> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let home = Home::new()?;
    let mut command = home.command(directory.path());
    command.arg("commit");
    let mut session = Session::spawn(&command, Streams::Terminal)?;
    session.wait_for(|screen| {
        active_page(screen, "Commit")
            && screen.contents().contains("Repository unavailable")
            && screen.contents().contains("Start commit unavailable")
    })?;
    record(&session, "commit-no-repository")?;
    session.send(b"\t")?;
    session.wait_for(|screen| {
        active_page(screen, "Config") && screen.contents().contains("1  Project")
    })?;
    record(&session, "config-no-repository")?;
    quit(&mut session)?;
    assert!(!directory.path().join(".git").exists());
    assert!(!directory.path().join("gitserious.toml").exists());
    Ok(())
}
