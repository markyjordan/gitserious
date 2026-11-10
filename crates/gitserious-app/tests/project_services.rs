use std::cell::{Cell, RefCell};
use std::error::Error;
use std::path::{Path, PathBuf};

use gitserious_app::{
    BranchState, ProjectConfiguration, ProjectConfigurationStore, ProjectOverview, ProjectRevision,
    ProjectServices, ProjectWorkspace, RepositoryInitializer, RepositoryLocator, RepositoryRoot,
    RepositoryStatus, RepositoryStatusReader, StatusEntry, StoredProject,
};
use gitserious_core::{TaxonomyId, TaxonomyVersion, built_in_taxonomies};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Repository {
    Present,
    Missing,
    Broken,
}

struct Locator {
    root: RepositoryRoot,
    repository: Cell<Repository>,
}

impl RepositoryLocator for Locator {
    type Error = &'static str;

    fn locate(&self, _: &Path) -> Result<RepositoryRoot, Self::Error> {
        match self.repository.get() {
            Repository::Present => Ok(self.root.clone()),
            Repository::Missing => Err("not a git repository"),
            Repository::Broken => Err("git failed"),
        }
    }

    fn is_not_repository(error: &Self::Error) -> bool {
        *error == "not a git repository"
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum StoreCall {
    EnsureLocalState,
    Create,
}

struct Store {
    project: RefCell<Result<Option<ProjectConfiguration>, &'static str>>,
    local_state: Result<(), &'static str>,
    calls: RefCell<Vec<StoreCall>>,
}

impl ProjectConfigurationStore for Store {
    type Error = &'static str;

    fn load(&self, _: &RepositoryRoot) -> Result<Option<StoredProject>, Self::Error> {
        self.project.borrow().clone().map(|project| {
            project.map(|configuration| StoredProject {
                configuration,
                revision: ProjectRevision::of_bytes(b"stored"),
            })
        })
    }

    fn create(
        &self,
        _: &RepositoryRoot,
        configuration: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error> {
        self.calls.borrow_mut().push(StoreCall::Create);
        let mut project = self.project.borrow_mut();
        if matches!(*project, Ok(Some(_))) {
            return Err("gitserious.toml already exists");
        }
        *project = Ok(Some(configuration.clone()));
        Ok(ProjectRevision::of_bytes(b"created"))
    }

    fn replace(
        &self,
        _: &RepositoryRoot,
        _: ProjectRevision,
        _: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error> {
        Err("unexpected project replacement")
    }

    fn ensure_local_state(&self, _: &RepositoryRoot) -> Result<(), Self::Error> {
        self.calls.borrow_mut().push(StoreCall::EnsureLocalState);
        self.local_state
    }
}

struct Status(Result<Vec<StatusEntry>, String>);

impl RepositoryStatusReader for Status {
    fn read(&self, root: &RepositoryRoot) -> Result<RepositoryStatus, String> {
        self.0.clone().map(|entries| RepositoryStatus {
            root: root.clone(),
            branch: BranchState::Named("main".into()),
            entries,
        })
    }
}

/// Creates the repository the locator then finds, unless told to fail.
struct Initializer<'a> {
    repository: &'a Cell<Repository>,
    failure: Option<&'static str>,
    calls: Cell<usize>,
}

impl RepositoryInitializer for Initializer<'_> {
    fn initialize(&self, _: &Path) -> Result<(), String> {
        self.calls.set(self.calls.get() + 1);
        if let Some(failure) = self.failure {
            return Err(failure.into());
        }
        self.repository.set(Repository::Present);
        Ok(())
    }
}

struct Session {
    locator: Locator,
    store: Store,
    status: Status,
}

impl Session {
    fn new(
        repository: Repository,
        project: Result<Option<ProjectConfiguration>, &'static str>,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            locator: Locator {
                root: RepositoryRoot::new(start())?,
                repository: Cell::new(repository),
            },
            store: Store {
                project: RefCell::new(project),
                local_state: Ok(()),
                calls: RefCell::new(Vec::new()),
            },
            status: Status(Ok(Vec::new())),
        })
    }

    fn initializer(&self, failure: Option<&'static str>) -> Initializer<'_> {
        Initializer {
            repository: &self.locator.repository,
            failure,
            calls: Cell::new(0),
        }
    }

    fn project<'a>(
        &'a self,
        initializer: &'a Initializer<'a>,
    ) -> ProjectServices<'a, Locator, Store> {
        ProjectServices::new(
            &self.locator,
            &self.store,
            &self.status,
            initializer,
            self.locator.root.as_path(),
        )
    }

    fn overview(&self) -> ProjectOverview {
        let initializer = self.initializer(Some("unexpected Git initialization"));
        self.project(&initializer).overview()
    }

    fn calls(&self) -> Vec<StoreCall> {
        self.store.calls.borrow().clone()
    }
}

