mod support;

use std::error::Error;
use std::path::Path;
use std::process::Output;

use support::Home;

fn gitserious(directory: &Path, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
    Ok(Home::new()?
        .command(directory)
        .args(arguments)
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
        &["--help"][..],
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
    let home = Home::new()?;
    for arguments in [&["--help"][..], &["--version"][..]] {
        let output = home.run(directory.path(), arguments)?;
        assert!(output.status.success());
    }
    let output = home.run(directory.path(), &["config"])?;
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
    let home = Home::new()?;
    for (arguments, usage) in [
        (
            &["--help"][..],
            "Usage: gitserious [COMMAND]\n       gitserious [OPTIONS]\n",
        ),
        (&["help", "commit"][..], "Usage: gitserious commit\n"),
    ] {
        let output = home
            .command_for(&renamed, directory.path())
            .args(arguments)
            .output()?;
        assert!(output.status.success(), "{arguments:?}");
        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains(usage), "{arguments:?}: {stdout}");
    }
    Ok(())
}
