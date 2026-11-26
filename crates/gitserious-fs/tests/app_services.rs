mod support;

use std::cell::Cell;
use std::collections::BTreeSet;
use std::convert::Infallible;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use gitserious_app::{
    AuthoredCommit, CommitAuthoringContext, CommitServices, CommitWorkspace, ConfigurationServices,
    ConfigurationWorkspace, GitOperation, GlobalConfiguration, GlobalConfigurationStore,
    LibraryTaxonomy, ProjectConfiguration, ProjectOverview, ProjectServices, ProjectWorkspace,
    RepositoryInitializer, SubmitError, initialize_project,
};
use gitserious_core::{
    AuthoredProperty, CommitDraft, CommitSubject, CommitTypeDefinition, CommitTypeId, Description,
    PropertyRequirement, PropertyValue, SchemaVersion, Taxonomy, TaxonomyId, TaxonomyVersion,
    built_in_taxonomies,
};
use gitserious_fs::{
    GitCommitWriter, GitIndexWriter, GitRepositoryInitializer, GitRepositoryLocator,
    GitRepositoryStatusReader, TomlProjectStore,
};
use support::{git, git_command, init, repository};

struct Library(GlobalConfiguration);
impl GlobalConfigurationStore for Library {
    type Error = Infallible;
    fn path(&self) -> PathBuf {
        PathBuf::from("/config/config.toml")
    }
    fn load(&self) -> Result<GlobalConfiguration, Self::Error> {
        Ok(self.0.clone())
    }
    fn compare_and_swap(
        &self,
        _: &GlobalConfiguration,
        _: &GlobalConfiguration,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn head_exists(directory: &Path) -> Result<bool, Box<dyn Error>> {
    Ok(git_command(directory)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()?
        .status
        .success())
}

/// A fresh project adds only its policy and ignored local state beside Git and user files.
fn assert_project_shape(directory: &Path) -> Result<(), Box<dyn Error>> {
    fn entries(directory: &Path) -> Result<BTreeSet<OsString>, std::io::Error> {
        fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect()
    }
    assert_eq!(
        entries(directory)?,
        [".git", ".gitserious", "gitserious.toml"]
            .map(OsString::from)
            .into(),
        "initialization must not generate a lock file or another root artifact"
    );
    assert!(directory.join("gitserious.toml").is_file());
    assert_eq!(
        entries(&directory.join(".gitserious"))?,
        [OsString::from(".gitignore")].into(),
        "fresh local state contains only its ignore rule"
    );
    assert_eq!(
        fs::read_to_string(directory.join(".gitserious/.gitignore"))?,
        "*\n"
    );
    Ok(())
}

type GitCommitServices<'a> =
    CommitServices<'a, GitRepositoryLocator, TomlProjectStore, GitCommitWriter>;

fn exercise(commit: &GitCommitServices<'_>, directory: &Path) -> Result<(), String> {
    let context = commit.prepare_commit()?;
    let draft = CommitDraft::new(
        CommitTypeId::new("note").map_err(|error| error.to_string())?,
        None,
        CommitSubject::new("exact reviewed bytes").map_err(|error| error.to_string())?,
        Vec::new(),
    )
    .map_err(|error| error.to_string())?;
    let taxonomy = context.initial_taxonomy();
    let message = taxonomy
        .validate(&draft)
        .into_message()
        .map_err(|error| error.to_string())?;
    let other_draft = CommitDraft::new(
        draft.commit_type().clone(),
        None,
        CommitSubject::new("different bytes").map_err(|error| error.to_string())?,
        Vec::new(),
    )
    .map_err(|error| error.to_string())?;
    let mismatch = AuthoredCommit::reviewed(
        taxonomy.id().clone(),
        draft.clone(),
        taxonomy
            .validate(&other_draft)
            .into_message()
            .map_err(|error| error.to_string())?,
    );
    let authored = AuthoredCommit::reviewed(taxonomy.id().clone(), draft, message.clone());
    assert_eq!(
        commit.submit_commit(&authored).err(),
        Some(SubmitError::NothingStaged)
    );
    git(directory, &["add", "file"]).map_err(|error| error.to_string())?;
    assert!(commit.submit_commit(&mismatch).is_err());
    // A normal Git hook rejects submission; the same reviewed object can then be retried.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let hook = directory.join(".git/hooks/pre-commit");
        fs::write(&hook, "#!/bin/sh\necho test-hook-rejected >&2\nexit 1\n")
            .map_err(|error| error.to_string())?;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
        let staged_before = git(directory, &["diff", "--cached", "--binary"])
            .map_err(|error| error.to_string())?
            .stdout;
        let error = commit
            .submit_commit(&authored)
            .err()
            .ok_or("hook did not reject commit")?;
        assert!(error.to_string().contains("test-hook-rejected"));
        assert!(
            !head_exists(directory).map_err(|error| error.to_string())?,
            "a rejected first commit must leave HEAD unborn"
        );
        assert!(commit.repository_status()?.has_staged_changes());
        assert_eq!(
            git(directory, &["diff", "--cached", "--binary"])
                .map_err(|error| error.to_string())?
                .stdout,
            staged_before
        );
        assert!(
            git(directory, &["ls-files", "untracked"])
                .map_err(|error| error.to_string())?
                .stdout
                .is_empty()
        );
        fs::remove_file(hook).map_err(|error| error.to_string())?;
    }
    commit
        .submit_commit(&authored)
        .map_err(|error| error.to_string())?;
    let count =
        git(directory, &["rev-list", "--count", "HEAD"]).map_err(|error| error.to_string())?;
    assert_eq!(String::from_utf8_lossy(&count.stdout).trim(), "1");
    let stored =
        git(directory, &["log", "-1", "--format=%B"]).map_err(|error| error.to_string())?;
    // Git log adds its own terminating newline after the stored commit message.
    assert_eq!(
        stored.stdout,
        format!("{}\n", message.as_str()).into_bytes()
    );
    assert!(!commit.repository_status()?.has_staged_changes());
    assert!(
        git(directory, &["ls-files", "untracked"])
            .map_err(|error| error.to_string())?
            .stdout
            .is_empty()
    );
    Ok(())
}

#[test]
fn commit_services_gate_staging_validate_review_and_retry_git_rejection()
-> Result<(), Box<dyn Error>> {
    let temp = repository()?;
    let taxonomy = Taxonomy::new(
        TaxonomyId::new("notes")?,
        Some(TaxonomyVersion::V1),
        Description::new("Test notes")?,
        None,
        vec![CommitTypeDefinition::new(
            Some(SchemaVersion::V1),
            CommitTypeId::new("note")?,
            "Record a note",
            Vec::new(),
        )?],
    )?;
    let global = GlobalConfiguration::new(
        1,
        taxonomy.id().clone(),
        vec![taxonomy.id().clone()],
        vec![taxonomy.into()],
    )?;
    initialize_project(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &global,
        temp.path(),
    )?;
    fs::write(temp.path().join("file"), "staged later\n")?;
    fs::write(temp.path().join("untracked"), "leave me alone\n")?;
    exercise(
        &CommitServices::new(
            &GitRepositoryLocator,
            &TomlProjectStore,
            &GitRepositoryStatusReader,
            &GitCommitWriter,
            temp.path(),
        ),
        temp.path(),
    )?;
    Ok(())
}

#[test]
fn commit_services_reject_absent_and_invalid_policy() -> Result<(), Box<dyn Error>> {
    let temp = repository()?;
    fs::write(temp.path().join("file"), "staged\n")?;
    git(temp.path(), &["add", "file"])?;
    let global = GlobalConfiguration::default();
    let commit = CommitServices::new(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &GitRepositoryStatusReader,
        &GitCommitWriter,
        temp.path(),
    );
    let check = |expected: &str| -> Result<(), String> {
        let error = commit
            .prepare_commit()
            .err()
            .ok_or("expected a policy blocker")?;
        assert!(error.contains(expected), "unexpected blocker: {error}");
        Ok(())
    };
    check("not initialized")?;
    initialize_project(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &global,
        temp.path(),
    )?;
    let config_path = temp.path().join("gitserious.toml");
    let config = fs::read_to_string(&config_path)?;
    fs::write(
        &config_path,
        config.replace(
            "default-taxonomy = \"conventional\"",
            "default-taxonomy = \"missing\"",
        ),
    )?;
    check("default taxonomy \"missing\"")?;
    fs::write(
        &config_path,
        "config-version = 1\ndefault-taxonomy = \"research\"\n",
    )?;
    check("missing field `taxonomies`")?;
    assert!(!temp.path().join(".git/refs/heads/main").exists());
    Ok(())
}

fn built_ins() -> Result<ProjectConfiguration, String> {
    ProjectConfiguration::with_first_as_default(built_in_taxonomies().to_vec())
        .map_err(|error| error.to_string())
}

fn follow_home(project: &GitProjectServices<'_>, directory: &Path) -> Result<(), String> {
    let fail = |error: Box<dyn Error>| error.to_string();
    assert_eq!(
        project.overview(),
        ProjectOverview::NoRepository {
            directory: directory.to_path_buf()
        }
    );
    init(directory).map_err(fail)?;
    let ProjectOverview::Uninitialized { root } = project.overview() else {
        return Err(format!(
            "expected an uninitialized repository: {:?}",
            project.overview()
        ));
    };
    let canonical = |path: &Path| fs::canonicalize(path).map_err(|error| error.to_string());
    assert_eq!(canonical(root.as_path())?, canonical(directory)?);

    let head = fs::read(directory.join(".git/HEAD")).map_err(|error| error.to_string())?;
    project.initialize(&built_ins()?)?;
    assert_project_shape(directory).map_err(|error| error.to_string())?;
    assert_eq!(
        fs::read(directory.join(".git/HEAD")).map_err(|error| error.to_string())?,
        head,
        "an existing repository keeps its HEAD"
    );
    let written =
        fs::read_to_string(directory.join("gitserious.toml")).map_err(|error| error.to_string())?;
    assert!(directory.join(".gitserious").is_dir());
    assert_eq!(
        project.overview(),
        ProjectOverview::Initialized {
            root: root.clone(),
            default: TaxonomyId::new("conventional").map_err(|error| error.to_string())?,
            version: Some(TaxonomyVersion::V1),
            policy_untracked: true,
        }
    );
    assert!(
        project.initialize(&built_ins()?).is_err(),
        "an existing project is never rewritten"
    );
    assert_project_shape(directory).map_err(|error| error.to_string())?;
    assert_eq!(
        fs::read_to_string(directory.join("gitserious.toml")).map_err(|error| error.to_string())?,
        written
    );

    git(directory, &["add", "gitserious.toml"]).map_err(fail)?;
    assert!(matches!(
        project.overview(),
        ProjectOverview::Initialized {
            policy_untracked: false,
            ..
        }
    ));

    fs::write(directory.join("gitserious.toml"), "not = [toml")
        .map_err(|error| error.to_string())?;
    let ProjectOverview::Unavailable { error, .. } = project.overview() else {
        return Err(format!(
            "expected an unreadable project: {:?}",
            project.overview()
        ));
    };
    assert!(error.contains("gitserious.toml"), "{error}");
    Ok(())
}

#[test]
fn home_follows_a_directory_from_no_repository_to_a_tracked_project() -> Result<(), Box<dyn Error>>
{
    let temp = tempfile::TempDir::new()?;
    follow_home(
        &project_services(temp.path(), &GitRepositoryInitializer),
        temp.path(),
    )?;
    Ok(())
}

fn offered_library(config: &dyn ConfigurationWorkspace) -> Result<(), String> {
    let offered = config
        .library_taxonomies()?
        .iter()
        .map(|taxonomy| taxonomy.reference().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        offered,
        ["team-changes"],
        "a taxonomy without types is left out"
    );
    Ok(())
}

#[test]
fn initialization_offers_only_library_taxonomies_that_have_types() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::TempDir::new()?;
    let conventional = &built_in_taxonomies()[0];
    let custom = |id: &str, version, commit_types: &[CommitTypeDefinition]| {
        LibraryTaxonomy::new(
            TaxonomyId::new(id)?,
            Some(TaxonomyVersion::new(version)?),
            Description::new("A team taxonomy")?,
            None,
            commit_types.to_vec(),
            true,
        )
        .map_err(Box::<dyn Error>::from)
    };
    let library = GlobalConfiguration::new(
        1,
        conventional.id().clone(),
        vec![conventional.id().clone()],
        vec![
            custom("team-changes", 3, &conventional.commit_types()[..3])?,
            custom("unfinished", 1, &[])?,
        ],
    )?;
    let library = Library(library);
    offered_library(&ConfigurationServices::new(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &library,
        temp.path(),
    ))?;
    Ok(())
}

