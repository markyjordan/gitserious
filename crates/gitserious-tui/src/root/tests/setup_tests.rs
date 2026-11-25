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