fn start() -> PathBuf {
    std::env::temp_dir().join("gitserious-project-services")
}

fn built_ins() -> Result<ProjectConfiguration, Box<dyn Error>> {
    Ok(ProjectConfiguration::new(
        1,
        TaxonomyId::new("conventional")?,
        built_in_taxonomies().to_vec(),
    )?)
}

fn entry(path: &str, untracked: bool) -> StatusEntry {
    StatusEntry {
        path: path.into(),
        original_path: None,
        index: if untracked { '?' } else { 'M' },
        worktree: if untracked { '?' } else { '.' },
        untracked,
        conflicted: false,
        submodule: "N...".into(),
    }
}

fn policy_untracked(entries: Vec<StatusEntry>) -> Result<bool, Box<dyn Error>> {
    let mut session = Session::new(Repository::Present, Ok(Some(built_ins()?)))?;
    session.status = Status(Ok(entries));
    match session.overview() {
        ProjectOverview::Initialized {
            policy_untracked, ..
        } => Ok(policy_untracked),
        other => Err(format!("expected an initialized project: {other:?}").into()),
    }
}

#[test]
fn a_directory_outside_git_has_no_repository() -> Result<(), Box<dyn Error>> {
    let session = Session::new(Repository::Missing, Ok(None))?;
    assert_eq!(
        session.overview(),
        ProjectOverview::NoRepository { directory: start() }
    );
    Ok(())
}

#[test]
fn a_broken_repository_is_unavailable_with_its_error() -> Result<(), Box<dyn Error>> {
    let session = Session::new(Repository::Broken, Ok(None))?;
    assert_eq!(
        session.overview(),
        ProjectOverview::Unavailable {
            directory: start(),
            error: "git failed".into(),
        }
    );
    Ok(())
}

#[test]
fn a_repository_without_a_project_is_uninitialized() -> Result<(), Box<dyn Error>> {
    let session = Session::new(Repository::Present, Ok(None))?;
    assert_eq!(
        session.overview(),
        ProjectOverview::Uninitialized {
            root: RepositoryRoot::new(start())?,
        }
    );
    Ok(())
}

#[test]
fn an_unreadable_project_is_unavailable_with_its_error() -> Result<(), Box<dyn Error>> {
    let session = Session::new(
        Repository::Present,
        Err("gitserious.toml is not valid TOML"),
    )?;
    assert_eq!(
        session.overview(),
        ProjectOverview::Unavailable {
            directory: start(),
            error: "gitserious.toml is not valid TOML".into(),
        }
    );
    Ok(())
}

#[test]
fn an_initialized_project_reports_its_default_taxonomy_and_version() -> Result<(), Box<dyn Error>> {
    let session = Session::new(Repository::Present, Ok(Some(built_ins()?)))?;
    assert_eq!(
        session.overview(),
        ProjectOverview::Initialized {
            root: RepositoryRoot::new(start())?,
            default: TaxonomyId::new("conventional")?,
            version: Some(TaxonomyVersion::V1),
            policy_untracked: false,
        }
    );
    Ok(())
}

