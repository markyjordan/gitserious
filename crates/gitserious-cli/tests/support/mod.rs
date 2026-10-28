#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gitserious_app::{
    AuthoredCommit, CommitAuthoringContext, CommitAuthoringOutcome, CommitDraftAuthor,
    CommitDraftAuthorOutcome, CommitOutput, CommitWriter, GlobalConfiguration,
    GlobalConfigurationStore, ProjectConfig, ProjectLock, ProjectState, ProjectStateStore,
    RepositoryLocator, RepositoryRoot, resolve_effective_configuration,
};
use gitserious_cli::{CommitAdapters, run_from_with_commit};
use gitserious_core::{
    AuthoredProperty, CommitDraft, CommitMessage, CommitScope, CommitSubject, CommitTypeDefinition,
    CommitTypeId, PropertyRequirement, PropertyValue,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FakeError(pub &'static str);

impl Display for FakeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for FakeError {}

pub fn repository_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fake-repository")
}

pub struct Locator {
    pub fail: bool,
}

impl RepositoryLocator for Locator {
    type Error = FakeError;

    fn locate(&self, _start: &Path) -> Result<RepositoryRoot, Self::Error> {
        if self.fail {
            return Err(FakeError("repository unavailable"));
        }
        RepositoryRoot::new(repository_path()).map_err(|_| FakeError("invalid root"))
    }
}

pub struct Global {
    pub configuration: GlobalConfiguration,
    pub fail: bool,
}

impl Global {
    pub fn new(configuration: GlobalConfiguration) -> Self {
        Self {
            configuration,
            fail: false,
        }
    }
}

impl GlobalConfigurationStore for Global {
    type Error = FakeError;

    fn load(&self) -> Result<GlobalConfiguration, Self::Error> {
        if self.fail {
            return Err(FakeError("global configuration unavailable"));
        }
        Ok(self.configuration.clone())
    }

    fn compare_and_swap(
        &self,
        _: &GlobalConfiguration,
        _: &GlobalConfiguration,
    ) -> Result<(), Self::Error> {
        Err(FakeError("unexpected global write"))
    }
}

#[derive(Default)]
pub struct Project {
    pub state: RefCell<Option<ProjectState>>,
    pub initialized: RefCell<Option<(ProjectConfig, ProjectLock)>>,
}

impl Project {
    pub fn with(state: ProjectState) -> Self {
        Self {
            state: RefCell::new(Some(state)),
            initialized: RefCell::default(),
        }
    }
}

impl ProjectStateStore for Project {
    type Error = FakeError;

    fn inspect(&self, _: &RepositoryRoot) -> Result<ProjectState, Self::Error> {
        Ok(self.state.borrow().clone().unwrap_or(ProjectState::Absent))
    }
    fn ensure_local_state(&self, _: &RepositoryRoot) -> Result<(), Self::Error> {
        Ok(())
    }
    fn initialize(
        &self,
        _: &RepositoryRoot,
        config: &ProjectConfig,
        lock: &ProjectLock,
    ) -> Result<(), Self::Error> {
        *self.initialized.borrow_mut() = Some((config.clone(), lock.clone()));
        Ok(())
    }
    fn create_lock(&self, _: &RepositoryRoot, _: &ProjectLock) -> Result<(), Self::Error> {
        Err(FakeError("unexpected create lock"))
    }
    fn replace_lock(
        &self,
        _: &RepositoryRoot,
        _: &ProjectLock,
        _: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Err(FakeError("unexpected replace lock"))
    }
    fn compare_and_swap(
        &self,
        _: &RepositoryRoot,
        _: &ProjectConfig,
        _: &ProjectLock,
        _: &ProjectConfig,
        _: &ProjectLock,
    ) -> Result<(), Self::Error> {
        Err(FakeError("unexpected project write"))
    }
}

pub fn initialized_state() -> Result<ProjectState, Box<dyn Error>> {
    let config = ProjectConfig::inherited();
    let effective = resolve_effective_configuration(&GlobalConfiguration::default(), &config)?;
    let lock = ProjectLock::capture(&config, &effective)?;
    Ok(ProjectState::Initialized { config, lock })
}

pub enum AuthorOutcome {
    Valid,
    Cancelled,
    Failed,
}

pub struct Author {
    pub outcome: RefCell<AuthorOutcome>,
    pub calls: Cell<usize>,
    pub seen: RefCell<Vec<(Vec<CommitTypeId>, Option<CommitTypeId>)>>,
}

impl Default for Author {
    fn default() -> Self {
        Self {
            outcome: RefCell::new(AuthorOutcome::Valid),
            calls: Cell::new(0),
            seen: RefCell::default(),
        }
    }
}

impl CommitDraftAuthor for Author {
    type Error = FakeError;

    fn author(
        &self,
        _: &[CommitTypeDefinition],
        _: Option<&CommitTypeDefinition>,
    ) -> Result<CommitDraftAuthorOutcome, Self::Error> {
        Err(FakeError("unexpected flat authoring"))
    }

    fn author_with_context(
        &self,
        context: &CommitAuthoringContext,
    ) -> Result<CommitAuthoringOutcome, Self::Error> {
        self.calls.set(self.calls.get() + 1);
        let taxonomy = context.initial_taxonomy();
        self.seen.borrow_mut().push((
            taxonomy
                .definitions()
                .iter()
                .map(|definition| definition.id().clone())
                .collect(),
            context
                .preselected_type()
                .map(|definition| definition.id().clone()),
        ));
        match *self.outcome.borrow() {
            AuthorOutcome::Valid => {
                let definition = context
                    .preselected_type()
                    .unwrap_or(&taxonomy.definitions()[0]);
                let draft = valid_draft(definition).map_err(|_| FakeError("invalid draft"))?;
                let message = taxonomy
                    .validate(&draft)
                    .into_message()
                    .map_err(|_| FakeError("unrenderable draft"))?;
                Ok(CommitAuthoringOutcome::Authored(AuthoredCommit::reviewed(
                    taxonomy.id().clone(),
                    draft,
                    message,
                )))
            }
            AuthorOutcome::Cancelled => Ok(CommitAuthoringOutcome::Cancelled),
            AuthorOutcome::Failed => Err(FakeError("fake adapter failure")),
        }
    }
}

pub fn valid_draft(definition: &CommitTypeDefinition) -> Result<CommitDraft, Box<dyn Error>> {
    let properties = definition
        .properties()
        .iter()
        .filter(|property| property.requirement() == &PropertyRequirement::Required)
        .map(|property| {
            Ok(AuthoredProperty::new(
                property.key().clone(),
                PropertyValue::new(format!("authored {}", property.key()))?,
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    Ok(CommitDraft::new(
        definition.id().clone(),
        Some(CommitScope::new("tui editor")?),
        CommitSubject::new("expose command")?,
        properties,
    )?
    .with_breaking_change(PropertyValue::new("replace CLI contract")?))
}

#[derive(Default)]
pub struct Writer {
    pub calls: Cell<usize>,
    pub messages: RefCell<Vec<String>>,
}

impl CommitWriter for Writer {
    type Error = FakeError;

    fn commit(
        &self,
        _: &RepositoryRoot,
        message: &CommitMessage,
    ) -> Result<CommitOutput, Self::Error> {
        self.calls.set(self.calls.get() + 1);
        self.messages.borrow_mut().push(message.as_str().to_owned());
        Ok(CommitOutput::new(
            b"git summary\n".to_vec(),
            b"hook warning\n".to_vec(),
        ))
    }
}

pub struct Harness {
    pub locator: Locator,
    pub project: Project,
    pub global: Global,
    pub author: Author,
    pub writer: Writer,
}

impl Harness {
    pub fn new(project: Project) -> Self {
        Self {
            locator: Locator { fail: false },
            project,
            global: Global::new(GlobalConfiguration::default()),
            author: Author::default(),
            writer: Writer::default(),
        }
    }

    pub fn run(&self, arguments: &[&str]) -> (ExitCode, String, String) {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit = run_from_with_commit(
            arguments.iter().copied(),
            &repository_path().join("subdir"),
            &self.locator,
            &self.project,
            &self.global,
            &CommitAdapters::new(&self.author, &self.writer),
            &mut stdout,
            &mut stderr,
        );
        (
            exit,
            String::from_utf8_lossy(&stdout).into_owned(),
            String::from_utf8_lossy(&stderr).into_owned(),
        )
    }
}
