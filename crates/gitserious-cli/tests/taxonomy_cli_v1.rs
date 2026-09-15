use std::cell::RefCell;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gitserious_app::{
    GlobalConfiguration, GlobalConfigurationStore, ProjectConfig, ProjectLock, ProjectState,
    ProjectStateStore, RepositoryLocator, RepositoryRoot,
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
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit = gitserious_cli::run_from(
        arguments,
        &PathBuf::from("/tmp"),
        &Locator,
        &Project,
        &Global(RefCell::new(GlobalConfiguration::default())),
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
    assert!(stdout.contains("conventional"));
    assert!(stdout.contains("research"));
    assert!(!stdout.contains("ml-research"));

    let (exit, stdout, _) = run(&["gitserious", "config", "show", "research"]);
    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(stdout.starts_with("research\n"));
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
