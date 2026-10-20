#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;

use gitserious_app::{
    GlobalConfiguration, GlobalConfigurationStore, GlobalPaths, ProjectConfig, ProjectLock,
    ProjectState, ProjectStateStore, RepositoryRoot, TaxonomyOrigin, fingerprint_project_config,
    resolve_effective_configuration,
};
use gitserious_core::TaxonomyId;
use gitserious_fs::{TomlGlobalConfigurationStore, TomlProjectStateStore};

#[test]
fn absent_global_config_uses_all_built_ins_and_round_trips() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let paths = GlobalPaths::new(
        directory.path().to_path_buf(),
        directory.path().join("data"),
        directory.path().join("state"),
        directory.path().join("cache"),
    );
    let store = TomlGlobalConfigurationStore::new(paths.config().clone());
    let original = store.load()?;
    assert_eq!(original.available_taxonomies().len(), 3);
    store.compare_and_swap(&original, &original)?;
    assert_eq!(store.load()?, original);
    let contents = fs::read_to_string(directory.path().join("config.toml"))?;
    assert!(contents.contains("default-taxonomy = \"conventional\""));
    assert!(!contents.contains("typesets"));
    assert!(!contents.contains("templates"));
    Ok(())
}

#[test]
fn legacy_global_shape_fails_with_actionable_message() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("config.toml"),
        "config-version = 1\ntemplates = []\ntypesets = []\n",
    )?;
    let paths = GlobalPaths::new(
        directory.path().to_path_buf(),
        directory.path().join("d"),
        directory.path().join("s"),
        directory.path().join("c"),
    );
    let error = TomlGlobalConfigurationStore::new(paths.config().clone())
        .load()
        .expect_err("legacy shape must fail");
    assert!(error.to_string().contains("superseded pre-release"));
    Ok(())
}

#[test]
fn project_lock_contains_portable_taxonomy_definitions() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let root = RepositoryRoot::new(directory.path().to_path_buf())?;
    let global = GlobalConfiguration::default();
    let config = ProjectConfig::inherited();
    let effective = resolve_effective_configuration(&global, &config)?;
    let lock = ProjectLock::capture(&config, &effective)?;
    let store = TomlProjectStateStore;
    store.ensure_local_state(&root)?;
    store.initialize(&root, &config, &lock)?;
    assert_eq!(
        store.inspect(&root)?,
        ProjectState::Initialized { config, lock }
    );
    let contents = fs::read_to_string(directory.path().join("gitserious.lock"))?;
    assert!(contents.contains("[[taxonomies]]"));
    assert!(contents.contains("id = \"research\""));
    assert!(!contents.contains("resolved-template"));
    Ok(())
}

#[test]
fn reloaded_fork_keeps_lineage() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let paths = GlobalPaths::new(
        directory.path().to_path_buf(),
        directory.path().join("data"),
        directory.path().join("state"),
        directory.path().join("cache"),
    );
    let store = TomlGlobalConfigurationStore::new(paths.config().clone());
    let original = store.load()?;
    let source = original
        .catalog()?
        .find(&TaxonomyId::new("conventional")?)
        .cloned()
        .ok_or("conventional taxonomy is missing")?;
    let fork = source.fork(TaxonomyId::new("team-notes")?);
    let fork_id = fork.id().clone();
    let mut available = original.available_taxonomies().to_vec();
    available.push(fork_id.clone());
    let replacement = GlobalConfiguration::new(
        original.version(),
        original.default_taxonomy().clone(),
        available,
        vec![fork.clone()],
    )?;
    store.compare_and_swap(&original, &replacement)?;

    let loaded = store.load()?;
    assert_eq!(loaded.custom_taxonomies(), &[fork]);
    assert_eq!(
        loaded.catalog()?.origin(&fork_id),
        Some(TaxonomyOrigin::Custom)
    );
    let lineage = loaded.custom_taxonomies()[0]
        .derived_from()
        .ok_or("reloaded fork is missing lineage")?;
    assert_eq!(lineage.id().as_str(), "conventional");
    assert_eq!(lineage.version().get(), 1);
    let contents = fs::read_to_string(directory.path().join("config.toml"))?;
    assert!(contents.contains("id = \"team-notes\""));
    assert!(contents.contains("derived-from"));
    Ok(())
}

#[test]
fn reloaded_project_override_pins_research() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let root = RepositoryRoot::new(directory.path().to_path_buf())?;
    let config = ProjectConfig::new(1, Some(TaxonomyId::new("research")?), None)?;
    let effective = resolve_effective_configuration(&GlobalConfiguration::default(), &config)?;
    let lock = ProjectLock::capture(&config, &effective)?;
    TomlProjectStateStore.initialize(&root, &config, &lock)?;

    let ProjectState::Initialized {
        config: loaded_config,
        lock: loaded_lock,
    } = TomlProjectStateStore.inspect(&root)?
    else {
        return Err("expected an initialized project".into());
    };
    assert_eq!(
        loaded_config.default_taxonomy().map(TaxonomyId::as_str),
        Some("research")
    );
    assert!(loaded_config.available_taxonomies().is_none());
    assert_eq!(loaded_lock.default_taxonomy().as_str(), "research");
    assert!(loaded_lock.matches(&loaded_config));
    assert_eq!(loaded_lock.taxonomies().len(), 3);
    let contents = fs::read_to_string(directory.path().join("gitserious.toml"))?;
    assert!(contents.contains("default-taxonomy = \"research\""));
    assert!(!contents.contains("available-taxonomies"));
    Ok(())
}

#[test]
fn reloaded_fingerprint_mismatch_does_not_match_project_config() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let root = RepositoryRoot::new(directory.path().to_path_buf())?;
    let config = ProjectConfig::inherited();
    let effective = resolve_effective_configuration(&GlobalConfiguration::default(), &config)?;
    let lock = ProjectLock::capture(&config, &effective)?;
    let store = TomlProjectStateStore;
    store.initialize(&root, &config, &lock)?;
    let other = ProjectConfig::new(1, Some(TaxonomyId::new("research")?), None)?;
    let mismatched = ProjectLock::new(
        lock.version(),
        fingerprint_project_config(&other),
        lock.default_taxonomy().clone(),
        lock.taxonomies().to_vec(),
    )?;
    store.replace_lock(&root, &lock, &mismatched)?;

    let ProjectState::Initialized {
        config: loaded_config,
        lock: loaded_lock,
    } = store.inspect(&root)?
    else {
        return Err("expected an initialized project".into());
    };
    assert_eq!(loaded_config, config);
    assert_eq!(loaded_lock, mismatched);
    assert!(!loaded_lock.matches(&loaded_config));
    let contents = fs::read_to_string(directory.path().join("gitserious.lock"))?;
    assert!(contents.contains(&mismatched.config_fingerprint().to_string()));
    assert!(!contents.contains(&lock.config_fingerprint().to_string()));
    Ok(())
}
