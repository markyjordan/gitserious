use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;

use gitserious_app::{
    ProjectConfiguration, ProjectConfigurationStore, ProjectRevision, RepositoryLocator,
    RepositoryRoot,
};
use gitserious_core::{TaxonomyId, built_in_taxonomies};
use gitserious_fs::{GitRepositoryLocator, ProjectStateError, TomlProjectStore};
use tempfile::TempDir;

const CUSTOM_CONVENTIONAL: &str = r#"config-version = 1
default-taxonomy = "conventional"

[[taxonomies]]
id = "conventional"
version = 7
description = "A project-owned conventional taxonomy."

[[taxonomies.commit-types]]
id = "ship"
schema-version = 1
description = "Ships a change."

[[taxonomies.commit-types.properties]]
key = "intent"
description = "Why the change ships."

[taxonomies.commit-types.properties.requirement]
level = "required"
"#;

fn repository() -> Result<(TempDir, RepositoryRoot), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(directory.path())
        .status()?;
    if !status.success() {
        return Err(format!("git init failed with {status}").into());
    }
    let root = GitRepositoryLocator.locate(directory.path())?;
    Ok((directory, root))
}

fn config_path(root: &RepositoryRoot) -> std::path::PathBuf {
    root.as_path().join("gitserious.toml")
}

fn research_first() -> Result<ProjectConfiguration, Box<dyn Error>> {
    let mut taxonomies = built_in_taxonomies().to_vec();
    taxonomies.reverse();
    Ok(ProjectConfiguration::new(
        1,
        TaxonomyId::new("conventional")?,
        taxonomies,
    )?)
}

fn format_error(root: &RepositoryRoot, contents: &str) -> Result<String, Box<dyn Error>> {
    fs::write(config_path(root), contents)?;
    match TomlProjectStore.load(root) {
        Err(ProjectStateError::Format { message, .. }) => Ok(message),
        other => Err(format!("expected a format error, got {other:?}").into()),
    }
}

#[test]
fn absent_project_loads_as_uninitialized_even_with_local_decoys() -> Result<(), Box<dyn Error>> {
    let (_directory, root) = repository()?;
    fs::create_dir(root.as_path().join(".gitserious"))?;
    fs::write(
        root.as_path().join(".gitserious/gitserious.toml"),
        CUSTOM_CONVENTIONAL,
    )?;

    assert_eq!(TomlProjectStore.load(&root)?, None);
    Ok(())
}

#[test]
fn created_project_round_trips_with_complete_definitions_in_order() -> Result<(), Box<dyn Error>> {
    let (_directory, root) = repository()?;
    let configuration = research_first()?;

    let revision = TomlProjectStore.create(&root, &configuration)?;
    let stored = TomlProjectStore
        .load(&root)?
        .ok_or("created project did not load")?;

    assert_eq!(stored.configuration, configuration);
    assert_eq!(stored.revision, revision);
    let written = fs::read_to_string(config_path(&root))?;
    assert!(written.starts_with("config-version = 1\ndefault-taxonomy = \"conventional\"\n"));
    assert!(written.contains("[[taxonomies.commit-types.properties]]"));
    assert!(!root.as_path().join("gitserious.lock").exists());
    Ok(())
}

#[test]
fn project_owned_built_in_identity_loads_as_written() -> Result<(), Box<dyn Error>> {
    let (_directory, root) = repository()?;
    fs::write(config_path(&root), CUSTOM_CONVENTIONAL)?;

    let stored = TomlProjectStore
        .load(&root)?
        .ok_or("project did not load")?;
    let taxonomy = &stored.configuration.taxonomies()[0];

    assert_eq!(taxonomy.id().as_str(), "conventional");
    assert_eq!(taxonomy.version().get(), 7);
    assert_eq!(
        taxonomy
            .commit_types()
            .iter()
            .map(|definition| definition.id().as_str())
            .collect::<Vec<_>>(),
        ["ship"]
    );
    assert_eq!(
        stored.revision,
        ProjectRevision::of_bytes(CUSTOM_CONVENTIONAL.as_bytes())
    );
    Ok(())
}

