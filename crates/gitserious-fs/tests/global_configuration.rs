use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use gitserious_app::{CustomConfiguration, GlobalConfigurationStore, GlobalPaths};
use gitserious_core::{
    ChangeTypeDefinition, ChangeTypeId, ChangeTypeSchema, ConditionId, Description,
    PropertyCondition, PropertyDefinition, PropertyKey, PropertyMultiplicity, PropertyRequirement,
    TaxonomyDefinition, TaxonomyId, TaxonomyVersion, TemplateDefinition, TemplateId,
    TemplateVersion, TypesetDefinition, TypesetId, TypesetVersion,
};
use gitserious_fs::{GlobalConfigurationError, TomlGlobalConfigurationStore};
use tempfile::TempDir;

fn store(root: &Path) -> TomlGlobalConfigurationStore {
    let paths = GlobalPaths::new(
        root.join("config"),
        root.join("data"),
        root.join("state"),
        root.join("cache"),
    );
    TomlGlobalConfigurationStore::new(paths.config().clone())
}

fn property(
    key: &str,
    requirement: PropertyRequirement,
    multiplicity: PropertyMultiplicity,
) -> Result<PropertyDefinition, Box<dyn Error>> {
    Ok(PropertyDefinition::new(
        PropertyKey::new(key)?,
        format!("Durable meaning of {key} 🦀."),
        requirement,
        multiplicity,
    )?)
}

fn sample_configuration() -> Result<CustomConfiguration, Box<dyn Error>> {
    let taxonomy_id = TaxonomyId::new("custom")?;
    let taxonomy = TaxonomyDefinition::new(
        taxonomy_id.clone(),
        TaxonomyVersion::new(2)?,
        Description::new("A custom taxonomy 🦀.")?,
        vec![ChangeTypeDefinition::new(
            ChangeTypeId::new("change")?,
            Description::new("A meaningful change.")?,
        )],
    )?;
    let conditional = PropertyCondition::new(
        ConditionId::new("known-cost")?,
        "Required when a known cost exists.",
    )?;
    let typeset = TypesetDefinition::new(
        taxonomy_id.clone(),
        TypesetId::new("all-levels")?,
        TypesetVersion::new(3)?,
        Description::new("Every requirement level.")?,
        vec![ChangeTypeSchema::new(
            ChangeTypeId::new("change")?,
            vec![
                property(
                    "required",
                    PropertyRequirement::Required,
                    PropertyMultiplicity::Single,
                )?,
                property(
                    "recommended",
                    PropertyRequirement::Recommended,
                    PropertyMultiplicity::Single,
                )?,
                property(
                    "optional",
                    PropertyRequirement::Optional,
                    PropertyMultiplicity::Multiple,
                )?,
                property(
                    "conditional",
                    PropertyRequirement::Conditional(conditional),
                    PropertyMultiplicity::Single,
                )?,
            ],
        )?],
    )?;
    let template = TemplateDefinition::new(
        TemplateId::new("custom-template")?,
        TemplateVersion::new(4)?,
        Description::new("Reusable custom policy.")?,
        taxonomy_id,
        typeset.id().clone(),
    );
    Ok(CustomConfiguration::new(
        vec![taxonomy],
        vec![typeset],
        vec![template],
    )?)
}

fn write_config(root: &Path, contents: &str) -> Result<PathBuf, Box<dyn Error>> {
    let directory = root.join("config");
    fs::create_dir_all(&directory)?;
    let path = directory.join("config.toml");
    fs::write(&path, contents)?;
    Ok(path)
}

#[test]
fn missing_configuration_loads_empty_without_creating_storage() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let store = store(temporary.path());
    assert_eq!(store.load()?, CustomConfiguration::default());
    assert!(!temporary.path().join("config").exists());
    assert_eq!(store.path(), temporary.path().join("config/config.toml"));
    Ok(())
}

#[test]
fn compare_and_swap_creates_round_trips_and_replaces_complete_configuration()
-> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let store = store(temporary.path());
    let expected = CustomConfiguration::default();
    let replacement = sample_configuration()?;
    store.compare_and_swap(&expected, &replacement)?;
    assert_eq!(store.load()?, replacement);

    let contents = fs::read_to_string(store.path())?;
    assert!(contents.starts_with("config-version = 1\n"));
    assert!(contents.contains("level = \"required\""));
    assert!(contents.contains("level = \"recommended\""));
    assert!(contents.contains("level = \"optional\""));
    assert!(contents.contains("level = \"conditional\""));
    assert!(contents.contains("multiplicity = \"multiple\""));
    assert!(contents.ends_with('\n'));
    assert!(
        fs::read_dir(temporary.path().join("config"))?
            .filter_map(Result::ok)
            .all(|entry| !entry.file_name().to_string_lossy().contains(".tmp."))
    );

    store.compare_and_swap(&replacement, &CustomConfiguration::default())?;
    assert_eq!(store.load()?, CustomConfiguration::default());
    Ok(())
}

