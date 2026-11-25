use super::*;
use gitserious_app::StatusEntry;
use std::path::PathBuf;

/// A Commit workspace whose index the staging keys write, recording each write.
struct Staging {
    entries: RefCell<Vec<StatusEntry>>,
    writes: RefCell<Vec<String>>,
    writable: bool,
}

impl Staging {
    fn new(entries: Vec<StatusEntry>) -> Self {
        Self {
            entries: RefCell::new(entries),
            writes: RefCell::new(Vec::new()),
            writable: true,
        }
    }

    fn writes(&self) -> Vec<String> {
        self.writes.borrow().clone()
    }

    fn record(&self, write: &str, paths: &[PathBuf]) {
        let paths: Vec<_> = paths.iter().map(|path| path.to_string_lossy()).collect();
        self.writes
            .borrow_mut()
            .push(format!("{write} {}", paths.join(" ")).trim_end().to_owned());
    }
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

impl ConfigurationWorkspace for Staging {
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

impl ProjectWorkspace for Staging {
    fn overview(&self) -> ProjectOverview {
        Services.overview()
    }
    fn initialize(&self, configuration: &ProjectConfiguration) -> Result<(), String> {
        Services.initialize(configuration)
    }
}

impl CommitWorkspace for Staging {
    fn repository_status(&self) -> Result<RepositoryStatus, String> {
        Ok(RepositoryStatus {
            entries: self.entries.borrow().clone(),
            ..Services.repository_status()?
        })
    }
    fn prepare_commit(&self) -> Result<CommitAuthoringContext, String> {
        commit::built_in_context()
    }
    fn submit_commit(
        &self,
        authored: &gitserious_app::AuthoredCommit,
    ) -> Result<CommitOutput, SubmitError> {
        Services.submit_commit(authored)
    }
    fn can_stage(&self) -> bool {
        self.writable
    }
    fn stage_tracked(&self) -> Result<(), String> {
        self.record("stage tracked", &[]);
        for entry in self.entries.borrow_mut().iter_mut() {
            if !entry.untracked && entry.worktree != '.' {
                entry.index = entry.worktree;
                entry.worktree = '.';
            }
        }
        Ok(())
    }
    fn stage(&self, paths: &[PathBuf]) -> Result<(), String> {
        self.record("stage", paths);
        for entry in self.entries.borrow_mut().iter_mut() {
            if paths.contains(&entry.path) {
                entry.index = if entry.untracked { 'A' } else { entry.worktree };
                entry.worktree = '.';
                entry.untracked = false;
            }
        }
        Ok(())
    }
    fn unstage(&self, paths: &[PathBuf]) -> Result<(), String> {
        self.record("unstage", paths);
        Ok(())
    }
}

/// The page's footer keys, as the root draws them.
fn hinted(root: &Root) -> Vec<&'static str> {
    root.commit
        .hints()
        .into_iter()
        .map(|(key, _)| key)
        .collect()
}

fn press(root: &mut Root, workspace: &Staging, code: KeyCode) -> Option<Action> {
    root.handle_event(key(code), of(workspace), &[])
}

#[test]
fn each_row_takes_space_exactly_when_its_footer_hints_it() {
    let rows = [
        (None, "Staged (2)"),
        (Some("unstage a.rs"), "a.rs"),
        (None, "conflicted d.rs under Staged"),
        (None, "Unstaged (2)"),
        (Some("stage b.rs"), "b.rs"),
        (None, "conflicted d.rs under Unstaged"),
        (None, "Untracked (1)"),
        (Some("stage c.txt"), "c.txt"),
    ];
    for (row, (write, name)) in rows.into_iter().enumerate() {
        let workspace = Staging::new(vec![
            entry("a.rs", 'M', '.'),
            entry("b.rs", '.', 'M'),
            entry("c.txt", '?', '?'),
            entry("d.rs", 'U', 'U'),
        ]);
        let mut root = on_commit_page(&workspace);
        for _ in 0..row {
            press(&mut root, &workspace, KeyCode::Down);
        }

        assert_eq!(hinted(&root).contains(&"space"), write.is_some(), "{name}");
        assert_eq!(press(&mut root, &workspace, KeyCode::Char(' ')), None);
        assert_eq!(
            workspace.writes(),
            write.into_iter().map(str::to_owned).collect::<Vec<_>>(),
            "{name}"
        );
    }
}

#[test]
fn space_stages_an_untracked_file_alone_and_the_page_refreshes() {
    let workspace = Staging::new(vec![entry("c.txt", '?', '?'), entry("e.txt", '?', '?')]);
    let mut root = on_commit_page(&workspace);
    for _ in 0..3 {
        press(&mut root, &workspace, KeyCode::Down);
    }

    press(&mut root, &workspace, KeyCode::Char(' '));

    assert_eq!(workspace.writes(), ["stage c.txt"]);
    let snapshot = root.commit.snapshot().cloned().unwrap_or_else(|| {
        unreachable!("the page read status");
    });
    assert!(snapshot.has_staged_changes(), "the page re-read status");
    assert_eq!(
        snapshot
            .entries
            .iter()
            .filter(|entry| entry.untracked)
            .count(),
        1
    );
}

#[test]
fn space_unstages_both_sides_of_a_staged_rename() {
    let workspace = Staging::new(vec![StatusEntry {
        original_path: Some("old.rs".into()),
        ..entry("new.rs", 'R', '.')
    }]);
    let mut root = on_commit_page(&workspace);
    press(&mut root, &workspace, KeyCode::Down);

    press(&mut root, &workspace, KeyCode::Char(' '));

    assert_eq!(workspace.writes(), ["unstage new.rs old.rs"]);
}

#[test]
fn a_stages_tracked_changes_only_when_some_exist() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = Staging::new(vec![entry("b.rs", '.', 'M'), entry("c.txt", '?', '?')]);
    let mut root = on_commit_page(&workspace);
    let page = rendered(&{
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|frame| root.render(frame, &[]))?;
        terminal
    });
    assert!(
        page.contains("[a] stage all tracked changes  [enter] start commit"),
        "{page}"
    );
    assert!(hinted(&root).contains(&"a"));

    press(&mut root, &workspace, KeyCode::Char('a'));
    assert_eq!(workspace.writes(), ["stage tracked"]);

    assert!(
        !hinted(&root).contains(&"a"),
        "only an untracked file is left"
    );
    press(&mut root, &workspace, KeyCode::Char('a'));
    assert_eq!(workspace.writes(), ["stage tracked"]);
    Ok(())
}

