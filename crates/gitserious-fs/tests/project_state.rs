use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use gitserious_app::{
    ConfigurationCatalog, ConfigurationCatalogError, CustomConfiguration, InitStatus,
    InitializeProjectError, ProjectConfig, ProjectState, ProjectStateStore, RepositoryLocator,
    built_in_effective_catalog, initialize_project, resolve_project_lock,
};
use gitserious_core::{
    ChangeTypeDefinition, ChangeTypeId, ChangeTypeSchema, Description, PropertyDefinition,
    PropertyKey, PropertyMultiplicity, PropertyRequirement, TaxonomyDefinition, TaxonomyId,
    TaxonomyVersion, TemplateDefinition, TemplateId, TemplateVersion, TypesetDefinition, TypesetId,
    TypesetVersion,
};
use gitserious_fs::{GitRepositoryLocator, ProjectStateError, TomlProjectStateStore};
use tempfile::TempDir;

fn catalog() -> Result<ConfigurationCatalog, ConfigurationCatalogError> {
    built_in_effective_catalog()
}

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

fn root(directory: &Path) -> Result<gitserious_app::RepositoryRoot, Box<dyn Error>> {
    Ok(GitRepositoryLocator.locate(directory)?)
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

fn populated_project_config() -> Result<ProjectConfig, Box<dyn Error>> {
    let taxonomy_id = TaxonomyId::new("delivery")?;
    let taxonomy = TaxonomyDefinition::new(
        taxonomy_id.clone(),
        TaxonomyVersion::new(2)?,
        Description::new("Delivery changes 🦀.")?,
        vec![ChangeTypeDefinition::new(
            ChangeTypeId::new("ship")?,
            Description::new("Release a durable artifact.")?,
        )],
    )?;
    let typeset = TypesetDefinition::new(
        taxonomy_id.clone(),
        TypesetId::new("evidence")?,
        TypesetVersion::new(3)?,
        Description::new("Release evidence requirements.")?,
        vec![ChangeTypeSchema::new(
            ChangeTypeId::new("ship")?,
            vec![PropertyDefinition::new(
                PropertyKey::new("verification")?,
                "How the artifact was verified.",
                PropertyRequirement::Required,
                PropertyMultiplicity::Multiple,
            )?],
        )?],
    )?;
    let alternate = TemplateDefinition::new(
        TemplateId::new("alternate")?,
        TemplateVersion::V1,
        Description::new("An alternate local selection.")?,
        taxonomy_id.clone(),
        typeset.id().clone(),
    );
    let selected = TemplateDefinition::new(
        TemplateId::new("release-team")?,
        TemplateVersion::new(4)?,
        Description::new("The selected release policy.")?,
        taxonomy_id,
        typeset.id().clone(),
    );
    let custom =
        CustomConfiguration::new(vec![taxonomy], vec![typeset], vec![selected, alternate])?;
    Ok(ProjectConfig::new(
        1,
        TemplateId::new("release-team")?,
        custom,
    )?)
}

#[test]
fn absent_state_has_no_filesystem_side_effects() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let paths = project_paths(repository.path());

    assert_eq!(
        TomlProjectStateStore.inspect(&root(repository.path())?)?,
        ProjectState::Absent
    );
    assert!(!paths.local_state.exists());
    assert!(!paths.config.exists());
    assert!(!paths.lock.exists());
    Ok(())
}

