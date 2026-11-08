#[path = "../../gitserious-app/tests/support/library_lookup.rs"]
mod library_lookup;
use library_lookup::LibraryLookup;

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use gitserious_app::{
    GlobalConfiguration, GlobalConfigurationStore, GlobalPaths, ProjectConfigurationStore,
};
use gitserious_core::{
    Taxonomy, TaxonomyId, TaxonomyLineage, TaxonomyVersion, built_in_taxonomies,
};
use gitserious_fs::{GlobalConfigurationError, TomlGlobalConfigurationStore};
use tempfile::TempDir;

fn store(root: &Path) -> TomlGlobalConfigurationStore {
    let paths = GlobalPaths::new(root.join("config"));
    TomlGlobalConfigurationStore::new(paths.config().clone())
}

fn custom_configuration() -> Result<GlobalConfiguration, Box<dyn Error>> {
    let team = TaxonomyId::new("team")?;
    let mut available = GlobalConfiguration::default()
        .available_taxonomies()
        .to_vec();
    available.push(team.clone());
    let source = &built_in_taxonomies()[0];
    let fork = Taxonomy::new(
        team.clone(),
        Some(TaxonomyVersion::V1),
        source.description().clone(),
        Some(TaxonomyLineage::new(source.id().clone(), source.version())),
        source.commit_types().to_vec(),
    )?;
    Ok(GlobalConfiguration::new(
        1,
        team.clone(),
        available,
        vec![fork.into()],
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
fn stored_properties_have_no_multiplicity() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let store = store(temporary.path());
    store.compare_and_swap(&GlobalConfiguration::default(), &custom_configuration()?)?;
    let written = fs::read_to_string(store.path())?;
    assert!(written.contains("[[taxonomies.commit-types.properties]]"));
    assert!(!written.contains("multiplicity"), "{written}");

    let with_multiplicity = written.replacen(
        "[taxonomies.commit-types.properties.requirement]",
        "multiplicity = \"single\"\n\n[taxonomies.commit-types.properties.requirement]",
        1,
    );
    write_config(temporary.path(), &with_multiplicity)?;
    let Err(error) = store.load() else {
        return Err("a property with a multiplicity key loaded".into());
    };
    assert!(error.to_string().contains("multiplicity"), "{error}");
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

#[cfg(unix)]
#[test]
fn new_configuration_directory_requests_private_unix_permissions() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::PermissionsExt;

    let temporary = TempDir::new()?;
    let store = store(temporary.path());
    store.compare_and_swap(&GlobalConfiguration::default(), &custom_configuration()?)?;
    let mode = fs::metadata(temporary.path().join("config"))?
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700);
    Ok(())
}

fn empty_record(
    name: &str,
    locked: bool,
) -> Result<gitserious_app::LibraryTaxonomy, Box<dyn Error>> {
    Ok(gitserious_app::LibraryTaxonomy::new(
        TaxonomyId::new(name)?,
        Some(TaxonomyVersion::V1),
        gitserious_core::Description::new("A Library entry awaiting its first type.")?,
        None,
        Vec::new(),
        locked,
    )?)
}

#[test]
fn zero_types_round_trip_without_entering_the_usable_catalog() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let disk = store(temporary.path());
    let initial = GlobalConfiguration::default();
    let replacement = GlobalConfiguration::new(
        1,
        initial.default_taxonomy().clone(),
        initial.available_taxonomies().to_vec(),
        vec![empty_record("unfinished", false)?],
    )?;
    disk.compare_and_swap(&initial, &replacement)?;
    let reopened = store(temporary.path()).load()?;
    assert_eq!(reopened, replacement);
    assert!(
        reopened
            .catalog()?
            .find(&TaxonomyId::new("unfinished")?)
            .is_none()
    );
    assert_eq!(reopened.catalog()?.taxonomies().len(), 3);
    let contents = fs::read_to_string(disk.path())?;
    assert!(contents.contains("name-locked = false"));
    assert!(contents.contains("commit-types = []"));
    assert!(!has_temporary_files(&temporary.path().join("config"))?);
    assert_eq!(fs::read_dir(temporary.path().join("config"))?.count(), 1);
    Ok(())
}

