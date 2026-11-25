use super::*;
use gitserious_app::SavedDraft;

/// A workspace with staged changes whose draft store is one slot, recording each write.
#[derive(Default)]
struct Drafts {
    slot: RefCell<Option<SavedDraft>>,
    writes: RefCell<Vec<&'static str>>,
}

impl ConfigurationWorkspace for Drafts {
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

impl ProjectWorkspace for Drafts {
    fn overview(&self) -> ProjectOverview {
        Ready.overview()
    }
    fn initialize(&self, configuration: &ProjectConfiguration) -> Result<(), String> {
        Services.initialize(configuration)
    }
}

impl CommitWorkspace for Drafts {
    fn repository_status(&self) -> Result<RepositoryStatus, String> {
        Ready.repository_status()
    }
    fn prepare_commit(&self) -> Result<CommitAuthoringContext, String> {
        Ready.prepare_commit()
    }
    fn submit_commit(
        &self,
        authored: &gitserious_app::AuthoredCommit,
    ) -> Result<CommitOutput, SubmitError> {
        Ready.submit_commit(authored)
    }
    fn keeps_drafts(&self) -> bool {
        true
    }
    fn saved_draft(&self) -> Option<SavedDraft> {
        self.slot.borrow().clone()
    }
    fn save_draft(&self, draft: &SavedDraft) -> Result<(), String> {
        self.writes.borrow_mut().push("save");
        *self.slot.borrow_mut() = Some(draft.clone());
        Ok(())
    }
    fn discard_draft(&self) -> Result<(), String> {
        self.writes.borrow_mut().push("discard");
        *self.slot.borrow_mut() = None;
        Ok(())
    }
}

fn send(root: &mut Root, drafts: &Drafts, events: impl IntoIterator<Item = Event>) {
    for event in events {
        root.session_event(event, of(drafts), &[]);
    }
}

/// Launches Commit, which opens the picker on the staged changes, and writes a feat draft.
fn written(drafts: &Drafts) -> Root {
    let mut root = Root::new(InitialPage::Commit, of(drafts), &[]);
    send(
        &mut root,
        drafts,
        [
            key(KeyCode::Enter),
            Event::Paste("(tui): keep drafts".into()),
            key(KeyCode::Down),
            Event::Paste("Leaving should not lose a message.".into()),
        ],
    );
    root
}

#[test]
fn quitting_mid_draft_keeps_it_and_the_next_commit_offers_it_back_intact()
-> Result<(), Box<dyn std::error::Error>> {
    let drafts = Drafts::default();
    let mut root = written(&drafts);
    let quit = root.session_event(
        modified(KeyCode::Char('c'), KeyModifiers::CONTROL),
        of(&drafts),
        &[],
    );
    assert_eq!(quit, Some(SessionRequest::Quit), "no discard question");
    let saved = drafts.slot.borrow().clone().ok_or("the draft was saved")?;
    assert_eq!(saved.subject, "(tui): keep drafts");
    assert_eq!(
        saved.properties[0].text,
        "Leaving should not lose a message."
    );

    let mut next = Root::new(InitialPage::Commit, of(&drafts), &[]);
    assert!(
        !next.commit.editing(),
        "the offer waits instead of the picker"
    );
    let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
    terminal.draw(|frame| next.render(frame, &[]))?;
    let page = rendered(&terminal);
    assert!(page.contains("[enter] resume draft"), "{page}");
    assert!(
        page.contains("Saved draft: feat(tui): keep drafts"),
        "{page}"
    );

    send(&mut next, &drafts, [key(KeyCode::Enter)]);
    terminal.draw(|frame| next.render(frame, &[]))?;
    let composer = rendered(&terminal);
    assert!(composer.contains("feat(tui): keep drafts"), "{composer}");
    assert!(
        composer.contains("Leaving should not lose a message."),
        "{composer}"
    );

    send(
        &mut next,
        &drafts,
        [key(KeyCode::Esc), key(KeyCode::Char('y'))],
    );
    assert!(
        next.commit.editing(),
        "the picker opens to choose another type"
    );
    assert!(
        drafts.slot.borrow().is_none(),
        "discarding a resumed draft deletes it: {:?}",
        drafts.writes.borrow()
    );
    Ok(())
}

#[test]
fn a_confirmed_discard_or_a_commit_deletes_the_draft() {
    let drafts = Drafts::default();
    let mut root = written(&drafts);
    send(&mut root, &drafts, [Event::FocusLost]);
    assert!(drafts.slot.borrow().is_some());

    send(
        &mut root,
        &drafts,
        [key(KeyCode::Esc), key(KeyCode::Char('y'))],
    );
    assert!(
        drafts.slot.borrow().is_none(),
        "{:?}",
        drafts.writes.borrow()
    );

    let mut root = written(&drafts);
    send(&mut root, &drafts, [Event::FocusLost]);
    root.commit.conclude_submission(
        &drafts,
        Ok(CommitOutput::new(b"created\n".to_vec(), Vec::new())),
    );
    assert!(drafts.slot.borrow().is_none(), "a commit ends the draft");
}
