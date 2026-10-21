use std::error::Error;
use std::process::Command;

#[test]
fn bare_non_terminal_invocation_prints_help_without_creating_project_files()
-> Result<(), Box<dyn Error>> {
    let directory = tempfile::TempDir::new()?;
    let output = Command::new(env!("CARGO_BIN_EXE_gitserious"))
        .current_dir(directory.path())
        .output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("commit"));
    assert!(!directory.path().join("gitserious.toml").exists());
    assert!(!directory.path().join("gitserious.lock").exists());
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