#[test]
fn only_an_untracked_policy_file_at_the_root_stays_local() -> Result<(), Box<dyn Error>> {
    assert!(policy_untracked(vec![entry("gitserious.toml", true)])?);
    assert!(!policy_untracked(vec![entry("gitserious.toml", false)])?);
    assert!(!policy_untracked(vec![entry("notes.txt", true)])?);
    assert!(!policy_untracked(vec![entry(
        "nested/gitserious.toml",
        true
    )])?);
    Ok(())
}

#[test]
fn a_failed_status_read_reports_the_policy_as_tracked() -> Result<(), Box<dyn Error>> {
    let mut session = Session::new(Repository::Present, Ok(Some(built_ins()?)))?;
    session.status = Status(Err("git status failed".into()));
    assert!(matches!(
        session.overview(),
        ProjectOverview::Initialized {
            policy_untracked: false,
            ..
        }
    ));
    Ok(())
}

#[test]
fn initialization_creates_git_only_when_no_repository_exists() -> Result<(), Box<dyn Error>> {
    let session = Session::new(Repository::Missing, Ok(None))?;
    let initializer = session.initializer(None);
    session.project(&initializer).initialize(&built_ins()?)?;
    assert_eq!(initializer.calls.get(), 1);
    assert_eq!(
        session.calls(),
        [StoreCall::EnsureLocalState, StoreCall::Create]
    );
    assert!(matches!(
        session.overview(),
        ProjectOverview::Initialized { .. }
    ));

    let session = Session::new(Repository::Present, Ok(None))?;
    let initializer = session.initializer(None);
    session.project(&initializer).initialize(&built_ins()?)?;
    assert_eq!(initializer.calls.get(), 0, "an existing repository is kept");
    assert_eq!(
        session.calls(),
        [StoreCall::EnsureLocalState, StoreCall::Create]
    );
    Ok(())
}

#[test]
fn a_broken_repository_neither_creates_git_nor_writes_a_project() -> Result<(), Box<dyn Error>> {
    let session = Session::new(Repository::Broken, Ok(None))?;
    let initializer = session.initializer(None);
    assert_eq!(
        session.project(&initializer).initialize(&built_ins()?),
        Err("git failed".into())
    );
    assert_eq!(initializer.calls.get(), 0);
    assert!(session.calls().is_empty());
    Ok(())
}

#[test]
fn a_failed_git_init_writes_no_project() -> Result<(), Box<dyn Error>> {
    let session = Session::new(Repository::Missing, Ok(None))?;
    let initializer = session.initializer(Some("git init failed"));
    assert_eq!(
        session.project(&initializer).initialize(&built_ins()?),
        Err("git init failed".into())
    );
    assert!(session.calls().is_empty());
    Ok(())
}

#[test]
fn a_failed_local_state_writes_no_project() -> Result<(), Box<dyn Error>> {
    let mut session = Session::new(Repository::Present, Ok(None))?;
    session.store.local_state = Err(".gitserious is not writable");
    let initializer = session.initializer(None);
    assert_eq!(
        session.project(&initializer).initialize(&built_ins()?),
        Err(".gitserious is not writable".into())
    );
    assert_eq!(session.calls(), [StoreCall::EnsureLocalState]);
    Ok(())
}

#[test]
fn an_existing_project_is_refused_and_kept() -> Result<(), Box<dyn Error>> {
    let existing = ProjectConfiguration::new(
        1,
        TaxonomyId::new("research")?,
        built_in_taxonomies().to_vec(),
    )?;
    let session = Session::new(Repository::Present, Ok(Some(existing.clone())))?;
    let initializer = session.initializer(None);
    assert_eq!(
        session.project(&initializer).initialize(&built_ins()?),
        Err("gitserious.toml already exists".into())
    );
    assert_eq!(*session.store.project.borrow(), Ok(Some(existing)));
    Ok(())
}
