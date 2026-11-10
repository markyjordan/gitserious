use std::cell::{Cell, RefCell};
use std::error::Error;
use std::path::{Path, PathBuf};

use gitserious_app::{
    AuthoredCommit, BranchState, CommitAuthoringContext, CommitOutput, CommitServices,
    CommitTaxonomy, CommitWorkspace, CommitWriter, GlobalConfiguration, GlobalConfigurationStore,
    InteractiveUi, InteractiveWorkspace, ProjectConfiguration, ProjectConfigurationStore,
    ProjectRevision, RepositoryInitializer, RepositoryLocator, RepositoryRoot, RepositoryStatus,
    RepositoryStatusReader, StatusEntry, StoredProject, run_interactive,
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
    fn run(&self, workspace: &dyn InteractiveWorkspace) -> Result<(), String> {
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
        )?;
        Ok(())
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
        let commit = session.commit();
        let preparation = commit.prepare_commit();
        if let Some(blocker) = blocker {
            let error = preparation
                .err()
                .ok_or("ineligible preparation succeeded")?;
            assert!(error.contains(blocker), "{name}: {error}");
            let error = commit
                .submit_commit(&approved_project_commit()?)
                .err()
                .ok_or("ineligible submission succeeded")?;
            assert!(error.contains(blocker), "{name}: {error}");
            assert!(session.writer.0.borrow().is_empty(), "{name}");
        } else {
            let authored = approved(&preparation?)?;
            let output = commit.submit_commit(&authored)?;
            assert_eq!(output.stdout(), b"created\n", "{name}");
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
            commit
                .submit_commit(&approved_project_commit()?)
                .err()
                .as_deref(),
            Some(failure)
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
    session.run(|workspace| {
        let authored = approved(&workspace.commit().prepare_commit()?)?;
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
        assert_eq!(
            workspace.configuration().library_taxonomies()?,
            vec![changed]
        );
        workspace.commit().submit_commit(&authored)?;
        assert_eq!(
            session.writer.0.borrow().as_slice(),
            &[(
                session.locator.root.clone(),
                authored.reviewed_message().clone()
            )]
        );
        Ok(())
    })
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
        commit.submit_commit(&approved_project_commit()?).err(),
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

    assert_eq!(error, expected.to_string());
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
        commit.submit_commit(&authored).err().as_deref(),
        Some("writer failed")
    );
    assert_eq!(writer.0.get(), 1);
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
            commit.submit_commit(&authored).err().as_deref(),
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
        commit.submit_commit(&cross_schema).err(),
        Some(expected.to_string())
    );
    assert!(written(&session).is_empty());
    Ok(())
}
