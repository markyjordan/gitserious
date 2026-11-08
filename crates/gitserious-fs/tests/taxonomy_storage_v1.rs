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
        Some(TaxonomyVersion::V1),
        Description::new("Team policy")?,
        None,
        vec![CommitTypeDefinition::new(
            Some(SchemaVersion::V1),
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

#[test]
fn custom_versions_disappear_at_library_and_project_boundaries() -> Result<(), Box<dyn Error>> {
    use gitserious_core::built_in_taxonomies;
    let directory = tempfile::tempdir()?;
    let paths = GlobalPaths::new(directory.path().join("library"));
    fs::create_dir_all(paths.config().as_path())?;
    let store = TomlGlobalConfigurationStore::new(paths.config().clone());
    let custom = r#"
[[taxonomies]]
id = "team"
version = 37
description = "Team policy"
created-on = "2001-02-03"
modified-on = "2002-03-04"
[taxonomies.derived-from]
taxonomy = "other-team"
version = 11
[[taxonomies.commit-types]]
id = "deploy"
schema-version = 9
description = "Deploy"
[[taxonomies.commit-types.properties]]
key = "checks"
description = "Checks"
[taxonomies.commit-types.properties.requirement]
level = "required"
"#;
    let source = format!(
        "config-version = 1\ndefault-taxonomy = 'team'\navailable-taxonomies = ['team']\n{custom}"
    );
    fs::write(store.path(), &source)?;
    let global = store.load()?;
    let record = &global.custom_taxonomies()[0];
    assert_eq!(record.version(), None);
    assert_eq!(record.commit_types()[0].schema_version(), None);
    assert_eq!(
        record
            .derived_from()
            .map(|source| source.reference().to_string()),
        Some("other-team".into())
    );
    assert_eq!(
        record.created_on().map(|date| date.to_string()),
        Some("2001-02-03".into())
    );
    assert_eq!(fs::read_to_string(store.path())?, source);
    store.compare_and_swap(&global, &global)?;
    let written: toml::Value = toml::from_str(&fs::read_to_string(store.path())?)?;
    let custom_wire = &written["taxonomies"][0];
    assert!(custom_wire.get("version").is_none());
    assert!(
        custom_wire["commit-types"][0]
            .get("schema-version")
            .is_none()
    );
    assert!(custom_wire["derived-from"].get("version").is_none());
    assert_eq!(written["config-version"].as_integer(), Some(1));

    let root = RepositoryRoot::new(directory.path().join("project"))?;
    fs::create_dir(root.as_path())?;
    let policy = ProjectConfiguration::new(
        1,
        global.default_taxonomy().clone(),
        vec![
            record.usable().ok_or("usable")?,
            built_in_taxonomies()[0].clone(),
        ],
    )?;
    let project_store = TomlProjectStore;
    project_store.create(&root, &policy)?;
    let path = root.as_path().join("gitserious.toml");
    let contents = fs::read_to_string(&path)?;
    let wire: toml::Value = toml::from_str(&contents)?;
    assert!(wire["taxonomies"][0].get("version").is_none());
    assert!(
        wire["taxonomies"][0]["commit-types"][0]
            .get("schema-version")
            .is_none()
    );
    assert_eq!(wire["taxonomies"][1]["version"].as_integer(), Some(1));
    assert_eq!(
        wire["taxonomies"][1]["commit-types"][0]["schema-version"].as_integer(),
        Some(1)
    );
    let legacy_project = format!(
        "config-version = 1\ndefault-taxonomy = 'team'\n{}",
        custom
            .replace("created-on = \"2001-02-03\"\n", "")
            .replace("modified-on = \"2002-03-04\"\n", "")
    );
    fs::write(&path, legacy_project)?;
    let loaded = project_store.load(&root)?.ok_or("project")?;
    assert_eq!(loaded.configuration.taxonomies()[0].version(), None);
    assert_eq!(
        loaded.configuration.taxonomies()[0].commit_types()[0].schema_version(),
        None
    );
    Ok(())
}
