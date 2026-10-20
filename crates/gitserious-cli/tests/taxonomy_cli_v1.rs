use std::cell::RefCell;
use std::convert::Infallible;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gitserious_app::{
    GlobalConfiguration, GlobalConfigurationStore, ProjectConfig, ProjectLock, ProjectState,
    ProjectStateStore, RepositoryLocator, RepositoryRoot,
};
use gitserious_core::{
    CommitTypeDefinition, CommitTypeId, ConditionId, Description, PropertyCondition,
    PropertyDefinition, PropertyKey, PropertyMultiplicity, PropertyRequirement, SchemaVersion,
    Taxonomy, TaxonomyId, TaxonomyLineage, TaxonomyVersion,
};

struct Global(RefCell<GlobalConfiguration>);

impl GlobalConfigurationStore for Global {
    type Error = Infallible;
    fn load(&self) -> Result<GlobalConfiguration, Self::Error> {
        Ok(self.0.borrow().clone())
    }
    fn compare_and_swap(
        &self,
        _: &GlobalConfiguration,
        replacement: &GlobalConfiguration,
    ) -> Result<(), Self::Error> {
        *self.0.borrow_mut() = replacement.clone();
        Ok(())
    }
}

struct Locator;
impl RepositoryLocator for Locator {
    type Error = Infallible;
    fn locate(&self, start: &Path) -> Result<RepositoryRoot, Self::Error> {
        Ok(RepositoryRoot::new(start.to_path_buf()).unwrap_or_else(|_| unreachable!()))
    }
}

struct Project;
impl ProjectStateStore for Project {
    type Error = Infallible;
    fn inspect(&self, _: &RepositoryRoot) -> Result<ProjectState, Self::Error> {
        Ok(ProjectState::Absent)
    }
    fn ensure_local_state(&self, _: &RepositoryRoot) -> Result<(), Self::Error> {
        Ok(())
    }
    fn initialize(
        &self,
        _: &RepositoryRoot,
        _: &ProjectConfig,
        _: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn create_lock(&self, _: &RepositoryRoot, _: &ProjectLock) -> Result<(), Self::Error> {
        Ok(())
    }
    fn replace_lock(
        &self,
        _: &RepositoryRoot,
        _: &ProjectLock,
        _: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn compare_and_swap(
        &self,
        _: &RepositoryRoot,
        _: &ProjectConfig,
        _: &ProjectLock,
        _: &ProjectConfig,
        _: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn run(arguments: &[&str]) -> (ExitCode, String, String) {
    run_with(GlobalConfiguration::default(), arguments)
}

fn run_with(global: GlobalConfiguration, arguments: &[&str]) -> (ExitCode, String, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit = gitserious_cli::run_from(
        arguments,
        &PathBuf::from("/tmp"),
        &Locator,
        &Project,
        &Global(RefCell::new(global)),
        &mut stdout,
        &mut stderr,
    );
    (
        exit,
        String::from_utf8(stdout).unwrap_or_else(|_| unreachable!()),
        String::from_utf8(stderr).unwrap_or_else(|_| unreachable!()),
    )
}

#[test]
fn list_and_show_are_taxonomy_only() {
    let (exit, stdout, stderr) = run(&["gitserious", "config", "list"]);
    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(stderr.is_empty());
    assert!(stdout.contains("conventional\tbuilt-in\tv1 default available\n"));
    assert!(stdout.contains("research\tbuilt-in\tv1 available\n"));
    assert!(stdout.contains("infra-ops\tbuilt-in\tv1 available\n"));
    assert!(!stdout.contains("ml-research"));

    let (exit, stdout, _) = run(&["gitserious", "config", "show", "research"]);
    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(stdout.starts_with("research\n"));
    assert!(stdout.contains("  claim\trequired\tsingle\n"));
    assert!(stdout.contains("  falsifier\trecommended\tsingle\n"));
    assert!(!stdout.contains("PropertyCondition"));
}

#[test]
fn show_prints_human_requirement_and_multiplicity_labels() -> Result<(), Box<dyn Error>> {
    let (exit, stdout, stderr) =
        run_with(lab_notes()?, &["gitserious", "config", "show", "lab-notes"]);
    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(stderr.is_empty());
    assert!(stdout.contains("forked from research@1\n"));
    assert!(
        stdout.contains(
            "  notes\tconditional\tmultiple\thas-notes\tRequired when the note applies.\n"
        )
    );
    assert!(!stdout.contains("PropertyCondition"));
    assert!(!stdout.contains("Multiple"));

    let (exit, stdout, stderr) = run_with(lab_notes()?, &["gitserious", "config", "list"]);
    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(stderr.is_empty());
    assert!(stdout.contains("lab-notes\tcustom\tv1 available\n"));
    Ok(())
}

fn lab_notes() -> Result<GlobalConfiguration, Box<dyn Error>> {
    let property = PropertyDefinition::new(
        PropertyKey::new("notes")?,
        "Repeated observations.",
        PropertyRequirement::Conditional(PropertyCondition::new(
            ConditionId::new("has-notes")?,
            "Required when the note applies.",
        )?),
        PropertyMultiplicity::Multiple,
    )?;
    let commit_type = CommitTypeDefinition::new(
        SchemaVersion::V1,
        CommitTypeId::new("note")?,
        "Record an observation.",
        vec![property],
    )?;
    let taxonomy = Taxonomy::new(
        TaxonomyId::new("lab-notes")?,
        TaxonomyVersion::V1,
        Description::new("Laboratory notes.")?,
        Some(TaxonomyLineage::new(
            TaxonomyId::new("research")?,
            TaxonomyVersion::V1,
        )),
        vec![commit_type],
    )?;
    let base = GlobalConfiguration::default();
    let mut available = base.available_taxonomies().to_vec();
    available.push(taxonomy.id().clone());
    Ok(GlobalConfiguration::new(
        base.version(),
        base.default_taxonomy().clone(),
        available,
        vec![taxonomy],
    )?)
}

#[test]
fn legacy_template_flags_and_commands_are_rejected() {
    let (exit, _, stderr) = run(&["gitserious", "commit", "--template", "default"]);
    assert_ne!(exit, ExitCode::SUCCESS);
    assert!(stderr.contains("unexpected argument '--template'"));

    let (exit, _, stderr) = run(&["gitserious", "config", "fork", "anything"]);
    assert_ne!(exit, ExitCode::SUCCESS);
    assert!(stderr.contains("unrecognized subcommand 'fork'"));
}