fn plain_directory(project: &GitProjectServices<'_>, directory: &Path) -> Result<(), String> {
    assert!(matches!(
        project.overview(),
        ProjectOverview::NoRepository { .. }
    ));
    project.initialize(&built_ins()?)?;
    assert_project_shape(directory).map_err(|error| error.to_string())?;
    assert!(
        directory.join(".git").is_dir(),
        "git init ran in the directory"
    );
    assert!(directory.join("gitserious.toml").is_file());
    assert!(matches!(
        project.overview(),
        ProjectOverview::Initialized {
            policy_untracked: true,
            ..
        }
    ));
    Ok(())
}

/// A `.git` file that names no repository is a broken repository, not a missing one, so
/// neither `git init` nor the project write runs.
fn blocked_git(project: &GitProjectServices<'_>, directory: &Path) -> Result<(), String> {
    assert!(project.initialize(&built_ins()?).is_err());
    assert!(
        !directory.join("gitserious.toml").exists(),
        "nothing is written after git fails"
    );
    Ok(())
}

type GitProjectServices<'a> = ProjectServices<'a, GitRepositoryLocator, TomlProjectStore>;

fn project_services<'a>(
    directory: &'a Path,
    initializer: &'a dyn RepositoryInitializer,
) -> GitProjectServices<'a> {
    ProjectServices::new(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &GitRepositoryStatusReader,
        initializer,
        directory,
    )
}

