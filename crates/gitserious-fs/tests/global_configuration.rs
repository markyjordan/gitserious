use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use gitserious_app::{GlobalConfiguration, GlobalConfigurationStore, GlobalPaths};
use gitserious_core::{TaxonomyId, built_in_taxonomies};
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

fn custom_configuration() -> Result<GlobalConfiguration, Box<dyn Error>> {
    let team = TaxonomyId::new("team")?;
    let mut available = GlobalConfiguration::default()
        .available_taxonomies()
        .to_vec();
    available.push(team.clone());
    Ok(GlobalConfiguration::new(
        1,
        team.clone(),
        available,
        vec![built_in_taxonomies()[0].fork(team)],
    )?)
}

fn write_config(root: &Path, contents: &str) -> Result<PathBuf, Box<dyn Error>> {
    let directory = root.join("config");
    fs::create_dir_all(&directory)?;
    let path = directory.join("config.toml");
    fs::write(&path, contents)?;
    Ok(path)
}

fn has_temporary_files(directory: &Path) -> Result<bool, Box<dyn Error>> {
    Ok(fs::read_dir(directory)?
        .filter_map(Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().starts_with(".tmp")))
}

#[test]
fn missing_configuration_loads_the_built_in_default_without_creating_storage()
-> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let store = store(temporary.path());
    assert_eq!(store.load()?, GlobalConfiguration::default());
    assert!(!temporary.path().join("config").exists());
    assert_eq!(store.path(), temporary.path().join("config/config.toml"));
    Ok(())
}

#[test]
fn compare_and_swap_creates_round_trips_and_replaces_complete_configuration()
-> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let store = store(temporary.path());
    let replacement = custom_configuration()?;
    store.compare_and_swap(&GlobalConfiguration::default(), &replacement)?;
    assert_eq!(store.load()?, replacement);

    let contents = fs::read_to_string(store.path())?;
    assert!(contents.starts_with("config-version = 1\n"));
    assert!(contents.contains("default-taxonomy = \"team\""));
    assert!(contents.contains("[[taxonomies]]"));
    assert!(contents.contains("id = \"team\""));
    assert!(contents.ends_with('\n'));
    assert!(!has_temporary_files(&temporary.path().join("config"))?);

    store.compare_and_swap(&replacement, &GlobalConfiguration::default())?;
    assert_eq!(store.load()?, GlobalConfiguration::default());
    Ok(())
}

#[test]
fn stale_expected_snapshots_are_rejected_without_overwrite() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let store = store(temporary.path());
    store.compare_and_swap(&GlobalConfiguration::default(), &custom_configuration()?)?;
    let before = fs::read(store.path())?;
    let result = store.compare_and_swap(
        &GlobalConfiguration::default(),
        &GlobalConfiguration::default(),
    );
    assert!(matches!(
        result,
        Err(GlobalConfigurationError::ConcurrentChange(_))
    ));
    assert_eq!(fs::read(store.path())?, before);
    assert!(!has_temporary_files(&temporary.path().join("config"))?);
    Ok(())
}

#[test]
fn strict_format_rejects_unknown_fields_unsupported_versions_and_invalid_values()
-> Result<(), Box<dyn Error>> {
    let selection = "default-taxonomy = \"conventional\"\n\
                     available-taxonomies = [\"conventional\"]\ntaxonomies = []\n";
    let cases = [
        (
            format!("config-version = 1\n{selection}unknown = true\n"),
            "unknown field",
        ),
        (format!("config-version = 2\n{selection}"), "version 2"),
        (
            "config-version = 1\ndefault-taxonomy = \"Bad\"\n\
             available-taxonomies = [\"conventional\"]\ntaxonomies = []\n"
                .to_owned(),
            "Bad",
        ),
        (
            "config-version = 1\ndefault-taxonomy = \"missing\"\n\
             available-taxonomies = [\"conventional\"]\ntaxonomies = []\n"
                .to_owned(),
            "missing",
        ),
    ];
    for (index, (contents, expected)) in cases.into_iter().enumerate() {
        let temporary = TempDir::new()?;
        let store = store(temporary.path());
        write_config(temporary.path(), &contents)?;
        let Err(error) = store.load() else {
            return Err(format!("format fixture {index} was unexpectedly accepted").into());
        };
        assert!(
            matches!(error, GlobalConfigurationError::Format { .. }),
            "case {index}: {error}"
        );
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
    let error = store
        .load()
        .err()
        .ok_or("a directory loaded as configuration")?;
    assert!(error.to_string().contains("config.toml"), "{error}");
    assert!(
        store
            .compare_and_swap(&GlobalConfiguration::default(), &custom_configuration()?)
            .is_err()
    );
    assert!(store.path().is_dir());
    Ok(())
}

#[test]
fn non_directory_configuration_roots_are_rejected_without_replacement() -> Result<(), Box<dyn Error>>
{
    let temporary = TempDir::new()?;
    fs::write(temporary.path().join("config"), "collision")?;
    let store = store(temporary.path());
    assert!(
        store
            .compare_and_swap(&GlobalConfiguration::default(), &custom_configuration()?)
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(temporary.path().join("config"))?,
        "collision"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn symbolic_configuration_files_are_never_replaced() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::symlink;

    let temporary = TempDir::new()?;
    let outside = temporary.path().join("outside.toml");
    fs::write(
        &outside,
        "config-version = 1\ndefault-taxonomy = \"conventional\"\n\
         available-taxonomies = [\"conventional\", \"research\", \"infra-ops\"]\ntaxonomies = []\n",
    )?;
    let before = fs::read(&outside)?;
    let link = temporary.path().join("config/config.toml");
    fs::create_dir(temporary.path().join("config"))?;
    symlink(&outside, &link)?;
    let store = store(temporary.path());
    assert!(matches!(
        store.compare_and_swap(&store.load()?, &custom_configuration()?),
        Err(GlobalConfigurationError::UnsafePath(_))
    ));
    assert!(fs::symlink_metadata(&link)?.file_type().is_symlink());
    assert_eq!(fs::read(&outside)?, before);
    Ok(())
}
