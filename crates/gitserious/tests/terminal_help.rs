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
