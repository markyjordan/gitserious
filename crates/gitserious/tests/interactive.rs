use std::cell::Cell;
use std::convert::Infallible;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use gitserious_app::{
    AuthoredCommit, GlobalConfiguration, GlobalConfigurationStore, HelpPage, HomeState,
    InteractiveEntry, InteractiveUi, InteractiveWorkspace, LibraryTaxonomy, ProjectConfiguration,
    RepositoryInitializer, initialize_project, run_interactive,
};
use gitserious_core::{
    CommitDraft, CommitSubject, CommitTypeDefinition, CommitTypeId, Description, SchemaVersion,
    Taxonomy, TaxonomyId, TaxonomyVersion, built_in_taxonomies,
};
use gitserious_fs::{
    GitCommitWriter, GitRepositoryInitializer, GitRepositoryLocator, GitRepositoryStatusReader,
    TomlProjectStore,
};

struct Library(GlobalConfiguration);
impl GlobalConfigurationStore for Library {
    type Error = Infallible;
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

fn git(directory: &Path, arguments: &[&str]) -> Result<Output, Box<dyn Error>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(output)
}

struct ExerciseUi<'a>(&'a Path);
impl InteractiveUi for ExerciseUi<'_> {
    fn run(
        &self,
        entry: InteractiveEntry,
        workspace: &dyn InteractiveWorkspace,
        _: &[HelpPage],
    ) -> Result<(), String> {
        assert_eq!(entry, InteractiveEntry::Home);
        assert!(workspace.prepare_commit().is_err());
        git(self.0, &["add", "file"]).map_err(|error| error.to_string())?;
        let context = workspace.prepare_commit()?;
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
        let unreviewed = AuthoredCommit::new(taxonomy.id().clone(), draft.clone());
        assert!(workspace.submit_commit(&unreviewed).is_err());
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
        assert!(workspace.submit_commit(&mismatch).is_err());
        let authored = AuthoredCommit::reviewed(taxonomy.id().clone(), draft, message.clone());
        // A normal Git hook rejects submission; the same reviewed object can then be retried.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let hook = self.0.join(".git/hooks/pre-commit");
            fs::write(&hook, "#!/bin/sh\necho test-hook-rejected >&2\nexit 1\n")
                .map_err(|error| error.to_string())?;
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))
                .map_err(|error| error.to_string())?;
            let error = workspace
                .submit_commit(&authored)
                .err()
                .ok_or("hook did not reject commit")?;
            assert!(error.contains("test-hook-rejected"));
            fs::remove_file(hook).map_err(|error| error.to_string())?;
        }
        workspace.submit_commit(&authored)?;
        let stored =
            git(self.0, &["log", "-1", "--format=%B"]).map_err(|error| error.to_string())?;
        // Git log adds its own terminating newline after the stored commit message.
        assert_eq!(
            stored.stdout,
            format!("{}\n", message.as_str()).into_bytes()
        );
        assert!(!workspace.repository_status()?.has_staged_changes());
        assert!(
            git(self.0, &["ls-files", "untracked"])
                .map_err(|error| error.to_string())?
                .stdout
                .is_empty()
        );
        Ok(())
    }
}

#[test]
fn shell_services_gate_staging_validate_review_and_retry_git_rejection()
-> Result<(), Box<dyn Error>> {
    let temp = tempfile::TempDir::new()?;
    git(temp.path(), &["init", "-b", "main"])?;
    git(temp.path(), &["config", "user.name", "Interactive Test"])?;
    git(
        temp.path(),
        &["config", "user.email", "interactive@example.invalid"],
    )?;
    git(temp.path(), &["config", "commit.gpgsign", "false"])?;
    let taxonomy = Taxonomy::new(
        TaxonomyId::new("notes")?,
        TaxonomyVersion::V1,
        Description::new("Test notes")?,
        None,
        vec![CommitTypeDefinition::new(
            SchemaVersion::V1,
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
        None,
        temp.path(),
    )?;
    fs::write(temp.path().join("file"), "staged later\n")?;
    fs::write(temp.path().join("untracked"), "leave me alone\n")?;
    run_interactive(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &Library(global),
        &GitRepositoryStatusReader,
        &GitRepositoryInitializer,
        &GitCommitWriter,
        temp.path(),
        &ExerciseUi(temp.path()),
        InteractiveEntry::Home,
        &[],
    )?;
    Ok(())
}

struct BlockedUi<'a>(&'a str);
impl InteractiveUi for BlockedUi<'_> {
    fn run(
        &self,
        _: InteractiveEntry,
        workspace: &dyn InteractiveWorkspace,
        _: &[HelpPage],
    ) -> Result<(), String> {
        let error = workspace
            .prepare_commit()
            .err()
            .ok_or("expected a policy blocker")?;
        assert!(error.contains(self.0), "unexpected blocker: {error}");
        Ok(())
    }
}

