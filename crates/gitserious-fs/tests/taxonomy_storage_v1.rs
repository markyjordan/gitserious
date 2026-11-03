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
    let paths = GlobalPaths::new(directory.path().to_path_buf());
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
    let paths = GlobalPaths::new(directory.path().to_path_buf());
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
fn retired_examples_load_without_writing_and_disappear_on_save() -> Result<(), Box<dyn Error>> {
    use gitserious_core::{
        CommitTypeDefinition, CommitTypeId, Description, PropertyDefinition, PropertyKey,
        PropertyRequirement, SchemaVersion, Taxonomy, TaxonomyId, TaxonomyVersion,
    };
    let directory = tempfile::tempdir()?;
    let paths = GlobalPaths::new(directory.path().join("library"));
    let store = TomlGlobalConfigurationStore::new(paths.config().clone());
    let before = store.load()?;
    let id = TaxonomyId::new("team")?;
    let taxonomy = Taxonomy::new(
        id.clone(),
        TaxonomyVersion::V1,
        Description::new("Team policy")?,
        None,
        vec![CommitTypeDefinition::new(
            SchemaVersion::V1,
            CommitTypeId::new("deploy")?,
            "Deploy",
            vec![PropertyDefinition::new(
                PropertyKey::new("checks")?,
                "Checks",
                PropertyRequirement::Required,
            )?],
        )?],
    )?;
    let global = GlobalConfiguration::new(1, id.clone(), vec![id], vec![taxonomy])?;
    store.compare_and_swap(&before, &global)?;
    let path = directory.path().join("library/config.toml");
    let clean = fs::read_to_string(&path)?;
    let legacy = clean.replace("key =", "example = 'old example'\nkey =");
    assert_ne!(legacy, clean);
    fs::write(&path, &legacy)?;
    assert_eq!(store.load()?, global);
    assert_eq!(fs::read_to_string(&path)?, legacy);
    store.compare_and_swap(&global, &global)?;
    assert!(!fs::read_to_string(&path)?.contains("example ="));
    fs::write(&path, clean.replace("key =", "unexpected = 'bad'\nkey ="))?;
    assert!(store.load().is_err());

    let root = RepositoryRoot::new(directory.path().join("project"))?;
    fs::create_dir(root.as_path())?;
    let config = ProjectConfig::inherited();
    let effective = resolve_effective_configuration(&global, &config)?;
    let lock = ProjectLock::capture(&config, &effective)?;
    let project_store = TomlProjectStateStore;
    project_store.ensure_local_state(&root)?;
    project_store.initialize(&root, &config, &lock)?;
    let path = root.as_path().join("gitserious.lock");
    let clean = fs::read_to_string(&path)?;
    let legacy = clean.replace("key =", "example = 'old example'\nkey =");
    fs::write(&path, &legacy)?;
    assert_eq!(
        project_store.inspect(&root)?,
        ProjectState::Initialized {
            config: config.clone(),
            lock: lock.clone()
        }
    );
    assert_eq!(fs::read_to_string(&path)?, legacy);
    project_store.replace_lock(&root, &lock, &lock)?;
    assert!(!fs::read_to_string(&path)?.contains("example ="));
    fs::write(&path, clean.replace("key =", "unexpected = 'bad'\nkey ="))?;
    assert!(project_store.inspect(&root).is_err());
    Ok(())
}
