#![allow(clippy::expect_used)]

use std::error::Error;
use std::fs;

use gitserious_app::{
    GlobalConfiguration, GlobalConfigurationStore, GlobalPaths, ProjectConfiguration,
    ProjectConfigurationStore, RepositoryRoot,
};
use gitserious_fs::{TomlGlobalConfigurationStore, TomlProjectStore};

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
    let global = GlobalConfiguration::new(
        1,
        id.clone(),
        vec![id.clone()],
        vec![taxonomy.clone().into()],
    )?;
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
    let project = ProjectConfiguration::new(1, id, vec![taxonomy])?;
    let project_store = TomlProjectStore;
    project_store.create(&root, &project)?;
    let path = root.as_path().join("gitserious.toml");
    let clean = fs::read_to_string(&path)?;
    let legacy = clean.replace("key =", "example = 'old example'\nkey =");
    assert_ne!(legacy, clean);
    fs::write(&path, &legacy)?;
    let stored = project_store.load(&root)?.ok_or("project did not load")?;
    assert_eq!(stored.configuration, project);
    assert_eq!(fs::read_to_string(&path)?, legacy);
    project_store.replace(&root, stored.revision, &project)?;
    assert!(!fs::read_to_string(&path)?.contains("example ="));
    fs::write(&path, clean.replace("key =", "unexpected = 'bad'\nkey ="))?;
    assert!(project_store.load(&root).is_err());
    Ok(())
}
