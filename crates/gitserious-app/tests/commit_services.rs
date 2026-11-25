use std::cell::{Cell, RefCell};
use std::error::Error;
use std::path::{Path, PathBuf};

use gitserious_app::{
    AuthoredCommit, BranchState, CitedCommit, CommitAuthoringContext, CommitOutput, CommitServices,
    CommitTaxonomy, CommitWorkspace, CommitWriter, ConfigurationServices, ConfigurationWorkspace,
    DraftStore, FileEditor, GitOperation, GlobalConfiguration, GlobalConfigurationStore,
    IndexWriter, ProjectConfiguration, ProjectConfigurationStore, ProjectRevision,
    RepositoryLocator, RepositoryRoot, RepositoryStatus, RepositoryStatusReader, SavedDraft,
    StatusEntry, StoredProject, SubmitError, WorktreePaths,
};
use gitserious_core::{
    AuthoredProperty, CommitDraft, CommitMessage, CommitProvenance, CommitSubject,
    CommitTypeDefinition, CommitTypeId, Description, Fingerprint, PropertyDefinition, PropertyKey,
    PropertyRequirement, PropertyValue, SchemaVersion, Taxonomy, TaxonomyId, TaxonomyVersion,
    built_in_commit_types, built_in_taxonomies, validate_commit_draft,
    validate_commit_draft_with_provenance,
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

    fn missing_work_tree(_: &Self::Error) -> Option<gitserious_app::MissingWorkTree> {
        None
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
                operation: None,
            }))),
            writer: Writer::default(),
        })
    }

    fn commit(&self) -> CommitServices<'_, Locator, Project, Writer> {
        CommitServices::new(
            &self.locator,
            &self.project,
            &self.status,
            &self.writer,
            self.locator.root.as_path(),
        )
    }

    fn configuration(&self) -> ConfigurationServices<'_, Locator, Project, Library> {
        ConfigurationServices::new(
            &self.locator,
            &self.project,
            &self.library,
            self.locator.root.as_path(),
        )
    }

    fn set_entries(&self, entries: Vec<StatusEntry>) {
        *self.status.0.borrow_mut() = Ok(RepositoryStatus {
            root: self.locator.root.clone(),
            branch: BranchState::Named("main".into()),
            entries,
            operation: None,
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
fn preparation_accepts_any_index() -> Result<(), Box<dyn Error>> {
    for entries in [
        vec![],
        vec![entry("file", '.', 'M')],
        vec![entry("file", '?', '?')],
        vec![entry("file", 'U', 'U')],
        vec![entry("file", 'M', '.')],
    ] {
        let session = Session::new(entries.clone())?;
        session
            .commit()
            .prepare_commit()
            .map_err(|error| format!("{entries:?}: {error}"))?;
    }
    Ok(())
}

#[test]
fn submission_requires_a_staged_conflict_free_index() -> Result<(), Box<dyn Error>> {
    let cases = [
        ("clean", vec![], Some(SubmitError::NothingStaged)),
        (
            "unstaged",
            vec![entry("file", '.', 'M')],
            Some(SubmitError::NothingStaged),
        ),
        (
            "untracked",
            vec![entry("file", '?', '?')],
            Some(SubmitError::NothingStaged),
        ),
        ("staged", vec![entry("file", 'M', '.')], None),
        ("partially staged", vec![entry("file", 'M', 'M')], None),
        (
            "conflicted",
            vec![entry("file", 'U', 'U')],
            Some(SubmitError::Conflicts(vec!["file".into()])),
        ),
        (
            "conflict and staged",
            vec![entry("conflict", 'U', 'U'), entry("staged", 'M', '.')],
            Some(SubmitError::Conflicts(vec!["conflict".into()])),
        ),
    ];
    for (name, entries, refusal) in cases {
        let session = Session::new(entries)?;
        let commit = session.commit();
        let authored = approved(&commit.prepare_commit()?)?;
        let result = commit.submit_commit(&authored);
        if let Some(refusal) = refusal {
            assert_eq!(result.err(), Some(refusal), "{name}");
            assert!(session.writer.0.borrow().is_empty(), "{name}");
        } else {
            assert_eq!(result?.stdout(), b"created\n", "{name}");
            assert_eq!(
                session.writer.0.borrow().as_slice(),
                &[(
                    session.locator.root.clone(),
                    authored.reviewed_message().clone()
                )],
                "{name}"
            );
        }
    }
    Ok(())
}

#[test]
fn submission_checks_the_index_before_reloading_policy() -> Result<(), Box<dyn Error>> {
    let session = Session::new(Vec::new())?;
    *session.project.0.borrow_mut() = Ok(None);

    assert_eq!(
        session
            .commit()
            .submit_commit(&approved_project_commit()?)
            .err(),
        Some(SubmitError::NothingStaged)
    );
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
        let commit = session.commit();
        assert_eq!(commit.prepare_commit().err().as_deref(), Some(failure));
        assert_eq!(
            commit.submit_commit(&approved_project_commit()?).err(),
            Some(SubmitError::Failed(failure.into()))
        );
        assert!(session.writer.0.borrow().is_empty());
    }
    Ok(())
}

#[test]
fn missing_or_unreadable_project_policy_never_reaches_the_writer() -> Result<(), Box<dyn Error>> {
    for policy in [Ok(None), Err("invalid persisted project policy")] {
        let session = Session::new(vec![entry("file", 'M', '.')])?;
        *session.project.0.borrow_mut() = policy;
        let commit = session.commit();
        assert!(commit.prepare_commit().is_err());
        assert!(commit.submit_commit(&approved_project_commit()?).is_err());
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
        let commit = session.commit();
        let authored = approved(&commit.prepare_commit()?)?;
        session.set_entries(entries.clone());
        assert!(commit.submit_commit(&authored).is_err());
        assert!(session.writer.0.borrow().is_empty());
    }
    Ok(())
}

#[test]
fn submission_rejects_a_removed_or_incompatible_authored_taxonomy() -> Result<(), Box<dyn Error>> {
    let required_property = Taxonomy::new(
        TaxonomyId::new("team")?,
        None,
        Description::new("Project change notes.")?,
        None,
        vec![CommitTypeDefinition::new(
            None,
            CommitTypeId::new("note")?,
            "Records a change.",
            vec![PropertyDefinition::new(
                PropertyKey::new("reason")?,
                "Why the change is needed.",
                PropertyRequirement::Required,
            )?],
        )?],
    )?;
    let replacements = [
        ProjectConfiguration::new(1, TaxonomyId::new("other")?, vec![taxonomy("other")?])?,
        ProjectConfiguration::new(
            1,
            TaxonomyId::new("team")?,
            vec![required_property, taxonomy("other")?],
        )?,
    ];
    for replacement in replacements {
        let session = Session::new(vec![entry("file", 'M', '.')])?;
        let commit = session.commit();
        let authored = approved(&commit.prepare_commit()?)?;
        *session.project.0.borrow_mut() = Ok(Some(replacement.clone()));
        assert!(commit.submit_commit(&authored).is_err());
        assert!(session.writer.0.borrow().is_empty());
    }
    Ok(())
}

#[test]
fn submission_rejects_changed_provenance_even_when_the_draft_remains_valid()
-> Result<(), Box<dyn Error>> {
    let session = Session::new(vec![entry("file", 'M', '.')])?;
    let commit = session.commit();
    let authored = approved(&commit.prepare_commit()?)?;
    let original = taxonomy("team")?;
    let changed = Taxonomy::new(
        original.id().clone(),
        original.version(),
        Description::new("A changed project description.")?,
        original.derived_from().cloned(),
        original.commit_types().to_vec(),
    )?;
    *session.project.0.borrow_mut() = Ok(Some(ProjectConfiguration::new(
        1,
        changed.id().clone(),
        vec![changed, taxonomy("other")?],
    )?));
    let current = commit.prepare_commit()?;
    let new_message = current
        .find_taxonomy(authored.taxonomy())
        .ok_or("selected taxonomy disappeared")?
        .validate(authored.draft())
        .into_message()?;
    assert_ne!(&new_message, authored.reviewed_message());
    assert!(commit.submit_commit(&authored).is_err());
    assert!(session.writer.0.borrow().is_empty());
    Ok(())
}

#[test]
fn a_default_only_change_keeps_the_authored_taxonomy_usable() -> Result<(), Box<dyn Error>> {
    let session = Session::new(vec![entry("file", 'M', '.')])?;
    let commit = session.commit();
    let authored = approved(&commit.prepare_commit()?)?;
    *session.project.0.borrow_mut() = Ok(Some(ProjectConfiguration::new(
        1,
        TaxonomyId::new("other")?,
        vec![taxonomy("team")?, taxonomy("other")?],
    )?));
    assert_eq!(
        commit.prepare_commit()?.initial_taxonomy().id().as_str(),
        "other"
    );
    commit.submit_commit(&authored)?;
    assert_eq!(
        session.writer.0.borrow().as_slice(),
        &[(
            session.locator.root.clone(),
            authored.reviewed_message().clone()
        )]
    );
    Ok(())
}

#[test]
fn persisted_policy_loss_after_approval_prevents_writing() -> Result<(), Box<dyn Error>> {
    for replacement in [Ok(None), Err("invalid persisted project policy")] {
        let session = Session::new(vec![entry("file", 'M', '.')])?;
        let commit = session.commit();
        let authored = approved(&commit.prepare_commit()?)?;
        *session.project.0.borrow_mut() = replacement.clone();
        assert!(commit.submit_commit(&authored).is_err());
        assert!(session.writer.0.borrow().is_empty());
    }
    Ok(())
}

#[test]
fn a_changed_library_does_not_replace_project_owned_commit_policy() -> Result<(), Box<dyn Error>> {
    let session = Session::new(vec![entry("file", 'M', '.')])?;
    *session.library.0.borrow_mut() = GlobalConfiguration::new(
        1,
        TaxonomyId::new("team")?,
        vec![TaxonomyId::new("team")?],
        vec![taxonomy("team")?.into()],
    )?;
    let commit = session.commit();
    let authored = approved(&commit.prepare_commit()?)?;
    let original = taxonomy("team")?;
    let changed = Taxonomy::new(
        original.id().clone(),
        original.version(),
        Description::new("The library changed after review.")?,
        original.derived_from().cloned(),
        original.commit_types().to_vec(),
    )?;
    *session.library.0.borrow_mut() = GlobalConfiguration::new(
        1,
        changed.id().clone(),
        vec![changed.id().clone()],
        vec![changed.clone().into()],
    )?;
    assert_eq!(session.configuration().library_taxonomies()?, vec![changed]);
    commit.submit_commit(&authored)?;
    assert_eq!(
        session.writer.0.borrow().as_slice(),
        &[(
            session.locator.root.clone(),
            authored.reviewed_message().clone()
        )]
    );
    Ok(())
}

struct FailingWriter(Cell<usize>);

impl CommitWriter for FailingWriter {
    type Error = &'static str;

    fn commit(&self, _: &RepositoryRoot, _: &CommitMessage) -> Result<CommitOutput, Self::Error> {
        self.0.set(self.0.get() + 1);
        Err("writer failed")
    }
}

fn staged(project: Option<ProjectConfiguration>) -> Result<Session, Box<dyn Error>> {
    let session = Session::new(vec![entry("file", 'M', '.')])?;
    *session.project.0.borrow_mut() = Ok(project);
    Ok(session)
}

fn built_in_project() -> Result<ProjectConfiguration, Box<dyn Error>> {
    Ok(ProjectConfiguration::new(
        1,
        TaxonomyId::new("conventional")?,
        built_in_taxonomies().to_vec(),
    )?)
}

fn complete_draft(
    taxonomy: &CommitTaxonomy,
    commit_type: &str,
) -> Result<CommitDraft, Box<dyn Error>> {
    let definition = taxonomy
        .definitions()
        .iter()
        .find(|definition| definition.id().as_str() == commit_type)
        .ok_or("missing commit type")?;
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
        None,
        CommitSubject::new("create durable commit")?,
        properties,
    )?)
}

