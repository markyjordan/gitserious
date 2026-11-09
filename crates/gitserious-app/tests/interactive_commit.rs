use std::cell::{Cell, RefCell};
use std::error::Error;
use std::path::{Path, PathBuf};

use gitserious_app::{
    AuthoredCommit, BranchState, CommitAuthoringContext, CommitOutput, CommitWriter,
    GlobalConfiguration, GlobalConfigurationStore, HelpPage, InteractiveUi, InteractiveWorkspace,
    ProjectConfiguration, ProjectConfigurationStore, ProjectRevision, RepositoryInitializer,
    RepositoryLocator, RepositoryRoot, RepositoryStatus, RepositoryStatusReader, StatusEntry,
    StoredProject, run_interactive,
};
use gitserious_core::{
    CommitDraft, CommitMessage, CommitSubject, CommitTypeDefinition, CommitTypeId, Description,
    Taxonomy, TaxonomyId,
};

struct Locator {
    root: RepositoryRoot,
    failure: Cell<Option<&'static str>>,
}

impl RepositoryLocator for Locator {
    type Error = &'static str;

    fn locate(&self, _: &Path) -> Result<RepositoryRoot, Self::Error> {
        self.failure
            .get()
            .map_or_else(|| Ok(self.root.clone()), Err)
    }

    fn is_not_repository(_: &Self::Error) -> bool {
        false
    }
}

