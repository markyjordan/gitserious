mod support;

use std::error::Error;
use std::fs;

use support::{Home, git, head_exists, repository, stderr, stdout};

#[test]
fn commit_without_a_terminal_stops_before_the_commit_page() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;

    let absent = home.run(repository.path(), &["commit"])?;
    assert_eq!(absent.status.code(), Some(1));
    assert!(stdout(&absent).is_empty());
    assert_eq!(
        stderr(&absent),
        "error: commit requires an interactive terminal\n"
    );
    assert!(!repository.path().join("gitserious.toml").exists());

    home.init(repository.path())?;
    let config = repository.path().join("gitserious.toml");
    let valid = fs::read_to_string(&config)?;
    let broken = valid.replace(
        "default-taxonomy = \"conventional\"",
        "default-taxonomy = \"missing\"",
    );
    fs::write(&config, &broken)?;
    let invalid = home.run(repository.path(), &["commit"])?;
    assert_eq!(invalid.status.code(), Some(1));
    assert!(stdout(&invalid).is_empty());
    assert_eq!(
        stderr(&invalid),
        "error: commit requires an interactive terminal\n"
    );
    assert_eq!(fs::read_to_string(&config)?, broken);
    assert!(!head_exists(repository.path())?);
    Ok(())
}

#[test]
fn a_cloned_project_authors_without_any_usable_library() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let author = Home::new()?;
    author.init(repository.path())?;
    let contributor = Home::new()?;
    let library = contributor.library_path();
    fs::create_dir_all(library.parent().ok_or("library has a parent")?)?;
    fs::write(&library, "not = [toml\n")?;

    let output = contributor.run(repository.path(), &["commit"])?;

    assert_eq!(
        stderr(&output),
        "error: commit requires an interactive terminal\n"
    );
    assert_eq!(fs::read_to_string(&library)?, "not = [toml\n");
    Ok(())
}

#[test]
fn commit_rejects_non_terminal_execution_without_touching_policy_or_git()
-> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;
    home.init(repository.path())?;
    let config = fs::read(repository.path().join("gitserious.toml"))?;
    git(repository.path(), &["config", "core.editor", "false"])?;
    fs::write(repository.path().join("staged.txt"), "staged\n")?;
    git(repository.path(), &["add", "staged.txt"])?;

    let output = home.run(repository.path(), &["commit"])?;

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
    assert_eq!(
        stderr(&output),
        "error: commit requires an interactive terminal\n"
    );
    assert_eq!(fs::read(repository.path().join("gitserious.toml"))?, config);
    assert!(!repository.path().join("gitserious.lock").exists());
    assert!(!head_exists(repository.path())?);
    Ok(())
}