#[test]
fn shell_services_reject_absent_and_invalid_policy() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::TempDir::new()?;
    git(temp.path(), &["init", "-b", "main"])?;
    fs::write(temp.path().join("file"), "staged\n")?;
    git(temp.path(), &["add", "file"])?;
    let global = GlobalConfiguration::default();
    let library = Library(global.clone());
    let check = |expected: &str| {
        run_interactive(
            &GitRepositoryLocator,
            &TomlProjectStore,
            &library,
            &GitRepositoryStatusReader,
            &GitRepositoryInitializer,
            &GitCommitWriter,
            temp.path(),
            &BlockedUi(expected),
            InteractiveEntry::Home,
            &[],
        )
    };
    check("not initialized")?;
    initialize_project(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &global,
        None,
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

struct HomeUi<'a>(&'a Path);
impl InteractiveUi for HomeUi<'_> {
    fn run(
        &self,
        _: InteractiveEntry,
        workspace: &dyn InteractiveWorkspace,
        _: &[HelpPage],
    ) -> Result<(), String> {
        let fail = |error: Box<dyn Error>| error.to_string();
        assert_eq!(
            workspace.home(),
            HomeState::NoRepository {
                directory: self.0.to_path_buf()
            }
        );
        git(self.0, &["init", "-b", "main"]).map_err(fail)?;
        let HomeState::Uninitialized { root } = workspace.home() else {
            return Err(format!(
                "expected an uninitialized repository: {:?}",
                workspace.home()
            ));
        };
        let canonical = |path: &Path| fs::canonicalize(path).map_err(|error| error.to_string());
        assert_eq!(canonical(root.as_path())?, canonical(self.0)?);

        let head = fs::read(self.0.join(".git/HEAD")).map_err(|error| error.to_string())?;
        workspace.initialize(&built_ins()?)?;
        assert_eq!(
            fs::read(self.0.join(".git/HEAD")).map_err(|error| error.to_string())?,
            head,
            "an existing repository keeps its HEAD"
        );
        let written = fs::read_to_string(self.0.join("gitserious.toml"))
            .map_err(|error| error.to_string())?;
        assert!(self.0.join(".gitserious").is_dir());
        assert_eq!(
            workspace.home(),
            HomeState::Initialized {
                root: root.clone(),
                default: TaxonomyId::new("conventional").map_err(|error| error.to_string())?,
                version: TaxonomyVersion::V1,
                policy_untracked: true,
            }
        );
        assert!(
            workspace.initialize(&built_ins()?).is_err(),
            "an existing project is never rewritten"
        );
        assert_eq!(
            fs::read_to_string(self.0.join("gitserious.toml")).map_err(|error| error.to_string())?,
            written
        );

        git(self.0, &["add", "gitserious.toml"]).map_err(fail)?;
        assert!(matches!(
            workspace.home(),
            HomeState::Initialized {
                policy_untracked: false,
                ..
            }
        ));

        fs::write(self.0.join("gitserious.toml"), "not = [toml")
            .map_err(|error| error.to_string())?;
        let HomeState::Unavailable { error, .. } = workspace.home() else {
            return Err(format!(
                "expected an unreadable project: {:?}",
                workspace.home()
            ));
        };
        assert!(error.contains("gitserious.toml"), "{error}");
        Ok(())
    }
}

#[test]
fn home_follows_a_directory_from_no_repository_to_a_tracked_project() -> Result<(), Box<dyn Error>>
{
    let temp = tempfile::TempDir::new()?;
    run_interactive(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &Library(GlobalConfiguration::default()),
        &GitRepositoryStatusReader,
        &GitRepositoryInitializer,
        &GitCommitWriter,
        temp.path(),
        &HomeUi(temp.path()),
        InteractiveEntry::Home,
        &[],
    )?;
    Ok(())
}

struct LibraryUi;
impl InteractiveUi for LibraryUi {
    fn run(
        &self,
        _: InteractiveEntry,
        workspace: &dyn InteractiveWorkspace,
        _: &[HelpPage],
    ) -> Result<(), String> {
        let offered = workspace
            .library_taxonomies()?
            .iter()
            .map(|taxonomy| taxonomy.versioned().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            offered,
            ["team-changes-v3"],
            "a taxonomy without types is left out"
        );
        Ok(())
    }
}