#[test]
fn initialization_writes_exact_config_and_ordered_lock() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let paths = project_paths(repository.path());
    fs::create_dir(&paths.local_state)?;
    fs::write(paths.local_state.join("notes.txt"), "preserve me")?;

    let outcome = initialize_project(
        &GitRepositoryLocator,
        &TomlProjectStateStore,
        &catalog()?,
        None,
        repository.path(),
    )?;

    assert_eq!(outcome.status(), InitStatus::Initialized);
    assert_eq!(
        fs::read_to_string(&paths.config)?,
        "config-version = 1\nactive-template = \"default\"\n\
         taxonomies = []\ntypesets = []\ntemplates = []\n"
    );
    let lock = fs::read_to_string(&paths.lock)?;
    assert!(lock.starts_with("# This file is generated by gitserious. Do not edit.\n"));
    assert!(lock.ends_with('\n'));
    assert_eq!(
        lock.matches("[[resolved-template.commit-types]]").count(),
        11
    );
    let mut previous = 0;
    for id in [
        "feat", "fix", "refactor", "perf", "test", "docs", "chore", "build", "ci", "style",
        "revert",
    ] {
        let position = lock[previous..]
            .find(&format!("id = \"{id}\""))
            .ok_or_else(|| format!("missing {id}"))?
            + previous;
        assert!(position >= previous);
        previous = position + 1;
    }
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
fn populated_project_configuration_round_trips_canonically() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let paths = project_paths(repository.path());
    let config = populated_project_config()?;
    let lock = resolve_project_lock(&config)?;

    TomlProjectStateStore.initialize(&root(repository.path())?, &config, &lock)?;

    assert_eq!(
        TomlProjectStateStore.inspect(&root(repository.path())?)?,
        ProjectState::Initialized {
            config: config.clone(),
            lock
        }
    );
    let contents = fs::read_to_string(paths.config)?;
    assert!(contents.starts_with("config-version = 1\nactive-template = \"release-team\"\n"));
    assert!(contents.contains("description = \"Delivery changes 🦀.\""));
    assert!(contents.contains("multiplicity = \"multiple\""));
    assert!(contents.contains("level = \"required\""));
    let taxonomy = contents.find("[[taxonomies]]").ok_or("missing taxonomy")?;
    let typeset = contents.find("[[typesets]]").ok_or("missing typeset")?;
    let alternate = contents
        .find("id = \"alternate\"")
        .ok_or("missing alternate template")?;
    let selected = contents
        .find("id = \"release-team\"")
        .ok_or("missing selected template")?;
    assert!(taxonomy < typeset && typeset < alternate && alternate < selected);
    assert!(contents.ends_with('\n'));
    Ok(())
}

#[test]
fn matching_state_is_byte_preserving_and_idempotent() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let paths = project_paths(repository.path());
    initialize_project(
        &GitRepositoryLocator,
        &TomlProjectStateStore,
        &catalog()?,
        None,
        repository.path(),
    )?;
    let config_before = fs::read(&paths.config)?;
    let lock_before = fs::read(&paths.lock)?;
    let config_modified = fs::metadata(&paths.config)?.modified()?;
    let lock_modified = fs::metadata(&paths.lock)?.modified()?;
    fs::remove_dir_all(&paths.local_state)?;

    let outcome = initialize_project(
        &GitRepositoryLocator,
        &TomlProjectStateStore,
        &catalog()?,
        None,
        repository.path(),
    )?;

    assert_eq!(outcome.status(), InitStatus::AlreadyInitialized);
    assert_eq!(fs::read(&paths.config)?, config_before);
    assert_eq!(fs::read(&paths.lock)?, lock_before);
    assert_eq!(fs::metadata(&paths.config)?.modified()?, config_modified);
    assert_eq!(fs::metadata(&paths.lock)?.modified()?, lock_modified);
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

    let outcome = initialize_project(
        &GitRepositoryLocator,
        &TomlProjectStateStore,
        &catalog()?,
        None,
        repository.path(),
    )?;

    assert_eq!(outcome.status(), InitStatus::Initialized);
    assert_eq!(fs::read_to_string(paths.local_ignore)?, "cache/**\n");
    assert_eq!(
        fs::read_to_string(paths.local_state.join("settings.local.toml"))?,
        "local = true\n"
    );
    Ok(())
}

#[test]
fn existing_valid_config_creates_only_a_missing_lock() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let paths = project_paths(repository.path());
    let authored = "config-version = 1\nactive-template = \"default\"\n\
                    taxonomies = []\ntypesets = []\ntemplates = []\n";
    fs::write(&paths.config, authored)?;

    let outcome = initialize_project(
        &GitRepositoryLocator,
        &TomlProjectStateStore,
        &catalog()?,
        None,
        repository.path(),
    )?;

    assert_eq!(outcome.status(), InitStatus::LockCreated);
    assert_eq!(fs::read_to_string(paths.config)?, authored);
    assert!(paths.lock.is_file());
    Ok(())
}

