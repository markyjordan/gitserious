use super::*;
use gitserious_app::MissingWorkTree;

fn page(root: &mut Root) -> Result<String, Box<dyn std::error::Error>> {
    let mut terminal = Terminal::new(TestBackend::new(80, 24))?;
    terminal.draw(|frame| root.render(frame, &[]))?;
    Ok(rendered(&terminal))
}

fn commit_on(workspace: &HomeWorkspace) -> Root {
    Root::new(InitialPage::Commit, of(workspace), &[])
}

#[test]
fn a_repository_without_gitserious_toml_initializes_from_commit_and_comes_back()
-> Result<(), Box<dyn std::error::Error>> {
    let workspace = HomeWorkspace::new(ProjectOverview::Uninitialized { root: fresh_root() });
    let mut root = commit_on(&workspace);
    let text = page(&mut root)?;
    assert!(text.contains("[enter] initialize gitserious"), "{text}");
    assert!(text.contains("gitserious.toml is missing"), "{text}");

    root.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
    assert_eq!(root.section, Section::Home);
    assert!(root.home.editing());
    root.handle_event(key(KeyCode::Enter), of(&workspace), &[]);

    assert!(workspace.created.borrow().is_some(), "gitserious init ran");
    assert_eq!(root.section, Section::Commit);
    assert_eq!(root.commit.blocker(), None);
    assert!(page(&mut root)?.contains("[enter] start commit"));
    Ok(())
}

#[test]
fn no_repository_offers_git_and_gitserious_and_esc_comes_back_without_writing()
-> Result<(), Box<dyn std::error::Error>> {
    let workspace = HomeWorkspace::new(ProjectOverview::NoRepository {
        directory: fresh_root().as_path().to_path_buf(),
    });
    let mut root = commit_on(&workspace);
    let text = page(&mut root)?;
    assert!(
        text.contains("[enter] initialize git and gitserious"),
        "{text}"
    );
    assert!(text.contains("Directory:"), "{text}");

    root.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
    assert!(root.home.editing());
    root.handle_event(key(KeyCode::Esc), of(&workspace), &[]);

    assert_eq!(root.section, Section::Commit);
    assert!(workspace.created.borrow().is_none());
    assert!(page(&mut root)?.contains("[enter] initialize git and gitserious"));
    Ok(())
}

#[test]
fn a_directory_git_cannot_commit_from_says_why_and_offers_nothing()
-> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        (MissingWorkTree::HomeOrRoot, "Open a project folder"),
        (MissingWorkTree::Bare, "bare repository"),
        (
            MissingWorkTree::InsideGitDir {
                worktree: Some("/work/tree".into()),
            },
            "Its working tree is /work/tree",
        ),
        (
            MissingWorkTree::DubiousOwnership {
                repository: "/srv/shared".into(),
            },
            "git config --global --add safe.directory /srv/shared",
        ),
    ];
    for (missing, says) in cases {
        let workspace = HomeWorkspace::new(ProjectOverview::Unavailable {
            directory: fresh_root().as_path().to_path_buf(),
            error: "fatal: this operation must be run in a work tree".into(),
            missing: Some(missing.clone()),
        });
        let mut root = commit_on(&workspace);
        let text = page(&mut root)?;
        assert!(
            text.contains("Start commit unavailable"),
            "{missing:?}: {text}"
        );
        assert!(
            text.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .contains(says),
            "{missing:?}: {text}"
        );

        root.handle_event(key(KeyCode::Enter), of(&workspace), &[]);
        assert_eq!(root.section, Section::Commit, "{missing:?}");
        assert!(!root.home.editing(), "{missing:?}");
    }
    Ok(())
}

/// A repository whose `gitserious.toml` exists but does not load.
struct BrokenPolicy;

impl ConfigurationWorkspace for BrokenPolicy {
    fn load_library(&self) -> Result<LibraryDraft, String> {
        Services.load_library()
    }
    fn save_library(&self, library: &LibraryDraft) -> Result<LibraryDraft, String> {
        Services.save_library(library)
    }
    fn load_project(&self) -> Result<ProjectSnapshot, String> {
        Services.load_project()
    }
    fn library_taxonomies(&self) -> Result<Vec<gitserious_core::Taxonomy>, String> {
        Services.library_taxonomies()
    }
    fn library_path(&self) -> std::path::PathBuf {
        Services.library_path()
    }
}

impl ProjectWorkspace for BrokenPolicy {
    fn overview(&self) -> ProjectOverview {
        ProjectOverview::Unavailable {
            directory: fresh_root().as_path().to_path_buf(),
            error: BROKEN.into(),
            missing: None,
        }
    }
    fn initialize(&self, configuration: &ProjectConfiguration) -> Result<(), String> {
        Services.initialize(configuration)
    }
}

const BROKEN: &str = "invalid project configuration in gitserious.toml at line 3, column 1: \
                      unexpected `=`";

impl CommitWorkspace for BrokenPolicy {
    fn repository_status(&self) -> Result<RepositoryStatus, String> {
        Services.repository_status()
    }
    fn prepare_commit(&self) -> Result<CommitAuthoringContext, String> {
        Err(BROKEN.into())
    }
    fn submit_commit(
        &self,
        authored: &gitserious_app::AuthoredCommit,
    ) -> Result<CommitOutput, SubmitError> {
        Services.submit_commit(authored)
    }
    fn can_edit_policy(&self) -> bool {
        true
    }
}

#[test]
fn e_hands_a_broken_policy_to_the_editor_only_where_it_is_offered()
-> Result<(), Box<dyn std::error::Error>> {
    let mut broken = Root::new(InitialPage::Commit, of(&BrokenPolicy), &[]);
    let text = page(&mut broken)?;
    assert!(text.contains("[e] edit gitserious.toml"), "{text}");
    assert!(text.contains("at line 3, column 1"), "{text}");
    assert!(
        broken
            .commit
            .hints()
            .contains(&("e", "edit gitserious.toml"))
    );
    assert_eq!(
        broken.session_event(key(KeyCode::Char('e')), of(&BrokenPolicy), &[]),
        Some(SessionRequest::EditPolicy)
    );

    let mut valid = on_commit_page(&Services);
    assert!(!valid.commit.hints().iter().any(|(key, _)| *key == "e"));
    assert_eq!(
        valid.session_event(key(KeyCode::Char('e')), of(&Services), &[]),
        None
    );
    Ok(())
}
