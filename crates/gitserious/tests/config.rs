#![cfg(unix)]

mod support;

use std::error::Error;
use std::fs;

use support::{Home, repository, stderr, stdout};

fn write_library(home: &Home, contents: &str) -> Result<(), Box<dyn Error>> {
    let path = home.library_path();
    fs::create_dir_all(path.parent().ok_or("library path has no parent")?)?;
    fs::write(path, contents)?;
    Ok(())
}

const PLATFORM_LIBRARY: &str = "config-version = 1\n\
default-taxonomy = \"platform\"\n\
available-taxonomies = [\"conventional\", \"platform\"]\n\
\n\
[[taxonomies]]\n\
id = \"platform\"\n\
version = 1\n\
description = \"Platform team policy.\"\n\
\n\
[[taxonomies.commit-types]]\n\
id = \"deploy\"\n\
schema-version = 1\n\
description = \"Ship a platform change.\"\n\
\n\
[[taxonomies.commit-types.properties]]\n\
key = \"intent\"\n\
description = \"Why the platform must change.\"\n\
\n\
[taxonomies.commit-types.properties.requirement]\n\
level = \"required\"\n";

#[test]
fn library_taxonomy_initializes_and_commits_from_the_project_copy() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;
    write_library(&home, PLATFORM_LIBRARY)?;

    let init = home.run(repository.path(), &["init"])?;
    assert!(init.status.success(), "{}", stderr(&init));
    assert!(
        stdout(&init).ends_with(" (platform).\n"),
        "{}",
        stdout(&init)
    );
    let config = fs::read_to_string(repository.path().join("gitserious.toml"))?;
    assert!(config.contains("default-taxonomy = \"platform\""));
    assert!(config.contains("id = \"deploy\""));
    assert!(!repository.path().join("gitserious.lock").exists());

    fs::remove_file(home.library_path())?;
    let portable = home.run(repository.path(), &["commit"])?;
    assert!(!portable.status.success());
    assert_eq!(
        stderr(&portable),
        "error: commit requires an interactive terminal\n"
    );
    Ok(())
}

fn assert_domain_initialization(domain: &str, representative: &str) -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;

    write_library(
        &home,
        &format!(
            "config-version = 1\ndefault-taxonomy = \"{domain}\"\n\
             available-taxonomies = [\"{domain}\"]\ntaxonomies = []\n"
        ),
    )?;

    let initialized = home.run(repository.path(), &["init"])?;
    assert!(initialized.status.success(), "{}", stderr(&initialized));
    assert!(stdout(&initialized).ends_with(&format!(" ({domain}-v1).\n")));
    let config_path = repository.path().join("gitserious.toml");
    let config = fs::read_to_string(&config_path)?;
    assert!(config.starts_with(&format!(
        "config-version = 1\ndefault-taxonomy = \"{domain}\"\n"
    )));
    assert!(config.contains(&format!("id = \"{representative}\"")));
    assert!(!repository.path().join("gitserious.lock").exists());

    let repeated = home.run(repository.path(), &["init"])?;
    assert!(repeated.status.success(), "{}", stderr(&repeated));
    assert_eq!(fs::read_to_string(config_path)?, config);
    Ok(())
}

#[test]
fn research_library_default_initializes_with_its_types() -> Result<(), Box<dyn Error>> {
    assert_domain_initialization("research", "hypothesis")
}

#[test]
fn infra_ops_library_default_initializes_with_its_types() -> Result<(), Box<dyn Error>> {
    let first = gitserious_core::built_in_taxonomies()[2].commit_types()[0]
        .id()
        .to_string();
    assert_domain_initialization("infra-ops", &first)
}

#[test]
fn bare_config_requires_a_terminal_without_loading_or_creating_policy() -> Result<(), Box<dyn Error>>
{
    let repository = repository()?;
    let home = Home::new()?;

    let result = home.run(repository.path(), &["config"])?;

    assert!(!result.status.success());
    assert!(stderr(&result).contains("configuration editing requires an interactive terminal"));
    assert!(!home.library_path().exists());
    assert!(!repository.path().join("gitserious.toml").exists());
    assert!(!repository.path().join("gitserious.lock").exists());
    Ok(())
}