#[test]
fn home_initializes_git_and_gitserious_in_a_plain_directory() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::TempDir::new()?;
    plain_directory(
        &project_services(temp.path(), &GitRepositoryInitializer),
        temp.path(),
    )?;
    Ok(())
}

#[test]
fn a_failed_git_init_writes_no_project() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::TempDir::new()?;
    fs::write(temp.path().join(".git"), "not a gitdir pointer\n")?;
    blocked_git(
        &project_services(temp.path(), &GitRepositoryInitializer),
        temp.path(),
    )?;
    Ok(())
}

/// Counts `git init` runs so a retry can prove it initializes Git only once.
struct CountingInitializer(Cell<usize>);
impl RepositoryInitializer for CountingInitializer {
    fn initialize(&self, directory: &Path) -> Result<(), String> {
        self.0.set(self.0.get() + 1);
        GitRepositoryInitializer.initialize(directory)
    }
}

fn recovery(
    project: &GitProjectServices<'_>,
    directory: &Path,
    initializer: &CountingInitializer,
) -> Result<(), String> {
    let blocker = directory.join(".gitserious");
    assert!(
        project.initialize(&built_ins()?).is_err(),
        "a file named .gitserious blocks the project write"
    );
    assert!(directory.join(".git").is_dir(), "git init already ran");
    assert!(!directory.join("gitserious.toml").exists());
    assert!(matches!(
        project.overview(),
        ProjectOverview::Uninitialized { .. }
    ));

    fs::remove_file(&blocker).map_err(|error| error.to_string())?;
    project.initialize(&built_ins()?)?;
    assert_eq!(initializer.0.get(), 1, "the retry writes only the project");
    assert!(matches!(
        project.overview(),
        ProjectOverview::Initialized { .. }
    ));
    Ok(())
}

