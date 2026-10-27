use std::convert::Infallible;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use gitserious_app::{
    AuthoredCommit, GlobalConfiguration, GlobalConfigurationStore, HelpPage, InteractiveEntry,
    InteractiveUi, InteractiveWorkspace, initialize_project, run_interactive,
};
use gitserious_core::{
    CommitDraft, CommitSubject, CommitTypeDefinition, CommitTypeId, Description, SchemaVersion,
    Taxonomy, TaxonomyId, TaxonomyVersion,
};
use gitserious_fs::{
    GitCommitWriter, GitRepositoryLocator, GitRepositoryStatusReader, TomlProjectStateStore,
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
        vec![taxonomy],
    )?;
    initialize_project(
        &GitRepositoryLocator,
        &TomlProjectStateStore,
        &global,
        None,
        temp.path(),
    )?;
    fs::write(temp.path().join("file"), "staged later\n")?;
    fs::write(temp.path().join("untracked"), "leave me alone\n")?;
    run_interactive(
        &GitRepositoryLocator,
        &TomlProjectStateStore,
        &Library(global),
        &GitRepositoryStatusReader,
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
fn shell_services_reject_absent_missing_orphan_and_stale_policy() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::TempDir::new()?;
    git(temp.path(), &["init", "-b", "main"])?;
    fs::write(temp.path().join("file"), "staged\n")?;
    git(temp.path(), &["add", "file"])?;
    let global = GlobalConfiguration::default();
    let library = Library(global.clone());
    let check = |expected: &str| {
        run_interactive(
            &GitRepositoryLocator,
            &TomlProjectStateStore,
            &library,
            &GitRepositoryStatusReader,
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
        &TomlProjectStateStore,
        &global,
        None,
        temp.path(),
    )?;
    let config_path = temp.path().join("gitserious.toml");
    let lock_path = temp.path().join("gitserious.lock");
    let config = fs::read(&config_path)?;
    let lock = fs::read(&lock_path)?;
    fs::remove_file(&lock_path)?;
    check("lock is missing")?;
    fs::write(&lock_path, lock)?;
    fs::remove_file(&config_path)?;
    check("without gitserious.toml")?;
    fs::write(
        &config_path,
        "config-version = 1\ndefault-taxonomy = \"research\"\n",
    )?;
    check("disagree")?;
    fs::write(&config_path, config)?;
    assert!(!temp.path().join(".git/refs/heads/main").exists());
    Ok(())
}
