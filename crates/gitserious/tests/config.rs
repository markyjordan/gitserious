#![cfg(unix)]

mod support;

use std::error::Error;
use std::fs;

use gitserious_core::ConditionId;
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
multiplicity = \"single\"\n\
\n\
[taxonomies.commit-types.properties.requirement]\n\
level = \"required\"\n";

#[test]
fn show_prints_built_in_properties_in_lock_words() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;

    for taxonomy in ["conventional", "research", "infra-ops"] {
        let output = home.run(repository.path(), &["config", "show", taxonomy])?;
        assert!(output.status.success(), "{}", stderr(&output));
        let shown = stdout(&output);
        let properties = shown
            .lines()
            .filter_map(|line| line.strip_prefix("  "))
            .collect::<Vec<_>>();
        assert!(!properties.is_empty(), "{shown}");
        for property in properties {
            let columns = property.split('\t').collect::<Vec<_>>();
            assert_eq!(columns.len(), 3, "{taxonomy}: {property:?}");
            let (requirement, multiplicity) = (columns[1], columns[2]);
            let condition = requirement.strip_prefix("conditional:");
            assert!(
                matches!(requirement, "required" | "recommended" | "optional")
                    || condition.is_some_and(|id| ConditionId::new(id).is_ok()),
                "{taxonomy}: {property:?}"
            );
            assert!(
                matches!(multiplicity, "single" | "multiple"),
                "{taxonomy}: {property:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn library_taxonomy_initializes_and_commits_from_the_portable_lock() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;
    write_library(&home, PLATFORM_LIBRARY)?;

    let listed = home.run(repository.path(), &["config", "list"])?;
    assert!(listed.status.success(), "{}", stderr(&listed));
    assert!(stdout(&listed).ends_with("platform\tcustom\tv1 default available\n"));

    let init = home.run(repository.path(), &["init"])?;
    assert!(init.status.success(), "{}", stderr(&init));
    assert!(
        stdout(&init).ends_with(" (platform@1).\n"),
        "{}",
        stdout(&init)
    );
    assert_eq!(
        fs::read_to_string(repository.path().join("gitserious.toml"))?,
        "config-version = 1\n"
    );
    let lock = fs::read_to_string(repository.path().join("gitserious.lock"))?;
    assert!(lock.contains("default-taxonomy = \"platform\""));
    assert!(lock.contains("id = \"deploy\""));

    let unknown = home.run(repository.path(), &["commit", "--type", "feat"])?;
    assert!(!unknown.status.success());
    assert!(
        stderr(&unknown).contains("choose one of: deploy"),
        "{}",
        stderr(&unknown)
    );

    fs::remove_file(home.library_path())?;
    let portable = home.run(repository.path(), &["commit", "--type", "deploy"])?;
    assert!(!portable.status.success());
    assert_eq!(
        stderr(&portable),
        "error: commit authoring requires an interactive terminal\n"
    );
    Ok(())
}

fn assert_domain_initialization(domain: &str, representative: &str) -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let home = Home::new()?;

    let initialized = home.run(repository.path(), &["init", "--taxonomy", domain])?;
    assert!(initialized.status.success(), "{}", stderr(&initialized));
    assert!(stdout(&initialized).ends_with(&format!(" ({domain}@1).\n")));
    assert!(!home.library_path().exists());
    let config_path = repository.path().join("gitserious.toml");
    let lock_path = repository.path().join("gitserious.lock");
    let config = fs::read_to_string(&config_path)?;
    let lock = fs::read_to_string(&lock_path)?;
    assert_eq!(
        config,
        format!("config-version = 1\ndefault-taxonomy = \"{domain}\"\n")
    );
    assert!(lock.contains(&format!("default-taxonomy = \"{domain}\"")));
    assert!(lock.contains(&format!("id = \"{representative}\"")));

    let unavailable = home.run(repository.path(), &["commit", "--type", "feat"])?;
    assert!(!unavailable.status.success());
    assert!(
        stderr(&unavailable).contains("choose one of: "),
        "{}",
        stderr(&unavailable)
    );
    let selected = home.run(repository.path(), &["commit", "--type", representative])?;
    assert!(!selected.status.success());
    assert!(
        stderr(&selected).contains("interactive terminal"),
        "{}",
        stderr(&selected)
    );

    let repeated = home.run(repository.path(), &["init"])?;
    assert!(repeated.status.success(), "{}", stderr(&repeated));
    assert_eq!(fs::read_to_string(config_path)?, config);
    assert_eq!(fs::read_to_string(lock_path)?, lock);
    Ok(())
}

#[test]
fn research_initializes_without_global_configuration_and_uses_its_types()
-> Result<(), Box<dyn Error>> {
    assert_domain_initialization("research", "hypothesis")
}

#[test]
fn infra_ops_initializes_without_global_configuration_and_uses_its_types()
-> Result<(), Box<dyn Error>> {
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
