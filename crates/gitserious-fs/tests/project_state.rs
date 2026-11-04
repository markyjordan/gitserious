use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use gitserious_app::{
    GlobalConfiguration, InitOutcome, InitStatus, InitializeProjectError,
    ProjectConfigurationStore, RepositoryLocator, RepositoryRoot, initialize_project,
};
use gitserious_core::TaxonomyId;
use gitserious_fs::{
    GitRepositoryError, GitRepositoryLocator, ProjectStateError, TomlProjectStore,
};
use tempfile::TempDir;

type InitResult =
    Result<InitOutcome, InitializeProjectError<GitRepositoryError, ProjectStateError>>;

fn repository() -> Result<TempDir, Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(directory.path())
        .status()?;
    if !status.success() {
        return Err(format!("git init failed with {status}").into());
    }
    Ok(directory)
}

fn root(directory: &Path) -> Result<RepositoryRoot, Box<dyn Error>> {
    Ok(GitRepositoryLocator.locate(directory)?)
}

fn init_with(directory: &Path, global: &GlobalConfiguration) -> InitResult {
    initialize_project(&GitRepositoryLocator, &TomlProjectStore, global, directory)
}

fn init(directory: &Path) -> InitResult {
    init_with(directory, &GlobalConfiguration::default())
}

struct ProjectPaths {
    local_state: PathBuf,
    local_ignore: PathBuf,
    config: PathBuf,
    lock: PathBuf,
}

fn project_paths(directory: &Path) -> ProjectPaths {
    let local_state = directory.join(".gitserious");
    ProjectPaths {
        local_ignore: local_state.join(".gitignore"),
        local_state,
        config: directory.join("gitserious.toml"),
        lock: directory.join("gitserious.lock"),
    }
}

fn assert_ignored(repository: &Path, path: &str) -> Result<(), Box<dyn Error>> {
    let status = Command::new("git")
        .args(["check-ignore", "-q", path])
        .current_dir(repository)
        .status()?;
    assert!(status.success(), "expected Git to ignore {path}");
    Ok(())
}

fn has_temporary_files(directory: &Path) -> Result<bool, Box<dyn Error>> {
    Ok(fs::read_dir(directory)?
        .filter_map(Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().starts_with(".tmp")))
}

#[test]
fn absent_project_has_no_filesystem_side_effects() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let paths = project_paths(repository.path());

    assert_eq!(TomlProjectStore.load(&root(repository.path())?)?, None);
    assert!(!paths.local_state.exists());
    assert!(!paths.config.exists());
    Ok(())
}

#[test]
fn initialization_writes_one_file_with_ordered_complete_taxonomies() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let paths = project_paths(repository.path());
    fs::create_dir(&paths.local_state)?;
    fs::write(paths.local_state.join("notes.txt"), "preserve me")?;

    let outcome = init(repository.path())?;

    assert_eq!(outcome.status(), InitStatus::Initialized);
    let config = fs::read_to_string(&paths.config)?;
    assert!(config.starts_with("config-version = 1\ndefault-taxonomy = \"conventional\"\n"));
    let sections = config
        .split("\n[[taxonomies]]\n")
        .skip(1)
        .collect::<Vec<_>>();
    assert_eq!(
        sections
            .iter()
            .map(|section| section.lines().next().unwrap_or_default())
            .collect::<Vec<_>>(),
        [
            "id = \"conventional\"",
            "id = \"research\"",
            "id = \"infra-ops\""
        ]
    );
    let mut previous = 0;
    for id in [
        "feat", "fix", "refactor", "perf", "test", "docs", "chore", "build", "ci", "style",
        "revert",
    ] {
        previous += sections[0][previous..]
            .find(&format!("id = \"{id}\""))
            .ok_or_else(|| format!("{id} is missing or out of order"))?
            + 1;
    }
    assert!(!paths.lock.exists());
    assert!(!has_temporary_files(repository.path())?);
    assert_eq!(
        fs::read_to_string(paths.local_state.join("notes.txt"))?,
        "preserve me"
    );
    assert_eq!(fs::read_to_string(paths.local_ignore)?, "*\n");
    assert_ignored(repository.path(), ".gitserious/.gitignore")?;
    assert_ignored(repository.path(), ".gitserious/notes.txt")?;
    Ok(())
}