fn reviewed(
    taxonomy: &CommitTaxonomy,
    commit_type: &str,
) -> Result<AuthoredCommit, Box<dyn Error>> {
    let draft = complete_draft(taxonomy, commit_type)?;
    let message = taxonomy.validate(&draft).into_message()?;
    Ok(AuthoredCommit::reviewed(
        taxonomy.id().clone(),
        draft,
        message,
    ))
}

fn written(session: &Session) -> Vec<String> {
    session
        .writer
        .0
        .borrow()
        .iter()
        .map(|(_, message)| message.as_str().to_owned())
        .collect()
}

#[test]
fn submission_writes_one_reviewed_message_under_the_default_taxonomy() -> Result<(), Box<dyn Error>>
{
    let session = staged(Some(built_in_project()?))?;
    let commit = session.commit();
    let context = commit.prepare_commit()?;
    let authored = reviewed(context.initial_taxonomy(), "feat")?;

    let output = commit.submit_commit(&authored)?;

    assert_eq!(output.stdout(), b"created\n");
    let messages = written(&session);
    assert_eq!(messages, [authored.reviewed_message().as_str()]);
    assert!(messages[0].starts_with("feat: create durable commit\n"));
    assert!(messages[0].contains("Gitserious-Taxonomy: conventional-v1\n"));
    Ok(())
}