#[test]
fn a_retry_after_a_partial_initialization_writes_only_the_project() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::TempDir::new()?;
    fs::write(temp.path().join(".gitserious"), "")?;
    let initializer = CountingInitializer(Cell::new(0));
    recovery(
        &project_services(temp.path(), &initializer),
        temp.path(),
        &initializer,
    )?;
    Ok(())
}

fn repeat_initialization(project: &GitProjectServices<'_>, directory: &Path) -> Result<(), String> {
    project.initialize(&built_ins()?)?;
    let policy = directory.join("gitserious.toml");
    let before = fs::read(&policy).map_err(|error| error.to_string())?;
    assert!(project.initialize(&built_ins()?).is_err());
    assert_eq!(fs::read(policy).map_err(|error| error.to_string())?, before);
    assert!(matches!(
        project.overview(),
        ProjectOverview::Initialized { .. }
    ));
    Ok(())
}

#[test]
fn interactive_initialization_in_an_existing_repository_never_reinitializes_git()
-> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    let initializer = CountingInitializer(Cell::new(0));
    repeat_initialization(
        &project_services(repository.path(), &initializer),
        repository.path(),
    )?;
    assert_eq!(initializer.0.get(), 0);
    assert_project_shape(repository.path())?;
    Ok(())
}

#[test]
fn a_broken_repository_does_not_call_the_initializer_or_write_project_state()
-> Result<(), Box<dyn Error>> {
    let temp = tempfile::TempDir::new()?;
    fs::write(temp.path().join(".git"), "not a gitdir pointer\n")?;
    let initializer = CountingInitializer(Cell::new(0));
    blocked_git(&project_services(temp.path(), &initializer), temp.path())?;
    assert_eq!(initializer.0.get(), 0);
    assert!(!temp.path().join(".gitserious").exists());
    assert!(!temp.path().join("gitserious.toml").exists());
    Ok(())
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

const PROJECT_OWNED_CONVENTIONAL: &str = r#"config-version = 1
default-taxonomy = "conventional"

[[taxonomies]]
id = "conventional"
version = 7
description = "A project-owned conventional taxonomy."

[[taxonomies.commit-types]]
id = "ship"
schema-version = 1
description = "Ships a change."
properties = []
"#;

fn reviewed_research_fix(
    context: &CommitAuthoringContext,
) -> Result<AuthoredCommit, Box<dyn Error>> {
    let taxonomy = context
        .find_taxonomy(&TaxonomyId::new("research")?)
        .ok_or("expected research in project policy")?;
    let definition = taxonomy
        .definitions()
        .iter()
        .find(|definition| definition.id().as_str() == "fix")
        .ok_or("expected research fix")?;
    let properties = definition
        .properties()
        .iter()
        .filter(|property| property.requirement() == &PropertyRequirement::Required)
        .map(|property| {
            Ok(AuthoredProperty::new(
                property.key().clone(),
                PropertyValue::new(format!("reviewed {}", property.key()))?,
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    let draft = CommitDraft::new(
        definition.id().clone(),
        None,
        CommitSubject::new("preserve reviewed research context")?,
        properties,
    )?;
    let message = taxonomy.validate(&draft).into_message()?;
    Ok(AuthoredCommit::reviewed(
        taxonomy.id().clone(),
        draft,
        message,
    ))
}

#[test]
fn git_stores_the_exact_reviewed_taxonomy_message() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    initialize_project(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &GlobalConfiguration::default(),
        repository.path(),
    )?;
    let config = fs::read(repository.path().join("gitserious.toml"))?;
    fs::write(repository.path().join("change.txt"), "reviewed change\n")?;
    git(repository.path(), &["add", "change.txt"])?;
    let commit = CommitServices::new(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &GitRepositoryStatusReader,
        &GitCommitWriter,
        repository.path(),
    );
    let authored = reviewed_research_fix(&commit.prepare_commit()?)?;

    commit.submit_commit(&authored)?;

    let object = stdout(&git(repository.path(), &["cat-file", "commit", "HEAD"])?);
    let (_, message) = object.split_once("\n\n").ok_or("missing commit body")?;
    assert_eq!(message, authored.reviewed_message().as_str());
    assert!(message.contains("\nsymptom:\nreviewed symptom\n"));
    assert!(message.contains("Gitserious-Taxonomy: research-v1\n"));
    assert_eq!(fs::read(repository.path().join("gitserious.toml"))?, config);
    assert!(!repository.path().join("gitserious.lock").exists());
    assert_eq!(
        stdout(&git(
            repository.path(),
            &["ls-tree", "--name-only", "HEAD"]
        )?),
        "change.txt\n"
    );
    Ok(())
}

#[test]
fn project_owned_conventional_replaces_the_built_in_definition() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    fs::write(
        repository.path().join("gitserious.toml"),
        PROJECT_OWNED_CONVENTIONAL,
    )?;
    fs::write(repository.path().join("change.txt"), "staged\n")?;
    git(repository.path(), &["add", "change.txt"])?;

    let context = CommitServices::new(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &GitRepositoryStatusReader,
        &GitCommitWriter,
        repository.path(),
    )
    .prepare_commit()?;

    assert_eq!(
        context
            .initial_taxonomy()
            .definitions()
            .iter()
            .map(|definition| definition.id().as_str())
            .collect::<Vec<_>>(),
        ["ship"]
    );
    assert!(!head_exists(repository.path())?);
    Ok(())
}

#[test]
fn a_merge_in_progress_never_reaches_git_commit() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    initialize_project(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &GlobalConfiguration::default(),
        repository.path(),
    )?;
    fs::write(repository.path().join("file"), "base\n")?;
    git(repository.path(), &["add", "."])?;
    git(repository.path(), &["commit", "-q", "-m", "base"])?;
    git(repository.path(), &["checkout", "-q", "-b", "other"])?;
    fs::write(repository.path().join("file"), "other\n")?;
    git(repository.path(), &["commit", "-q", "-am", "other"])?;
    git(repository.path(), &["checkout", "-q", "-"])?;
    fs::write(repository.path().join("file"), "mine\n")?;
    git(repository.path(), &["commit", "-q", "-am", "mine"])?;
    let commit = CommitServices::new(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &GitRepositoryStatusReader,
        &GitCommitWriter,
        repository.path(),
    );
    let authored = reviewed_research_fix(&commit.prepare_commit()?)?;
    let head = stdout(&git(repository.path(), &["rev-parse", "HEAD"])?);

    assert!(git(repository.path(), &["merge", "other"]).is_err());
    fs::write(repository.path().join("file"), "resolved\n")?;
    git(repository.path(), &["add", "file"])?;

    let refusal = commit
        .prepare_commit()
        .err()
        .ok_or("authoring started during a merge")?;
    assert!(refusal.contains("`git merge --continue`"), "{refusal}");
    assert_eq!(
        commit.submit_commit(&authored).err(),
        Some(SubmitError::Operation(GitOperation::Merge))
    );
    assert_eq!(
        stdout(&git(repository.path(), &["rev-parse", "HEAD"])?),
        head
    );
    git(
        repository.path(),
        &["rev-parse", "-q", "--verify", "MERGE_HEAD"],
    )?;
    Ok(())
}

#[test]
fn a_first_commit_stages_its_files_without_git_add() -> Result<(), Box<dyn Error>> {
    let repository = repository()?;
    initialize_project(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &GlobalConfiguration::default(),
        repository.path(),
    )?;
    fs::write(repository.path().join("change.txt"), "first change\n")?;
    let commit = CommitServices::new(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &GitRepositoryStatusReader,
        &GitCommitWriter,
        repository.path(),
    )
    .staging(&GitIndexWriter);
    let authored = reviewed_research_fix(&commit.prepare_commit()?)?;
    assert_eq!(
        commit.submit_commit(&authored).err(),
        Some(SubmitError::NothingStaged)
    );

    commit.stage(&["change.txt".into()])?;
    commit.stage(&["gitserious.toml".into()])?;
    commit.submit_commit(&authored)?;

    assert_eq!(
        stdout(&git(
            repository.path(),
            &["ls-tree", "--name-only", "HEAD"]
        )?),
        "change.txt\ngitserious.toml\n"
    );
    Ok(())
}