struct Project(RefCell<Result<Option<ProjectConfiguration>, &'static str>>);

impl ProjectConfigurationStore for Project {
    type Error = &'static str;

    fn load(&self, _: &RepositoryRoot) -> Result<Option<StoredProject>, Self::Error> {
        self.0.borrow().clone().map(|project| {
            project.map(|configuration| StoredProject {
                configuration,
                revision: ProjectRevision::of_bytes(b"persisted project"),
            })
        })
    }

    fn create(
        &self,
        _: &RepositoryRoot,
        _: &ProjectConfiguration,
    ) -> Result<ProjectRevision, Self::Error> {
        Err("unexpected project creation")
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
        Err("unexpected project initialization")
    }
}

struct Library(RefCell<GlobalConfiguration>);

impl GlobalConfigurationStore for Library {
    type Error = &'static str;

    fn path(&self) -> PathBuf {
        std::env::temp_dir().join("gitserious-test-library.toml")
    }

    fn load(&self) -> Result<GlobalConfiguration, Self::Error> {
        Ok(self.0.borrow().clone())
    }

    fn compare_and_swap(
        &self,
        _: &GlobalConfiguration,
        _: &GlobalConfiguration,
    ) -> Result<(), Self::Error> {
        Err("unexpected library save")
    }
}

struct Status(RefCell<Result<RepositoryStatus, String>>);

impl RepositoryStatusReader for Status {
    fn read(&self, _: &RepositoryRoot) -> Result<RepositoryStatus, String> {
        self.0.borrow().clone()
    }
}

struct Initializer;

impl RepositoryInitializer for Initializer {
    fn initialize(&self, _: &Path) -> Result<(), String> {
        Err("unexpected Git initialization".into())
    }
}

#[derive(Default)]
struct Writer(RefCell<Vec<(RepositoryRoot, CommitMessage)>>);

impl CommitWriter for Writer {
    type Error = &'static str;

    fn commit(
        &self,
        root: &RepositoryRoot,
        message: &CommitMessage,
    ) -> Result<CommitOutput, Self::Error> {
        self.0.borrow_mut().push((root.clone(), message.clone()));
        Ok(CommitOutput::new(b"created\n".to_vec(), Vec::new()))
    }
}

struct Script<F>(F);

impl<F> InteractiveUi for Script<F>
where
    F: Fn(&dyn InteractiveWorkspace) -> Result<(), Box<dyn Error>>,
{
    fn run(&self, workspace: &dyn InteractiveWorkspace, _: &[HelpPage]) -> Result<(), String> {
        (self.0)(workspace).map_err(|error| error.to_string())
    }
}

struct Session {
    locator: Locator,
    project: Project,
    library: Library,
    status: Status,
    writer: Writer,
}

impl Session {
    fn new(entries: Vec<StatusEntry>) -> Result<Self, Box<dyn Error>> {
        let root = RepositoryRoot::new(std::env::temp_dir())?;
        Ok(Self {
            locator: Locator {
                root: root.clone(),
                failure: Cell::new(None),
            },
            project: Project(RefCell::new(Ok(Some(project_configuration()?)))),
            library: Library(RefCell::new(GlobalConfiguration::default())),
            status: Status(RefCell::new(Ok(RepositoryStatus {
                root,
                branch: BranchState::Named("main".into()),
                entries,
            }))),
            writer: Writer::default(),
        })
    }

    fn run<F>(&self, script: F) -> Result<(), Box<dyn Error>>
    where
        F: Fn(&dyn InteractiveWorkspace) -> Result<(), Box<dyn Error>>,
    {
        run_interactive(
            &self.locator,
            &self.project,
            &self.library,
            &self.status,
            &Initializer,
            &self.writer,
            self.locator.root.as_path(),
            &Script(script),
            &[],
        )?;
        Ok(())
    }

    fn set_entries(&self, entries: Vec<StatusEntry>) {
        *self.status.0.borrow_mut() = Ok(RepositoryStatus {
            root: self.locator.root.clone(),
            branch: BranchState::Named("main".into()),
            entries,
        });
    }
}

fn taxonomy(name: &str) -> Result<Taxonomy, Box<dyn Error>> {
    Ok(Taxonomy::new(
        TaxonomyId::new(name)?,
        None,
        Description::new("Project change notes.")?,
        None,
        vec![CommitTypeDefinition::new(
            None,
            CommitTypeId::new("note")?,
            "Records a change.",
            Vec::new(),
        )?],
    )?)
}

fn project_configuration() -> Result<ProjectConfiguration, Box<dyn Error>> {
    Ok(ProjectConfiguration::new(
        1,
        TaxonomyId::new("team")?,
        vec![taxonomy("team")?, taxonomy("other")?],
    )?)
}

fn approved(context: &CommitAuthoringContext) -> Result<AuthoredCommit, Box<dyn Error>> {
    let taxonomy = context.initial_taxonomy();
    let draft = CommitDraft::new(
        CommitTypeId::new("note")?,
        None,
        CommitSubject::new("keep the approved message")?,
        Vec::new(),
    )?;
    let message = taxonomy.validate(&draft).into_message()?;
    Ok(AuthoredCommit::reviewed(
        taxonomy.id().clone(),
        draft,
        message,
    ))
}

fn approved_project_commit() -> Result<AuthoredCommit, Box<dyn Error>> {
    let project = project_configuration()?;
    approved(&CommitAuthoringContext::new(
        project.taxonomies().to_vec(),
        project.default_taxonomy(),
        None,
    )?)
}

fn entry(path: &str, index: char, worktree: char) -> StatusEntry {
    StatusEntry {
        path: path.into(),
        original_path: None,
        index,
        worktree,
        untracked: index == '?',
        conflicted: index == 'U',
        submodule: "N...".into(),
    }
}

#[test]
fn preparation_and_submission_require_a_staged_conflict_free_index() -> Result<(), Box<dyn Error>> {
    let cases = [
        ("clean", vec![], Some("Stage changes")),
        (
            "unstaged",
            vec![entry("file", '.', 'M')],
            Some("Stage changes"),
        ),
        (
            "untracked",
            vec![entry("file", '?', '?')],
            Some("Stage changes"),
        ),
        ("staged", vec![entry("file", 'M', '.')], None),
        ("partially staged", vec![entry("file", 'M', 'M')], None),
        (
            "conflicted",
            vec![entry("file", 'U', 'U')],
            Some("conflicts"),
        ),
        (
            "conflict and staged",
            vec![entry("conflict", 'U', 'U'), entry("staged", 'M', '.')],
            Some("conflicts"),
        ),
    ];
    for (name, entries, blocker) in cases {
        let session = Session::new(entries)?;
        session.run(|workspace| {
            let preparation = workspace.prepare_commit();
            if let Some(blocker) = blocker {
                let error = preparation
                    .err()
                    .ok_or("ineligible preparation succeeded")?;
                assert!(error.contains(blocker), "{name}: {error}");
                let error = workspace
                    .submit_commit(&approved_project_commit()?)
                    .err()
                    .ok_or("ineligible submission succeeded")?;
                assert!(error.contains(blocker), "{name}: {error}");
                assert!(session.writer.0.borrow().is_empty(), "{name}");
            } else {
                let authored = approved(&preparation?)?;
                let output = workspace.submit_commit(&authored)?;
                assert_eq!(output.stdout(), b"created\n", "{name}");
                assert_eq!(
                    session.writer.0.borrow().as_slice(),
                    &[(
                        session.locator.root.clone(),
                        authored
                            .reviewed_message()
                            .ok_or("no approved bytes")?
                            .clone()
                    )],
                    "{name}"
                );
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn locator_and_status_failures_stop_preparation_and_submission() -> Result<(), Box<dyn Error>> {
    for failure in ["locator unavailable", "status unavailable"] {
        let session = Session::new(vec![entry("file", 'M', '.')])?;
        if failure.starts_with("locator") {
            session.locator.failure.set(Some(failure));
        } else {
            *session.status.0.borrow_mut() = Err(failure.into());
        }
        session.run(|workspace| {
            assert_eq!(workspace.prepare_commit().err().as_deref(), Some(failure));
            assert_eq!(
                workspace
                    .submit_commit(&approved_project_commit()?)
                    .err()
                    .as_deref(),
                Some(failure)
            );
            Ok(())
        })?;
        assert!(session.writer.0.borrow().is_empty());
    }
    Ok(())
}

#[test]
fn missing_or_unreadable_project_policy_never_reaches_the_writer() -> Result<(), Box<dyn Error>> {
    for policy in [Ok(None), Err("invalid persisted project policy")] {
        let session = Session::new(vec![entry("file", 'M', '.')])?;
        *session.project.0.borrow_mut() = policy;
        session.run(|workspace| {
            assert!(workspace.prepare_commit().is_err());
            assert!(
                workspace
                    .submit_commit(&approved_project_commit()?)
                    .is_err()
            );
            Ok(())
        })?;
        assert!(session.writer.0.borrow().is_empty());
    }
    Ok(())
}

#[test]
fn submission_rechecks_index_and_conflicts_after_approval() -> Result<(), Box<dyn Error>> {
    for entries in [
        vec![],
        vec![entry("conflict", 'U', 'U'), entry("staged", 'M', '.')],
    ] {
        let session = Session::new(vec![entry("file", 'M', '.')])?;
        session.run(|workspace| {
            let authored = approved(&workspace.prepare_commit()?)?;
            session.set_entries(entries.clone());
            assert!(workspace.submit_commit(&authored).is_err());
            Ok(())
        })?;
        assert!(session.writer.0.borrow().is_empty());
    }
    Ok(())
}