#[test]
fn preparation_offers_the_default_taxonomy_types_in_policy_order() -> Result<(), Box<dyn Error>> {
    let session = staged(Some(built_in_project()?))?;

    let context = session.commit().prepare_commit()?;

    assert!(context.preselected_type().is_none());
    assert_eq!(
        context
            .initial_taxonomy()
            .definitions()
            .iter()
            .map(|definition| definition.id().clone())
            .collect::<Vec<_>>(),
        built_in_commit_types()
            .iter()
            .map(|definition| definition.id().clone())
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn an_uninitialized_project_names_init_before_authoring() -> Result<(), Box<dyn Error>> {
    let session = staged(None)?;
    let commit = session.commit();

    for error in [
        commit.prepare_commit().err(),
        commit
            .submit_commit(&approved_project_commit()?)
            .err()
            .map(|error| error.to_string()),
    ] {
        let error = error.ok_or("an uninitialized project was accepted")?;
        assert!(
            error.contains("run `gitserious init` before committing"),
            "{error}"
        );
    }
    assert!(written(&session).is_empty());
    Ok(())
}

#[test]
fn a_project_owned_built_in_replaces_the_compiled_schema() -> Result<(), Box<dyn Error>> {
    let conventional = Taxonomy::new(
        TaxonomyId::new("conventional")?,
        Some(TaxonomyVersion::new(7)?),
        Description::new("A project-owned conventional taxonomy.")?,
        None,
        vec![CommitTypeDefinition::new(
            Some(SchemaVersion::V1),
            CommitTypeId::new("ship")?,
            "Ships a change.",
            vec![PropertyDefinition::new(
                PropertyKey::new("intent")?,
                "Why the change ships.",
                PropertyRequirement::Required,
            )?],
        )?],
    )?;
    let session = staged(Some(ProjectConfiguration::new(
        1,
        conventional.id().clone(),
        vec![conventional],
    )?))?;
    let commit = session.commit();
    let context = commit.prepare_commit()?;

    commit.submit_commit(&reviewed(context.initial_taxonomy(), "ship")?)?;

    assert!(written(&session)[0].contains("Gitserious-Taxonomy: conventional-v7\n"));
    Ok(())
}

#[test]
fn types_outside_the_taxonomy_never_reach_git() -> Result<(), Box<dyn Error>> {
    let session = staged(Some(built_in_project()?))?;
    let commit = session.commit();
    let context = commit.prepare_commit()?;
    let custom = AuthoredCommit::reviewed(
        context.initial_taxonomy().id().clone(),
        CommitDraft::new(
            CommitTypeId::new("custom")?,
            None,
            CommitSubject::new("escape policy")?,
            Vec::new(),
        )?,
        reviewed(context.initial_taxonomy(), "fix")?
            .reviewed_message()
            .clone(),
    );

    let error = commit
        .submit_commit(&custom)
        .err()
        .map(|error| error.to_string())
        .ok_or("an unknown type was written")?;

    assert!(
        error.starts_with("commit type \"custom\" is unavailable"),
        "{error}"
    );
    assert!(written(&session).is_empty());
    Ok(())
}

#[test]
fn an_incomplete_draft_is_revalidated_before_writing() -> Result<(), Box<dyn Error>> {
    let session = staged(Some(built_in_project()?))?;
    let commit = session.commit();
    let context = commit.prepare_commit()?;
    let taxonomy = context.initial_taxonomy();
    let incomplete = CommitDraft::new(
        CommitTypeId::new("feat")?,
        None,
        CommitSubject::new("missing properties")?,
        Vec::new(),
    )?;
    let expected = taxonomy
        .validate(&incomplete)
        .into_message()
        .err()
        .ok_or("an incomplete feat draft validated")?;

    let error = commit
        .submit_commit(&AuthoredCommit::reviewed(
            taxonomy.id().clone(),
            incomplete,
            reviewed(taxonomy, "fix")?.reviewed_message().clone(),
        ))
        .err()
        .ok_or("an incomplete draft was written")?;

    assert_eq!(error, SubmitError::Failed(expected.to_string()));
    assert!(written(&session).is_empty());
    Ok(())
}

#[test]
fn a_writer_failure_is_reported_after_one_write_attempt() -> Result<(), Box<dyn Error>> {
    let session = staged(Some(built_in_project()?))?;
    let writer = FailingWriter(Cell::new(0));
    let commit = CommitServices::new(
        &session.locator,
        &session.project,
        &session.status,
        &writer,
        session.locator.root.as_path(),
    );
    let authored = reviewed(commit.prepare_commit()?.initial_taxonomy(), "feat")?;

    assert_eq!(
        commit
            .submit_commit(&authored)
            .err()
            .map(|error| error.to_string())
            .as_deref(),
        Some("writer failed")
    );
    assert_eq!(writer.0.get(), 1);
    Ok(())
}

struct FixedPaths(Vec<String>);

impl WorktreePaths for FixedPaths {
    fn paths(&self, _: &RepositoryRoot) -> Result<Vec<String>, String> {
        Ok(self.0.clone())
    }
}

struct FixedCommits(Vec<CitedCommit>);

impl WorktreePaths for FixedCommits {
    fn paths(&self, _: &RepositoryRoot) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }

    fn commits(&self, _: &RepositoryRoot) -> Result<Vec<CitedCommit>, String> {
        Ok(self.0.clone())
    }
}

struct BrokenPaths;

impl WorktreePaths for BrokenPaths {
    fn paths(&self, _: &RepositoryRoot) -> Result<Vec<String>, String> {
        Err("ls-files failed".into())
    }

    fn commits(&self, _: &RepositoryRoot) -> Result<Vec<CitedCommit>, String> {
        Err("git log failed".into())
    }
}

#[test]
fn cite_paths_follow_the_worktree_reader() -> Result<(), Box<dyn Error>> {
    let session = Session::new(Vec::new())?;
    let reader = FixedPaths(vec!["src/".into(), "src/net/retry.rs".into()]);
    let commit = session.commit().citing(&reader);
    assert_eq!(CommitWorkspace::cite_paths(&commit), reader.0);

    assert!(CommitWorkspace::cite_paths(&session.commit()).is_empty());
    assert!(CommitWorkspace::cite_commits(&session.commit()).is_empty());
    assert!(CommitWorkspace::cite_paths(&session.commit().citing(&BrokenPaths)).is_empty());
    assert!(CommitWorkspace::cite_commits(&session.commit().citing(&BrokenPaths)).is_empty());
    let history = FixedCommits(vec![CitedCommit {
        sha: "abc1234".into(),
        subject: "back off".into(),
    }]);
    assert_eq!(
        CommitWorkspace::cite_commits(&session.commit().citing(&history)),
        history.0
    );
    session.locator.failure.set(Some("not a repository"));
    assert!(CommitWorkspace::cite_paths(&session.commit().citing(&reader)).is_empty());
    assert!(CommitWorkspace::cite_commits(&session.commit().citing(&history)).is_empty());
    Ok(())
}

#[test]
fn a_custom_taxonomy_commits_with_its_own_schema() -> Result<(), Box<dyn Error>> {
    let ops = Taxonomy::new(
        TaxonomyId::new("ops")?,
        Some(TaxonomyVersion::V1),
        Description::new("Operations change classes.")?,
        None,
        vec![CommitTypeDefinition::new(
            Some(SchemaVersion::V1),
            CommitTypeId::new("provision")?,
            "Infrastructure provisioning.",
            vec![PropertyDefinition::new(
                PropertyKey::new("intent")?,
                "Why the infrastructure must change.",
                PropertyRequirement::Required,
            )?],
        )?],
    )?;
    let session = staged(Some(ProjectConfiguration::new(
        1,
        ops.id().clone(),
        vec![ops],
    )?))?;
    let commit = session.commit();
    let context = commit.prepare_commit()?;
    assert_eq!(
        context
            .initial_taxonomy()
            .definitions()
            .iter()
            .map(|definition| definition.id().clone())
            .collect::<Vec<_>>(),
        [CommitTypeId::new("provision")?]
    );

    commit.submit_commit(&reviewed(context.initial_taxonomy(), "provision")?)?;

    let message = &written(&session)[0];
    assert!(message.starts_with("provision: create durable commit\n"));
    assert!(message.contains("intent:\nauthored intent\n"));
    assert!(message.contains("Gitserious-Taxonomy: ops\n"));
    assert!(!message.contains("Gitserious-Template"));
    Ok(())
}

#[test]
fn mismatched_reviewed_bytes_never_reach_git() -> Result<(), Box<dyn Error>> {
    let session = staged(Some(built_in_project()?))?;
    let commit = session.commit();
    let context = commit.prepare_commit()?;
    let taxonomy = context.initial_taxonomy();
    let draft = complete_draft(taxonomy, "fix")?;
    let definition = taxonomy
        .definitions()
        .iter()
        .find(|definition| definition.id().as_str() == "fix")
        .ok_or("conventional has no fix")?;
    let built_in = built_in_taxonomies()
        .iter()
        .find(|built_in| built_in.id() == taxonomy.id())
        .ok_or("conventional is not built in")?
        .clone();
    let plain = validate_commit_draft(definition, &draft).into_message()?;
    let wrong_fingerprint = validate_commit_draft_with_provenance(
        &CommitProvenance::new(built_in, Fingerprint::from_bytes([0; 32])),
        &draft,
    )
    .into_message()?;
    let cases = [
        (
            AuthoredCommit::reviewed(taxonomy.id().clone(), draft.clone(), plain),
            "reviewed commit bytes do not match the selected taxonomy",
        ),
        (
            AuthoredCommit::reviewed(taxonomy.id().clone(), draft, wrong_fingerprint),
            "reviewed commit bytes do not match the selected taxonomy",
        ),
    ];

    for (authored, expected) in cases {
        assert_eq!(
            commit
                .submit_commit(&authored)
                .err()
                .map(|error| error.to_string())
                .as_deref(),
            Some(expected)
        );
    }
    assert!(written(&session).is_empty());
    Ok(())
}

#[test]
fn choosing_another_project_taxonomy_uses_its_schema_without_policy_writes()
-> Result<(), Box<dyn Error>> {
    let project = built_in_project()?;
    let session = staged(Some(project.clone()))?;
    let commit = session.commit();
    let context = commit.prepare_commit()?;
    assert_eq!(context.initial_taxonomy().id().as_str(), "conventional");
    assert_eq!(context.taxonomies().len(), built_in_taxonomies().len());
    let research = context
        .find_taxonomy(&TaxonomyId::new("research")?)
        .ok_or("research is not in the project")?;

    commit.submit_commit(&reviewed(research, "fix")?)?;

    let messages = written(&session);
    assert_eq!(messages.len(), 1);
    assert!(messages[0].contains("symptom:\n"));
    assert!(messages[0].contains("Gitserious-Taxonomy: research-v1\n"));
    assert_eq!(*session.project.0.borrow(), Ok(Some(project)));
    Ok(())
}

#[test]
fn unknown_taxonomies_and_cross_schema_drafts_never_reach_git() -> Result<(), Box<dyn Error>> {
    let session = staged(Some(built_in_project()?))?;
    let commit = session.commit();
    let context = commit.prepare_commit()?;
    let conventional = context.initial_taxonomy();
    let research = context
        .find_taxonomy(&TaxonomyId::new("research")?)
        .ok_or("research is not in the project")?;

    let research_fix = reviewed(research, "fix")?;
    let forged = AuthoredCommit::reviewed(
        TaxonomyId::new("global-only")?,
        research_fix.draft().clone(),
        research_fix.reviewed_message().clone(),
    );
    let error = commit
        .submit_commit(&forged)
        .err()
        .map(|error| error.to_string())
        .ok_or("an unknown taxonomy was written")?;
    assert!(
        error.starts_with("taxonomy \"global-only\" is unavailable"),
        "{error}"
    );

    let conventional_fix = reviewed(conventional, "fix")?;
    let cross_schema = AuthoredCommit::reviewed(
        research.id().clone(),
        conventional_fix.draft().clone(),
        conventional_fix.reviewed_message().clone(),
    );
    let expected = research
        .validate(cross_schema.draft())
        .into_message()
        .err()
        .ok_or("a conventional fix validated under research")?;
    assert_eq!(
        commit
            .submit_commit(&cross_schema)
            .err()
            .map(|error| error.to_string()),
        Some(expected.to_string())
    );
    assert!(written(&session).is_empty());
    Ok(())
}

#[test]
fn a_merge_cherry_pick_or_revert_stops_authoring_and_never_reaches_the_writer()
-> Result<(), Box<dyn Error>> {
    for (operation, command) in [
        (GitOperation::Merge, "merge"),
        (GitOperation::CherryPick, "cherry-pick"),
        (GitOperation::Revert, "revert"),
    ] {
        let session = Session::new(vec![entry("file", 'M', '.')])?;
        if let Ok(status) = session.status.0.borrow_mut().as_mut() {
            status.operation = Some(operation);
        }
        let commit = session.commit();

        let refusal = commit
            .prepare_commit()
            .err()
            .ok_or("authoring started during an operation")?;
        assert!(
            refusal.contains(&format!("`git {command} --continue`"))
                && refusal.contains(&format!("`git {command} --abort`")),
            "{refusal}"
        );
        assert_eq!(
            commit.submit_commit(&approved_project_commit()?).err(),
            Some(SubmitError::Operation(operation))
        );
        assert!(session.writer.0.borrow().is_empty(), "{command}");
    }
    Ok(())
}

#[test]
fn a_rebase_stop_authors_a_normal_commit_unless_it_stopped_on_conflicts()
-> Result<(), Box<dyn Error>> {
    let stopped = |entries| -> Result<Session, Box<dyn Error>> {
        let session = Session::new(entries)?;
        if let Ok(status) = session.status.0.borrow_mut().as_mut() {
            status.operation = Some(GitOperation::Rebase);
        }
        Ok(session)
    };

    let edit = stopped(vec![entry("file", 'M', '.')])?;
    let authored = approved(&edit.commit().prepare_commit()?)?;
    edit.commit().submit_commit(&authored)?;
    assert_eq!(edit.writer.0.borrow().len(), 1);

    let conflicted = stopped(vec![entry("a.rs", 'U', 'U'), entry("b.rs", 'U', 'U')])?;
    assert_eq!(
        conflicted.commit().prepare_commit().err().as_deref(),
        Some(
            "A rebase stopped on conflicts in a.rs, b.rs. Resolve them, then run \
             `git rebase --continue`."
        )
    );
    Ok(())
}

#[derive(Default)]
struct Index(RefCell<Vec<(RepositoryRoot, String)>>);

impl IndexWriter for Index {
    fn stage_tracked(&self, root: &RepositoryRoot) -> Result<(), String> {
        self.0.borrow_mut().push((root.clone(), "tracked".into()));
        Ok(())
    }
    fn stage(&self, root: &RepositoryRoot, paths: &[PathBuf]) -> Result<(), String> {
        self.0
            .borrow_mut()
            .push((root.clone(), format!("stage {paths:?}")));
        Ok(())
    }
    fn unstage(&self, root: &RepositoryRoot, paths: &[PathBuf]) -> Result<(), String> {
        self.0
            .borrow_mut()
            .push((root.clone(), format!("unstage {paths:?}")));
        Ok(())
    }
}

#[test]
fn staging_writes_the_located_repository_index_only_through_an_index_writer()
-> Result<(), Box<dyn Error>> {
    let session = Session::new(Vec::new())?;
    let unwired = session.commit();
    assert!(!unwired.can_stage());
    assert!(unwired.stage_tracked().is_err());
    assert!(unwired.stage(&["file".into()]).is_err());

    let index = Index::default();
    let commit = session.commit().staging(&index);
    assert!(commit.can_stage());
    commit.stage_tracked()?;
    commit.stage(&["file".into()])?;
    commit.unstage(&["new".into(), "old".into()])?;

    let root = session.locator.root.clone();
    assert_eq!(
        index.0.borrow().as_slice(),
        [
            (root.clone(), "tracked".to_owned()),
            (root.clone(), "stage [\"file\"]".to_owned()),
            (root, "unstage [\"new\", \"old\"]".to_owned()),
        ]
    );
    session.locator.failure.set(Some("locator unavailable"));
    assert_eq!(
        commit.stage_tracked().err().as_deref(),
        Some("locator unavailable")
    );
    Ok(())
}

#[derive(Default)]
struct Editor(RefCell<Vec<(RepositoryRoot, PathBuf)>>);

impl FileEditor for Editor {
    fn edit(&self, root: &RepositoryRoot, path: &Path) -> Result<(), String> {
        self.0.borrow_mut().push((root.clone(), path.to_path_buf()));
        Ok(())
    }
}

#[test]
fn only_a_policy_that_does_not_load_is_offered_to_the_editor() -> Result<(), Box<dyn Error>> {
    let session = Session::new(Vec::new())?;
    let editor = Editor::default();
    assert!(!session.commit().can_edit_policy(), "no editor is wired");
    assert!(
        !session.commit().editing(&editor).can_edit_policy(),
        "the policy loads"
    );

    *session.project.0.borrow_mut() = Err("invalid project configuration");
    let commit = session.commit().editing(&editor);
    assert!(commit.can_edit_policy());
    commit.edit_policy()?;

    assert_eq!(
        editor.0.borrow().as_slice(),
        [(
            session.locator.root.clone(),
            PathBuf::from("gitserious.toml")
        )]
    );
    Ok(())
}

#[derive(Default)]
struct Drafts(RefCell<Option<SavedDraft>>);

impl DraftStore for Drafts {
    fn load(&self, _: &RepositoryRoot) -> Result<Option<SavedDraft>, String> {
        Ok(self.0.borrow().clone())
    }
    fn save(&self, _: &RepositoryRoot, draft: &SavedDraft) -> Result<(), String> {
        *self.0.borrow_mut() = Some(draft.clone());
        Ok(())
    }
    fn discard(&self, _: &RepositoryRoot) -> Result<(), String> {
        *self.0.borrow_mut() = None;
        Ok(())
    }
}

#[test]
fn drafts_are_kept_only_through_a_draft_store() -> Result<(), Box<dyn Error>> {
    let session = Session::new(Vec::new())?;
    let draft = SavedDraft {
        taxonomy: "team".into(),
        fingerprint: "sha256:00".into(),
        commit_type: "note".into(),
        subject: ": kept".into(),
        properties: Vec::new(),
        breaking_change: String::new(),
    };
    let unwired = session.commit();
    assert!(!unwired.keeps_drafts());
    unwired.save_draft(&draft)?;
    assert_eq!(unwired.saved_draft(), None);

    let drafts = Drafts::default();
    let commit = session.commit().drafting(&drafts);
    assert!(commit.keeps_drafts());
    commit.save_draft(&draft)?;
    assert_eq!(commit.saved_draft(), Some(draft));
    commit.discard_draft()?;
    assert_eq!(commit.saved_draft(), None);
    Ok(())
}