#[test]
fn initialization_offers_only_library_taxonomies_that_have_types() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::TempDir::new()?;
    let conventional = &built_in_taxonomies()[0];
    let custom = |id: &str, version, commit_types: &[CommitTypeDefinition]| {
        LibraryTaxonomy::new(
            TaxonomyId::new(id)?,
            TaxonomyVersion::new(version)?,
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
    run_interactive(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &Library(library),
        &GitRepositoryStatusReader,
        &GitRepositoryInitializer,
        &GitCommitWriter,
        temp.path(),
        &LibraryUi,
        InteractiveEntry::Home,
        &[],
    )?;
    Ok(())
}

struct PlainDirectoryUi<'a>(&'a Path);
impl InteractiveUi for PlainDirectoryUi<'_> {
    fn run(
        &self,
        _: InteractiveEntry,
        workspace: &dyn InteractiveWorkspace,
        _: &[HelpPage],
    ) -> Result<(), String> {
        assert!(matches!(workspace.home(), HomeState::NoRepository { .. }));
        workspace.initialize(&built_ins()?)?;
        assert!(
            self.0.join(".git").is_dir(),
            "git init ran in the directory"
        );
        assert!(self.0.join("gitserious.toml").is_file());
        assert!(matches!(
            workspace.home(),
            HomeState::Initialized {
                policy_untracked: true,
                ..
            }
        ));
        Ok(())
    }
}

/// A `.git` file that names no repository is a broken repository, not a missing one, so
/// neither `git init` nor the project write runs.
struct BlockedGitUi<'a>(&'a Path);
impl InteractiveUi for BlockedGitUi<'_> {
    fn run(
        &self,
        _: InteractiveEntry,
        workspace: &dyn InteractiveWorkspace,
        _: &[HelpPage],
    ) -> Result<(), String> {
        assert!(workspace.initialize(&built_ins()?).is_err());
        assert!(
            !self.0.join("gitserious.toml").exists(),
            "nothing is written after git fails"
        );
        Ok(())
    }
}

fn run_home(directory: &Path, ui: &dyn InteractiveUi) -> Result<(), Box<dyn Error>> {
    run_home_with(directory, ui, &GitRepositoryInitializer)
}

fn run_home_with(
    directory: &Path,
    ui: &dyn InteractiveUi,
    initializer: &dyn RepositoryInitializer,
) -> Result<(), Box<dyn Error>> {
    run_interactive(
        &GitRepositoryLocator,
        &TomlProjectStore,
        &Library(GlobalConfiguration::default()),
        &GitRepositoryStatusReader,
        initializer,
        &GitCommitWriter,
        directory,
        ui,
        InteractiveEntry::Home,
        &[],
    )?;
    Ok(())
}

#[test]
fn home_initializes_git_and_gitserious_in_a_plain_directory() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::TempDir::new()?;
    run_home(temp.path(), &PlainDirectoryUi(temp.path()))
}

#[test]
fn a_failed_git_init_writes_no_project() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::TempDir::new()?;
    fs::write(temp.path().join(".git"), "not a gitdir pointer\n")?;
    run_home(temp.path(), &BlockedGitUi(temp.path()))
}

/// Counts `git init` runs so a retry can prove it initializes Git only once.
struct CountingInitializer(Cell<usize>);
impl RepositoryInitializer for CountingInitializer {
    fn initialize(&self, directory: &Path) -> Result<(), String> {
        self.0.set(self.0.get() + 1);
        GitRepositoryInitializer.initialize(directory)
    }
}

struct RecoveryUi<'a>(&'a Path, &'a CountingInitializer);
impl InteractiveUi for RecoveryUi<'_> {
    fn run(
        &self,
        _: InteractiveEntry,
        workspace: &dyn InteractiveWorkspace,
        _: &[HelpPage],
    ) -> Result<(), String> {
        let blocker = self.0.join(".gitserious");
        assert!(
            workspace.initialize(&built_ins()?).is_err(),
            "a file named .gitserious blocks the project write"
        );
        assert!(self.0.join(".git").is_dir(), "git init already ran");
        assert!(!self.0.join("gitserious.toml").exists());
        assert!(matches!(workspace.home(), HomeState::Uninitialized { .. }));

        fs::remove_file(&blocker).map_err(|error| error.to_string())?;
        workspace.initialize(&built_ins()?)?;
        assert_eq!(self.1.0.get(), 1, "the retry writes only the project");
        assert!(matches!(workspace.home(), HomeState::Initialized { .. }));
        Ok(())
    }
}

#[test]
fn a_retry_after_a_partial_initialization_writes_only_the_project() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::TempDir::new()?;
    fs::write(temp.path().join(".gitserious"), "")?;
    let initializer = CountingInitializer(Cell::new(0));
    run_home_with(
        temp.path(),
        &RecoveryUi(temp.path(), &initializer),
        &initializer,
    )
}