#[test]
fn stale_expected_snapshots_are_rejected_without_overwrite() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let store = store(temporary.path());
    let replacement = sample_configuration()?;
    store.compare_and_swap(&CustomConfiguration::default(), &replacement)?;
    let before = fs::read(store.path())?;
    let result = store.compare_and_swap(
        &CustomConfiguration::default(),
        &CustomConfiguration::default(),
    );
    assert!(matches!(
        result,
        Err(GlobalConfigurationError::ConcurrentChange(_))
    ));
    assert_eq!(fs::read(store.path())?, before);
    Ok(())
}

#[test]
fn strict_format_rejects_unknown_fields_unsupported_versions_and_invalid_values()
-> Result<(), Box<dyn Error>> {
    let cases = [
        (
            "config-version = 1\ntaxonomies = []\ntypesets = []\ntemplates = []\nunknown = true\n",
            "unknown field",
        ),
        (
            "config-version = 2\ntaxonomies = []\ntypesets = []\ntemplates = []\n",
            "unsupported custom configuration version 2",
        ),
        (
            "config-version = 1\ntypesets = []\ntemplates = []\n[[taxonomies]]\nid = \"Bad\"\nversion = 1\ndescription = \"Bad.\"\nchange-types = [{ id = \"change\", description = \"Change.\" }]\n",
            "invalid taxonomies[0].id",
        ),
    ];
    for (index, (contents, expected)) in cases.into_iter().enumerate() {
        let temporary = TempDir::new()?;
        let store = store(temporary.path());
        write_config(temporary.path(), contents)?;
        let Err(error) = store.load() else {
            return Err(std::io::Error::other(format!(
                "format fixture {index} was unexpectedly accepted"
            ))
            .into());
        };
        assert!(matches!(error, GlobalConfigurationError::Format { .. }));
        assert!(
            error.to_string().contains(expected),
            "case {index} did not contain {expected:?}: {error}"
        );
    }
    Ok(())
}

#[test]
fn nonregular_configuration_paths_are_rejected() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let store = store(temporary.path());
    fs::create_dir_all(store.path())?;
    assert!(matches!(
        store.load(),
        Err(GlobalConfigurationError::ExpectedFile(_))
    ));
    Ok(())
}

#[test]
fn non_directory_configuration_roots_are_rejected_without_replacement() -> Result<(), Box<dyn Error>>
{
    let temporary = TempDir::new()?;
    fs::write(temporary.path().join("config"), "collision")?;
    let store = store(temporary.path());
    assert!(matches!(
        store.compare_and_swap(&CustomConfiguration::default(), &sample_configuration()?),
        Err(GlobalConfigurationError::ExpectedDirectory(_))
    ));
    assert_eq!(
        fs::read_to_string(temporary.path().join("config"))?,
        "collision"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn symbolic_configuration_files_are_rejected_without_following_them() -> Result<(), Box<dyn Error>>
{
    use std::os::unix::fs::symlink;

    let temporary = TempDir::new()?;
    let outside = temporary.path().join("outside.toml");
    fs::write(
        &outside,
        "config-version = 1\ntaxonomies = []\ntypesets = []\ntemplates = []\n",
    )?;
    fs::create_dir(temporary.path().join("config"))?;
    symlink(&outside, temporary.path().join("config/config.toml"))?;
    let store = store(temporary.path());
    assert!(matches!(
        store.load(),
        Err(GlobalConfigurationError::Symlink(_))
    ));
    assert!(matches!(
        store.compare_and_swap(&CustomConfiguration::default(), &sample_configuration()?),
        Err(GlobalConfigurationError::Symlink(_))
    ));
    assert!(outside.exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn new_configuration_directory_requests_private_unix_permissions() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::PermissionsExt;

    let temporary = TempDir::new()?;
    let store = store(temporary.path());
    store.compare_and_swap(&CustomConfiguration::default(), &sample_configuration()?)?;
    let mode = fs::metadata(temporary.path().join("config"))?
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700);
    Ok(())
}