#[test]
fn absent_name_metadata_defaults_to_locked_even_without_types() -> Result<(), Box<dyn Error>> {
    let temporary = TempDir::new()?;
    let disk = store(temporary.path());
    write_config(
        temporary.path(),
        "config-version = 1\ndefault-taxonomy = \"conventional\"\navailable-taxonomies = [\"conventional\"]\n[[taxonomies]]\nid = \"unfinished\"\nversion = 1\ndescription = \"Existing entry\"\ncommit-types = []\n",
    )?;
    let global = disk.load()?;
    assert!(global.custom_taxonomies()[0].name_locked());
    assert!(global.custom_taxonomies()[0].commit_types().is_empty());
    disk.compare_and_swap(&global, &global)?;
    assert!(!fs::read_to_string(disk.path())?.contains("name-locked"));
    Ok(())
}

#[test]
fn eligibility_changes_keep_selection_and_durable_name_locking() -> Result<(), Box<dyn Error>> {
    use gitserious_app::{DraftEdit, DraftField, LibraryDraft};
    let temporary = TempDir::new()?;
    let disk = store(temporary.path());
    let initial = custom_configuration()?;
    disk.compare_and_swap(&GlobalConfiguration::default(), &initial)?;
    let original = &initial.custom_taxonomies()[0];
    let empty = gitserious_app::LibraryTaxonomy::new(
        original.id().clone(),
        Some(TaxonomyVersion::new(2)?),
        original.description().clone(),
        original.derived_from().cloned(),
        Vec::new(),
        original.name_locked(),
    )?;
    let replacement = GlobalConfiguration::new(
        1,
        initial.default_taxonomy().clone(),
        initial.available_taxonomies().to_vec(),
        vec![empty],
    )?;
    disk.compare_and_swap(&initial, &replacement)?;
    let reopened = disk.load()?;
    assert_eq!(reopened.default_taxonomy(), initial.default_taxonomy());
    assert_eq!(
        reopened.available_taxonomies(),
        initial.available_taxonomies()
    );
    assert!(reopened.custom_taxonomies()[0].name_locked());
    assert!(reopened.catalog()?.find(original.id()).is_none());
    let mut draft = LibraryDraft::from_configuration(reopened);
    let node = draft.find_path(&["team"]).ok_or("missing empty entry")?;
    assert_eq!(
        draft.edit(&node, DraftEdit::Text(DraftField::Name, "renamed")),
        Err(gitserious_app::EditRefusal::NameLocked)
    );
    assert!(draft.build().is_ok());
    Ok(())
}

#[test]
fn library_dates_round_trip_and_invalid_calendar_dates_are_rejected() -> Result<(), Box<dyn Error>>
{
    let temporary = TempDir::new()?;
    let store = store(temporary.path());
    let configuration = custom_configuration()?;
    let record = configuration.custom_taxonomies()[0]
        .clone()
        .with_dates(Some("2024-02-29".parse()?), Some("2026-11-07".parse()?));
    let configuration = GlobalConfiguration::new(
        1,
        configuration.default_taxonomy().clone(),
        configuration.available_taxonomies().to_vec(),
        vec![record],
    )?;
    store.compare_and_swap(&GlobalConfiguration::default(), &configuration)?;
    assert_eq!(store.load()?, configuration);
    let bytes = fs::read_to_string(store.path())?;
    assert!(bytes.contains("created-on = \"2024-02-29\""));
    assert!(bytes.contains("modified-on = \"2026-11-07\""));
    let root = gitserious_app::RepositoryRoot::new(temporary.path().join("project"))?;
    fs::create_dir(root.as_path())?;
    let project = gitserious_app::ProjectConfiguration::new(
        1,
        configuration.default_taxonomy().clone(),
        vec![
            configuration.custom_taxonomies()[0]
                .usable()
                .ok_or("usable")?,
        ],
    )?;
    let project_store = gitserious_fs::TomlProjectStore;
    project_store.create(&root, &project)?;
    let project_path = root.as_path().join("gitserious.toml");
    let project_bytes = fs::read_to_string(&project_path)?;
    assert!(!project_bytes.contains("created-on"));
    fs::write(
        &project_path,
        project_bytes.replace(
            "[[taxonomies]]",
            "[[taxonomies]]\ncreated-on = \"2024-02-29\"",
        ),
    )?;
    assert!(project_store.load(&root).is_err());
    fs::write(store.path(), bytes.replace("2024-02-29", "2025-02-29"))?;
    assert!(store.load().is_err());
    Ok(())
}
