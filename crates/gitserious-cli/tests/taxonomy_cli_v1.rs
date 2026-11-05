use std::cell::RefCell;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gitserious_app::{
    GlobalConfiguration, GlobalConfigurationStore, ProjectConfiguration, ProjectConfigurationStore,
    ProjectRevision, RepositoryLocator, RepositoryRoot, StoredProject,
};

struct Global(RefCell<GlobalConfiguration>);

impl GlobalConfigurationStore for Global {
    type Error = Infallible;
    fn path(&self) -> PathBuf {
        PathBuf::from("/config/config.toml")
    }
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

    fn is_not_repository(_: &Self::Error) -> bool {
        false
    }
    fn locate(&self, start: &Path) -> Result<RepositoryRoot, Self::Error> {
        Ok(RepositoryRoot::new(start.to_path_buf()).unwrap_or_else(|_| unreachable!()))
    }
}

struct Project;
impl ProjectConfigurationStore for Project {
    type Error = Infallible;

    fn load(&self, _: &RepositoryRoot) -> Result<Option<StoredProject>, Self::Error> {
        Ok(None)
    }
    fn create(
        &self,
        _: &RepositoryRoot,
        configuration: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error> {
        let _ = configuration;
        Ok(ProjectRevision::of_bytes(b"created"))
    }
    fn replace(
        &self,
        _: &RepositoryRoot,
        _: ProjectRevision,
        _: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error> {
        unreachable!("rejected arguments never write")
    }
    fn ensure_local_state(&self, _: &RepositoryRoot) -> Result<(), Self::Error> {
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
fn legacy_template_flags_and_commands_are_rejected() {
    let (exit, _, stderr) = run(&["gitserious", "commit", "--template", "default"]);
    assert_ne!(exit, ExitCode::SUCCESS);
    assert!(stderr.contains("unexpected argument '--template'"));

    for subcommand in ["fork", "list", "show", "verify"] {
        let (exit, _, stderr) = run(&["gitserious", "config", subcommand]);
        assert_eq!(exit, ExitCode::from(2));
        assert!(
            stderr.contains(&format!("unexpected argument '{subcommand}'")),
            "{stderr}"
        );
    }
}