#[test]
fn reinitializing_is_byte_preserving_even_after_the_library_changes() -> Result<(), Box<dyn Error>>
{
    let repository = repository()?;
    let paths = project_paths(repository.path());
    init(repository.path())?;
    let before = fs::read(&paths.config)?;
    let modified = fs::metadata(&paths.config)?.modified()?;
    fs::remove_dir_all(&paths.local_state)?;
    let research_only = GlobalConfiguration::new(
        1,
        TaxonomyId::new("research")?,
        vec![TaxonomyId::new("research")?],
        Vec::new(),
    )?;

    let outcome = init_with(repository.path(), &research_only)?;

    assert_eq!(outcome.status(), InitStatus::AlreadyInitialized);
    assert_eq!(outcome.taxonomy().as_str(), "conventional");
    assert_eq!(fs::read(&paths.config)?, before);
    assert_eq!(fs::metadata(&paths.config)?.modified()?, modified);
    assert_eq!(fs::read_to_string(paths.local_ignore)?, "*\n");
    Ok(())
}

#[test]
fn existing_local_state_and_custom_ignore_are_preserved() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let paths = project_paths(repository.path());
    fs::create_dir(&paths.local_state)?;
    fs::write(&paths.local_ignore, "cache/**\n")?;
    fs::write(
        paths.local_state.join("settings.local.toml"),
        "local = true\n",
    )?;

    let outcome = init(repository.path())?;

    assert_eq!(outcome.status(), InitStatus::Initialized);
    assert_eq!(fs::read_to_string(paths.local_ignore)?, "cache/**\n");
    assert_eq!(
        fs::read_to_string(paths.local_state.join("settings.local.toml"))?,
        "local = true\n"
    );
    Ok(())
}

#[test]
fn invalid_existing_project_is_reported_and_never_regenerated() -> Result<(), Box<dyn Error>> {
    for authored in [
        "config-version = 1\n",
        "# team policy\nconfig-version = 1\ndefault-taxonomy = \"infra-ops\"\n",
        "not = [valid\n",
    ] {
        let repository = repository()?;
        let paths = project_paths(repository.path());
        fs::write(&paths.config, authored)?;

        let error = init(repository.path())
            .err()
            .ok_or("an invalid project was accepted")?;

        assert!(matches!(
            error,
            InitializeProjectError::Store(ProjectStateError::Format { .. })
        ));
        assert!(error.to_string().contains("gitserious.toml"), "{error}");
        assert_eq!(fs::read_to_string(&paths.config)?, authored);
        assert!(!paths.local_state.exists());
    }
    Ok(())
}

#[test]
fn obsolete_lock_is_ignored_and_never_written() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let paths = project_paths(repository.path());
    fs::write(&paths.lock, "not project policy\n")?;

    init(repository.path())?;

    assert_eq!(fs::read_to_string(&paths.lock)?, "not project policy\n");
    assert!(TomlProjectStore.load(&root(repository.path())?)?.is_some());
    Ok(())
}

#[test]
fn non_directory_and_non_regular_known_paths_are_refused() -> Result<(), Box<dyn Error>> {
    let non_directory_repository = repository()?;
    let paths = project_paths(non_directory_repository.path());
    fs::write(&paths.local_state, "collision")?;
    assert!(matches!(
        init(non_directory_repository.path()),
        Err(InitializeProjectError::Store(_))
    ));
    assert_eq!(fs::read_to_string(&paths.local_state)?, "collision");
    assert!(!paths.config.exists());

    let directory_repository = repository()?;
    fs::create_dir(directory_repository.path().join("gitserious.toml"))?;
    assert!(matches!(
        TomlProjectStore.load(&root(directory_repository.path())?),
        Err(ProjectStateError::UnsafePath(_))
    ));

    let marker_repository = repository()?;
    let paths = project_paths(marker_repository.path());
    fs::create_dir(&paths.local_state)?;
    fs::create_dir(&paths.local_ignore)?;
    assert!(matches!(
        TomlProjectStore.ensure_local_state(&root(marker_repository.path())?),
        Err(ProjectStateError::UnsafePath(_))
    ));
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlinked_local_state_is_refused() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::symlink;

    let symlinked_directory_repository = repository()?;
    let outside = tempfile::tempdir()?;
    let paths = project_paths(symlinked_directory_repository.path());
    symlink(outside.path(), &paths.local_state)?;
    assert!(matches!(
        init(symlinked_directory_repository.path()),
        Err(InitializeProjectError::Store(
            ProjectStateError::UnsafePath(_)
        ))
    ));
    assert_eq!(fs::read_dir(outside.path())?.count(), 0);
    assert!(!paths.config.exists());

    let marker_repository = repository()?;
    let paths = project_paths(marker_repository.path());
    fs::create_dir(&paths.local_state)?;
    let outside = tempfile::NamedTempFile::new()?;
    symlink(outside.path(), &paths.local_ignore)?;
    assert!(matches!(
        TomlProjectStore.ensure_local_state(&root(marker_repository.path())?),
        Err(ProjectStateError::UnsafePath(_))
    ));
    assert_eq!(fs::read(outside.path())?, b"");
    Ok(())
}
