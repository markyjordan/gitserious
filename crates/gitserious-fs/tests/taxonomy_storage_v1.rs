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
fn property_examples_survive_global_storage_and_portable_project_locks()
-> Result<(), Box<dyn Error>> {
    use gitserious_core::{
        CommitTypeDefinition, CommitTypeId, Description, PropertyDefinition, PropertyExample,
        PropertyKey, PropertyRequirement, SchemaVersion, Taxonomy, TaxonomyId, TaxonomyVersion,
    };
    let directory = tempfile::tempdir()?;
    let paths = GlobalPaths::new(directory.path().join("library"));
    let global_store = TomlGlobalConfigurationStore::new(paths.config().clone());
    let before = global_store.load()?;
    let message = "deploy(db): provision a replica\nverification:\n  Reads stay under 2000 qps. 👩‍💻界e\u{301}\n\nChecked staging.\n";
    let taxonomy_id = TaxonomyId::new("custom-example")?;
    let taxonomy = |example: &str| -> Result<Taxonomy, Box<dyn Error>> {
        Ok(Taxonomy::new(
            taxonomy_id.clone(),
            TaxonomyVersion::new(1)?,
            Description::new("Custom deployment taxonomy")?,
            None,
            vec![CommitTypeDefinition::new(
                SchemaVersion::new(1)?,
                CommitTypeId::new("deploy")?,
                "Deploy a service",
                vec![
                    PropertyDefinition::new(
                        PropertyKey::new("verification")?,
                        "How it was checked",
                        PropertyRequirement::Required,
                    )?
                    .with_example(PropertyExample::new(example)?),
                ],
            )?],
        )?)
    };
    let first = taxonomy(message)?;
    let changed = taxonomy("deploy: other\nverification:\nOther.")?;
    assert_ne!(
        gitserious_app::fingerprint_taxonomy(&first),
        gitserious_app::fingerprint_taxonomy(&changed)
    );
    assert_eq!(
        gitserious_app::fingerprint_taxonomy(&first),
        gitserious_app::fingerprint_taxonomy(&first.clone())
    );
    let global = GlobalConfiguration::new(1, taxonomy_id.clone(), vec![taxonomy_id], vec![first])?;
    global_store.compare_and_swap(&before, &global)?;
    assert_eq!(global_store.load()?, global);
    assert_eq!(
        global_store.load()?.custom_taxonomies()[0].commit_types()[0].properties()[0]
            .example()
            .ok_or("stored example")?
            .as_str(),
        message
    );
    let root = RepositoryRoot::new(directory.path().join("project"))?;
    fs::create_dir(root.as_path())?;
    let config = ProjectConfig::inherited();
    let effective = resolve_effective_configuration(&global, &config)?;
    let lock = ProjectLock::capture(&config, &effective)?;
    let project_store = TomlProjectStateStore;
    project_store.ensure_local_state(&root)?;
    project_store.initialize(&root, &config, &lock)?;
    assert_eq!(
        project_store.inspect(&root)?,
        ProjectState::Initialized { config, lock }
    );
    assert!(fs::read_to_string(root.as_path().join("gitserious.lock"))?.contains("example ="));
    Ok(())
}
