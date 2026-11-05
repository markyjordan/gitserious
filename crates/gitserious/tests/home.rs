use std::error::Error;
use std::path::Path;
use std::process::{Command, Output};

fn gitserious(directory: &Path, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
    Ok(Command::new(env!("CARGO_BIN_EXE_gitserious"))
        .args(arguments)
        .current_dir(directory)
        .env("PAGER", "sed -e s/^/paged:/")
        .output()?)
}

#[test]
fn bare_non_terminal_invocation_prints_concise_help_without_creating_project_files()
-> Result<(), Box<dyn Error>> {
    let directory = tempfile::TempDir::new()?;
    let output = gitserious(directory.path(), &[])?;
    assert!(output.status.success());
    assert_eq!(output.stdout, gitserious(directory.path(), &["-h"])?.stdout);
    assert!(!directory.path().join("gitserious.toml").exists());
    assert!(!directory.path().join("gitserious.lock").exists());
    Ok(())
}

#[test]
fn help_never_pages_when_stdout_is_not_a_terminal() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::TempDir::new()?;
    for arguments in [
        &["-h"][..],
        &["commit", "-h"][..],
        &["--help"][..],
        &["commit", "--help"][..],
        &["help"][..],
        &["help", "commit"][..],
    ] {
        let output = gitserious(directory.path(), arguments)?;
        let stdout = String::from_utf8(output.stdout)?;
        assert!(output.status.success(), "{arguments:?}");
        assert!(output.stderr.is_empty(), "{arguments:?}");
        assert!(stdout.starts_with(char::is_alphabetic), "{arguments:?}");
        assert!(!stdout.contains("paged:"), "{arguments:?}: {stdout}");
        assert!(!stdout.contains('\x1b'), "{arguments:?}");
    }
    Ok(())
}

#[test]
fn explicit_help_version_and_non_terminal_config_keep_their_contracts() -> Result<(), Box<dyn Error>>
{
    let directory = tempfile::TempDir::new()?;
    for arguments in [
        &["--help"][..],
        &["--version"][..],
        &["commit", "--help"][..],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_gitserious"))
            .args(arguments)
            .current_dir(directory.path())
            .output()?;
        assert!(output.status.success());
    }
    let output = Command::new(env!("CARGO_BIN_EXE_gitserious"))
        .arg("config")
        .current_dir(directory.path())
        .output()?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)?
            .contains("configuration editing requires an interactive terminal")
    );
    assert!(!directory.path().join("gitserious.toml").exists());
    Ok(())
}

#[test]
fn help_names_gitserious_whatever_the_executable_is_called() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::TempDir::new()?;
    let renamed = directory
        .path()
        .join(format!("renamed{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(env!("CARGO_BIN_EXE_gitserious"), &renamed)?;
    for (arguments, usage) in [
        (&["--help"][..], "Usage: gitserious [OPTIONS] [COMMAND]"),
        (
            &["help", "commit"][..],
            "Usage: gitserious commit [OPTIONS]",
        ),
    ] {
        let output = Command::new(&renamed)
            .args(arguments)
            .current_dir(directory.path())
            .output()?;
        assert!(output.status.success(), "{arguments:?}");
        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains(usage), "{arguments:?}: {stdout}");
    }
    Ok(())
}