#[test]
fn recognized_stale_lock_is_replaced_without_rewriting_config() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let paths = project_paths(repository.path());
    initialize_project(
        &GitRepositoryLocator,
        &TomlProjectStateStore,
        &catalog()?,
        None,
        repository.path(),
    )?;
    let authored = fs::read(&paths.config)?;
    let current = fs::read_to_string(&paths.lock)?;
    let stale = current.replace(
        "config-fingerprint = \"sha256:e",
        "config-fingerprint = \"sha256:a",
    );
    assert_ne!(stale, current);
    fs::write(&paths.lock, stale)?;

    let outcome = initialize_project(
        &GitRepositoryLocator,
        &TomlProjectStateStore,
        &catalog()?,
        None,
        repository.path(),
    )?;

    assert_eq!(outcome.status(), InitStatus::LockRefreshed);
    assert_eq!(fs::read(&paths.config)?, authored);
    assert_eq!(fs::read_to_string(paths.lock)?, current);
    Ok(())
}

#[test]
fn concurrent_lock_change_is_preserved_and_refused() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let paths = project_paths(repository.path());
    initialize_project(
        &GitRepositoryLocator,
        &TomlProjectStateStore,
        &catalog()?,
        None,
        repository.path(),
    )?;
    let original = match TomlProjectStateStore.inspect(&root(repository.path())?)? {
        ProjectState::Initialized { lock, .. } => lock,
        state => return Err(format!("unexpected state: {state:?}").into()),
    };
    let changed = fs::read_to_string(&paths.lock)?.replace(
        "config-fingerprint = \"sha256:e",
        "config-fingerprint = \"sha256:a",
    );
    fs::write(&paths.lock, &changed)?;

    let error = TomlProjectStateStore.replace_lock(&root(repository.path())?, &original, &original);

    assert!(matches!(
        error,
        Err(ProjectStateError::ConcurrentLockChange(_))
    ));
    assert_eq!(fs::read_to_string(paths.lock)?, changed);
    assert!(fs::read_dir(repository.path())?.all(|entry| {
        entry.is_ok_and(|entry| {
            !entry
                .file_name()
                .to_string_lossy()
                .starts_with(".gitserious.lock.tmp.")
        })
    }));
    Ok(())
}

#[test]
fn malformed_unknown_and_unsupported_config_are_refused() -> Result<(), Box<dyn Error>> {
    for contents in [
        "not toml",
        "config-version = 2\nactive-template = \"default\"\n\
         taxonomies = []\ntypesets = []\ntemplates = []\n",
        "config-version = 1\nactive-template = \"default\"\n\
         taxonomies = []\ntypesets = []\ntemplates = []\nunknown = true\n",
        "config-version = 1\nactive-template = \"Custom\"\n\
         taxonomies = []\ntypesets = []\ntemplates = []\n",
        "config-version = 1\nactive-template = \"custom\"\n\
         typesets = []\ntemplates = []\n\
         [[taxonomies]]\nid = \"custom\"\nversion = 1\n\
         description = \"Custom.\"\nunknown = true\n\
         change-types = [{ id = \"change\", description = \"Change.\" }]\n",
    ] {
        let repository = repository()?;
        let paths = project_paths(repository.path());
        fs::write(paths.config, contents)?;

        assert!(matches!(
            TomlProjectStateStore.inspect(&root(repository.path())?),
            Err(ProjectStateError::ConfigFormat { .. })
        ));
    }
    Ok(())
}

#[test]
fn malformed_unknown_and_unsupported_lock_are_refused() -> Result<(), Box<dyn Error>> {
    for mutation in ["malformed", "unknown", "unsupported"] {
        let repository = repository()?;
        let paths = project_paths(repository.path());
        initialize_project(
            &GitRepositoryLocator,
            &TomlProjectStateStore,
            &catalog()?,
            None,
            repository.path(),
        )?;
        let contents = match mutation {
            "malformed" => String::from("not toml"),
            "unknown" => format!("{}unknown = true\n", fs::read_to_string(&paths.lock)?),
            "unsupported" => {
                fs::read_to_string(&paths.lock)?.replacen("lock-version = 1", "lock-version = 2", 1)
            }
            _ => return Err("unexpected mutation".into()),
        };
        fs::write(&paths.lock, contents)?;

        assert!(matches!(
            TomlProjectStateStore.inspect(&root(repository.path())?),
            Err(ProjectStateError::LockFormat { .. })
        ));
    }
    Ok(())
}

