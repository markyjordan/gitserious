#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;

use gitserious_app::{
    GlobalConfiguration, GlobalConfigurationStore, GlobalPaths, ProjectConfig, ProjectLock,
    ProjectState, ProjectStateStore, RepositoryRoot, resolve_effective_configuration,
};
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