#[test]
fn a_page_that_cannot_write_the_index_offers_no_staging_keys() {
    let workspace = Staging {
        writable: false,
        ..Staging::new(vec![entry("b.rs", '.', 'M'), entry("c.txt", '?', '?')])
    };
    let mut root = on_commit_page(&workspace);
    for _ in 0..4 {
        press(&mut root, &workspace, KeyCode::Down);
    }

    assert!(!hinted(&root).contains(&"space"));
    assert!(!hinted(&root).contains(&"a"));
    press(&mut root, &workspace, KeyCode::Char(' '));
    press(&mut root, &workspace, KeyCode::Char('a'));
    assert!(workspace.writes().is_empty());
}

#[test]
fn s_leaves_the_picker_for_the_page() {
    let workspace = Staging::new(vec![entry("a.rs", 'M', '.')]);
    let mut root = on_commit_page(&workspace);
    press(&mut root, &workspace, KeyCode::Enter);
    assert!(root.commit.editing());
    assert!(hinted(&root).contains(&"s"));

    press(&mut root, &workspace, KeyCode::Char('s'));

    assert!(!root.commit.editing());
    assert!(!root.commit.has_pending_review());
}

#[test]
fn s_keeps_a_review_with_nothing_staged_until_enter_resumes_it() {
    let workspace = Staging::new(vec![entry("b.rs", '.', 'M')]);
    let mut root = on_commit_page(&workspace);
    for event in [
        key(KeyCode::Enter),
        key(KeyCode::Enter),
        Event::Paste(": stage from review".into()),
        key(KeyCode::Down),
    ] {
        root.handle_event(event, of(&workspace), &[]);
    }
    assert!(
        !hinted(&root).contains(&"s"),
        "the composer takes s as text"
    );
    for event in [
        Event::Paste("reviewed intent".into()),
        key(KeyCode::Down),
        Event::Paste("reviewed decision".into()),
        key(KeyCode::Down),
        modified(KeyCode::Char('n'), KeyModifiers::ALT),
        modified(KeyCode::Char('s'), KeyModifiers::CONTROL),
    ] {
        root.handle_event(event, of(&workspace), &[]);
    }
    assert!(hinted(&root).contains(&"s"), "nothing is staged");

    press(&mut root, &workspace, KeyCode::Char('s'));
    assert!(!root.commit.editing());
    assert!(root.commit.has_pending_review());
    press(&mut root, &workspace, KeyCode::Char('a'));
    assert_eq!(workspace.writes(), ["stage tracked"]);

    press(&mut root, &workspace, KeyCode::Enter);
    assert!(root.commit.editing());
    assert!(!hinted(&root).contains(&"s"), "something is staged now");
    assert_eq!(
        press(&mut root, &workspace, KeyCode::Enter),
        Some(Action::Commit(CommitRequest::Submit))
    );
    assert!(root.commit.approved().is_some_and(|authored| {
        authored
            .reviewed_message()
            .as_str()
            .starts_with("feat: stage from review")
    }));
}