#[test]
fn orphan_lock_is_refused_without_guessing_config() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let paths = project_paths(repository.path());
    fs::write(paths.lock, "generated but orphaned")?;

    let error = initialize_project(
        &GitRepositoryLocator,
        &TomlProjectStateStore,
        &catalog()?,
        None,
        repository.path(),
    );

    assert!(matches!(error, Err(InitializeProjectError::OrphanLock)));
    Ok(())
}

#[test]
fn non_directory_and_non_regular_known_paths_are_refused() -> Result<(), Box<dyn Error>> {
    let non_directory_repository = repository()?;
    let paths = project_paths(non_directory_repository.path());
    fs::write(&paths.local_state, "collision")?;
    assert!(matches!(
        initialize_project(
            &GitRepositoryLocator,
            &TomlProjectStateStore,
            &catalog()?,
            None,
            non_directory_repository.path()
        ),
        Err(InitializeProjectError::Store(
            ProjectStateError::ExpectedDirectory(_)
        ))
    ));
    assert!(!paths.config.exists());
    assert!(!paths.lock.exists());

    for target in ["gitserious.toml", "gitserious.lock"] {
        let repository = repository()?;
        fs::create_dir(repository.path().join(target))?;
        assert!(matches!(
            TomlProjectStateStore.inspect(&root(repository.path())?),
            Err(ProjectStateError::ExpectedFile(_))
        ));
    }

    let marker_repository = repository()?;
    let paths = project_paths(marker_repository.path());
    fs::create_dir(&paths.local_state)?;
    fs::create_dir(&paths.local_ignore)?;
    assert!(matches!(
        TomlProjectStateStore.ensure_local_state(&root(marker_repository.path())?),
        Err(ProjectStateError::ExpectedFile(_))
    ));
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlinks_at_each_protected_path_are_refused() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::symlink;

    let symlinked_directory_repository = repository()?;
    let outside = tempfile::tempdir()?;
    let paths = project_paths(symlinked_directory_repository.path());
    symlink(outside.path(), &paths.local_state)?;
    assert!(matches!(
        TomlProjectStateStore.ensure_local_state(&root(symlinked_directory_repository.path())?),
        Err(ProjectStateError::Symlink(_))
    ));

    for target in ["gitserious.toml", "gitserious.lock"] {
        let repository = repository()?;
        let outside = tempfile::NamedTempFile::new()?;
        symlink(outside.path(), repository.path().join(target))?;
        assert!(matches!(
            TomlProjectStateStore.inspect(&root(repository.path())?),
            Err(ProjectStateError::Symlink(_))
        ));
    }

    let marker_repository = repository()?;
    let paths = project_paths(marker_repository.path());
    fs::create_dir(&paths.local_state)?;
    let outside = tempfile::NamedTempFile::new()?;
    symlink(outside.path(), &paths.local_ignore)?;
    assert!(matches!(
        TomlProjectStateStore.ensure_local_state(&root(marker_repository.path())?),
        Err(ProjectStateError::Symlink(_))
    ));
    Ok(())
}

#[test]
fn direct_store_creation_refuses_either_existing_root_artifact() -> Result<(), Box<dyn Error>> {
    for target in ["gitserious.toml", "gitserious.lock"] {
        let repository = repository()?;
        let paths = project_paths(repository.path());
        let collision = repository.path().join(target);
        fs::write(&collision, "preserve")?;
        let config = ProjectConfig::default_channel()?;
        let lock = resolve_project_lock(&config)?;

        assert!(matches!(
            TomlProjectStateStore.initialize(&root(repository.path())?, &config, &lock),
            Err(ProjectStateError::Collision(_))
        ));
        assert_eq!(fs::read_to_string(collision)?, "preserve");
        if target == "gitserious.toml" {
            assert!(!paths.lock.exists());
        } else {
            assert!(!paths.config.exists());
        }
    }
    Ok(())
}
