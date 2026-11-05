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
    assert_eq!(
        output.stdout,
        gitserious(directory.path(), &["--help"])?.stdout
    );
    assert!(!directory.path().join("gitserious.toml").exists());
    assert!(!directory.path().join("gitserious.lock").exists());
    Ok(())
}

#[test]
fn help_never_pages_when_stdout_is_not_a_terminal() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::TempDir::new()?;
    for command in [&[][..], &["commit"][..]] {
        let paged = gitserious(directory.path(), &[&["help"][..], command].concat())?;
        let direct = gitserious(
            directory.path(),
            &[&["help", "--no-pager"][..], command].concat(),
        )?;
        assert!(paged.status.success(), "{command:?}");
        assert!(paged.stderr.is_empty(), "{command:?}");
        assert!(!paged.stdout.is_empty(), "{command:?}");
        assert_eq!(paged.stdout, direct.stdout, "{command:?}");
        assert!(!paged.stdout.contains(&0x1b), "{command:?}");
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