#[test]
fn invalid_documents_name_their_defect() -> Result<(), Box<dyn Error>> {
    let (_directory, root) = repository()?;
    let duplicated = format!(
        "{CUSTOM_CONVENTIONAL}\n{}",
        &CUSTOM_CONVENTIONAL[CUSTOM_CONVENTIONAL
            .find("[[taxonomies]]")
            .ok_or("fixture has a taxonomy")?..]
    );
    let cases = [
        ("config-version = 1\n[[", "TOML parse error"),
        (
            &CUSTOM_CONVENTIONAL.replace("config-version = 1", "config-version = 2"),
            "unsupported project config version 2",
        ),
        (
            &CUSTOM_CONVENTIONAL.replace(
                "default-taxonomy = \"conventional\"",
                "default-taxonomy = \"research\"",
            ),
            "default taxonomy \"research\" must also be available",
        ),
        (&duplicated, "available taxonomies repeat \"conventional\""),
        (
            "config-version = 1\ndefault-taxonomy = \"conventional\"\ntaxonomies = []\n",
            "at least one taxonomy must be available",
        ),
        (
            "config-version = 1\ndefault-taxonomy = \"conventional\"\n",
            "missing field `taxonomies`",
        ),
        (
            &CUSTOM_CONVENTIONAL.replace(
                "default-taxonomy = \"conventional\"\n",
                "default-taxonomy = \"conventional\"\navailable-taxonomies = [\"conventional\"]\n",
            ),
            "unknown field `available-taxonomies`",
        ),
        (
            &CUSTOM_CONVENTIONAL.replace("id = \"ship\"", "id = \"Not Valid\""),
            "taxonomies[0].commit-types[0].id",
        ),
    ];
    for (contents, expected) in cases {
        let message = format_error(&root, contents)?;
        assert!(
            message.contains(expected),
            "expected {expected:?} in {message:?}"
        );
    }
    Ok(())
}

#[test]
fn create_refuses_to_overwrite_an_existing_file() -> Result<(), Box<dyn Error>> {
    let (_directory, root) = repository()?;
    fs::write(config_path(&root), "hand-written\n")?;

    let result = TomlProjectStore.create(&root, &research_first()?);

    assert!(matches!(result, Err(ProjectStateError::Collision(_))));
    assert_eq!(fs::read_to_string(config_path(&root))?, "hand-written\n");
    Ok(())
}

#[test]
fn replace_writes_only_over_the_expected_revision() -> Result<(), Box<dyn Error>> {
    let (_directory, root) = repository()?;
    fs::write(config_path(&root), CUSTOM_CONVENTIONAL)?;
    let opened = TomlProjectStore
        .load(&root)?
        .ok_or("project did not load")?;

    let revision = TomlProjectStore.replace(&root, opened.revision, &research_first()?)?;
    let stored = TomlProjectStore
        .load(&root)?
        .ok_or("project did not load")?;

    assert_eq!(stored.configuration, research_first()?);
    assert_eq!(stored.revision, revision);
    Ok(())
}

#[test]
fn replace_refuses_a_stale_revision_and_keeps_the_external_edit() -> Result<(), Box<dyn Error>> {
    let (_directory, root) = repository()?;
    fs::write(config_path(&root), CUSTOM_CONVENTIONAL)?;
    let opened = TomlProjectStore
        .load(&root)?
        .ok_or("project did not load")?;
    let external = format!("# reviewed by a maintainer\n{CUSTOM_CONVENTIONAL}");
    fs::write(config_path(&root), &external)?;

    let result = TomlProjectStore.replace(&root, opened.revision, &research_first()?);

    assert!(matches!(
        result,
        Err(ProjectStateError::ConcurrentChange(_))
    ));
    assert_eq!(fs::read_to_string(config_path(&root))?, external);
    Ok(())
}

#[test]
fn replace_refuses_when_the_file_was_removed() -> Result<(), Box<dyn Error>> {
    let (_directory, root) = repository()?;
    fs::write(config_path(&root), CUSTOM_CONVENTIONAL)?;
    let opened = TomlProjectStore
        .load(&root)?
        .ok_or("project did not load")?;
    fs::remove_file(config_path(&root))?;

    let result = TomlProjectStore.replace(&root, opened.revision, &research_first()?);

    assert!(matches!(
        result,
        Err(ProjectStateError::ConcurrentChange(_))
    ));
    assert!(!config_path(&root).exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlinked_project_file_is_refused() -> Result<(), Box<dyn Error>> {
    let (directory, root) = repository()?;
    let elsewhere = directory.path().join("elsewhere.toml");
    fs::write(&elsewhere, CUSTOM_CONVENTIONAL)?;
    std::os::unix::fs::symlink(&elsewhere, config_path(&root))?;

    assert!(matches!(
        TomlProjectStore.load(&root),
        Err(ProjectStateError::UnsafePath(_))
    ));
    assert!(Path::new(&elsewhere).is_file());
    Ok(())
}
