use super::*;

pub(super) fn of<T>(services: &T) -> Capabilities<'_>
where
    T: CommitWorkspace + ConfigurationWorkspace + ProjectWorkspace,
{
    Capabilities {
        commit: services,
        config: services,
        project: services,
    }
}

pub(super) struct Services;
impl ConfigurationWorkspace for Services {
    fn load_library(&self) -> Result<LibraryDraft, String> {
        Ok(LibraryDraft::from_configuration(
            GlobalConfiguration::default(),
        ))
    }
    fn save_library(&self, _: &LibraryDraft) -> Result<LibraryDraft, String> {
        Err("test save unavailable".into())
    }
    fn load_project(&self) -> Result<ProjectSnapshot, String> {
        Ok(ProjectSnapshot::new(
            RepositoryRoot::new(std::env::temp_dir()).map_err(|error| error.to_string())?,
            None,
        ))
    }
    fn library_taxonomies(&self) -> Result<Vec<gitserious_core::Taxonomy>, String> {
        Ok(Vec::new())
    }
    fn library_path(&self) -> std::path::PathBuf {
        "/home/tester/.config/gitserious/config.toml".into()
    }
}
impl ProjectWorkspace for Services {
    fn overview(&self) -> ProjectOverview {
        ProjectOverview::Uninitialized { root: fresh_root() }
    }
    fn initialize(&self, _: &ProjectConfiguration) -> Result<(), String> {
        Err("test initialize unavailable".into())
    }
}

impl CommitWorkspace for Services {
    fn repository_status(&self) -> Result<RepositoryStatus, String> {
        Ok(RepositoryStatus {
            root: RepositoryRoot::new(std::env::temp_dir()).map_err(|error| error.to_string())?,
            branch: BranchState::Unborn("main".into()),
            entries: Vec::new(),
        })
    }
    fn prepare_commit(&self) -> Result<CommitAuthoringContext, String> {
        Err("Stage changes with Git".into())
    }
    fn submit_commit(&self, _: &gitserious_app::AuthoredCommit) -> Result<CommitOutput, String> {
        Err("test submit unavailable".into())
    }
}

pub(super) struct Unavailable;

impl ConfigurationWorkspace for Unavailable {
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

impl ProjectWorkspace for Unavailable {
    fn overview(&self) -> ProjectOverview {
        Services.overview()
    }
    fn initialize(&self, configuration: &ProjectConfiguration) -> Result<(), String> {
        Services.initialize(configuration)
    }
}

impl CommitWorkspace for Unavailable {
    fn repository_status(&self) -> Result<RepositoryStatus, String> {
        Err("repository unavailable".into())
    }
    fn prepare_commit(&self) -> Result<CommitAuthoringContext, String> {
        Err("repository unavailable".into())
    }
    fn submit_commit(
        &self,
        authored: &gitserious_app::AuthoredCommit,
    ) -> Result<CommitOutput, String> {
        CommitWorkspace::submit_commit(&Services, authored)
    }
}

pub(super) fn fresh_root() -> RepositoryRoot {
    let filesystem_root = std::env::temp_dir()
        .ancestors()
        .last()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    RepositoryRoot::new(filesystem_root.join("fresh"))
        .unwrap_or_else(|error| unreachable!("the filesystem root is absolute: {error}"))
}

/// A workspace whose Home state follows initialization.
pub(super) struct HomeWorkspace {
    pub(super) state: RefCell<ProjectOverview>,
    pub(super) created: RefCell<Option<ProjectConfiguration>>,
    pub(super) failure: Option<&'static str>,
    pub(super) library: Result<Vec<gitserious_core::Taxonomy>, String>,
}

impl HomeWorkspace {
    pub(super) fn new(state: ProjectOverview) -> Self {
        Self {
            state: RefCell::new(state),
            created: RefCell::new(None),
            failure: None,
            library: Ok(Vec::new()),
        }
    }
}

impl ConfigurationWorkspace for HomeWorkspace {
    fn load_library(&self) -> Result<LibraryDraft, String> {
        Services.load_library()
    }
    fn save_library(&self, _: &LibraryDraft) -> Result<LibraryDraft, String> {
        Err("Project preview must never save.".into())
    }
    fn load_project(&self) -> Result<ProjectSnapshot, String> {
        let state = self.state.borrow();
        let root = match &*state {
            ProjectOverview::Uninitialized { root } | ProjectOverview::Initialized { root, .. } => {
                root.clone()
            }
            _ => return Err("No Git repository.".into()),
        };
        Ok(ProjectSnapshot::new(root, self.created.borrow().clone()))
    }
    fn library_taxonomies(&self) -> Result<Vec<gitserious_core::Taxonomy>, String> {
        self.library.clone()
    }
    fn library_path(&self) -> std::path::PathBuf {
        Services.library_path()
    }
}

impl CommitWorkspace for HomeWorkspace {
    fn repository_status(&self) -> Result<RepositoryStatus, String> {
        Services.repository_status()
    }
    fn prepare_commit(&self) -> Result<CommitAuthoringContext, String> {
        Services.prepare_commit()
    }
    fn submit_commit(
        &self,
        authored: &gitserious_app::AuthoredCommit,
    ) -> Result<CommitOutput, String> {
        Services.submit_commit(authored)
    }
}
impl ProjectWorkspace for HomeWorkspace {
    fn overview(&self) -> ProjectOverview {
        self.state.borrow().clone()
    }
    fn initialize(&self, configuration: &ProjectConfiguration) -> Result<(), String> {
        if let Some(failure) = self.failure {
            return Err(failure.into());
        }
        *self.state.borrow_mut() = ProjectOverview::Initialized {
            root: fresh_root(),
            default: configuration.default_taxonomy().clone(),
            version: Some(TaxonomyVersion::V1),
            policy_untracked: true,
        };
        *self.created.borrow_mut() = Some(configuration.clone());
        Ok(())
    }
}

pub(super) fn home_screen(
    workspace: &HomeWorkspace,
    width: u16,
    height: u16,
) -> Result<(Root, Terminal<TestBackend>), Box<dyn std::error::Error>> {
    let mut root = Root::new(InitialPage::Home, of(workspace), &[]);
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    terminal.draw(|frame| root.render(frame, &[]))?;
    Ok((root, terminal))
}

pub(super) fn row_of(text: &str, needle: &str) -> Option<usize> {
    text.lines().position(|row| row.contains(needle))
}

pub(super) fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
pub(super) fn span(text: &str, strong: bool) -> HelpSpan {
    HelpSpan {
        text: text.into(),
        strong,
    }
}

pub(super) fn numbered(count: usize) -> Vec<HelpLine> {
    (1..=count)
        .map(|number| vec![span(&format!("line {number}"), false)])
        .collect()
}

/// The column and row where `needle` first appears on screen.
pub(super) fn position(terminal: &Terminal<TestBackend>, needle: &str) -> Option<(u16, u16)> {
    rendered(terminal).lines().enumerate().find_map(|(y, row)| {
        let x = row.find(needle)?;
        Some((
            u16::try_from(row[..x].chars().count()).ok()?,
            u16::try_from(y).ok()?,
        ))
    })
}

pub(super) fn rendered(terminal: &Terminal<TestBackend>) -> String {
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(usize::from(terminal.size().map_or(60, |size| size.width)))
        .map(|row| {
            row.iter()
                .map(ratatui::buffer::Cell::symbol)
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The four root pages, with `overview` numbered lines on the first.
pub(super) fn pages(overview: usize) -> Vec<HelpPage> {
    vec![
        HelpPage::new("Overview", None, move |_| numbered(overview)),
        HelpPage::new("Init", Some("init"), |_| numbered(1)),
        HelpPage::new("Config", Some("config"), |_| numbered(1)),
        HelpPage::new("Commit", Some("commit"), |_| numbered(1)),
    ]
}

/// The first content row, right of the sidebar's rule.
pub(super) fn top(terminal: &Terminal<TestBackend>) -> Option<String> {
    rendered(terminal)
        .lines()
        .nth(2)
        .and_then(|row| row.split('│').nth(1))
        .map(|content| content.trim().to_owned())
}

pub(super) fn footer(terminal: &Terminal<TestBackend>) -> String {
    rendered(terminal)
        .lines()
        .last()
        .unwrap_or_default()
        .to_owned()
}

pub(super) fn click(area: Rect) -> Event {
    Event::Mouse(event::MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: area.x + 1,
        row: area.y,
        modifiers: KeyModifiers::NONE,
    })
}

/// A workspace whose staged changes are ready to author.
pub(super) struct Ready;

impl ConfigurationWorkspace for Ready {
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

impl ProjectWorkspace for Ready {
    fn overview(&self) -> ProjectOverview {
        Services.overview()
    }
    fn initialize(&self, configuration: &ProjectConfiguration) -> Result<(), String> {
        Services.initialize(configuration)
    }
}

impl CommitWorkspace for Ready {
    fn repository_status(&self) -> Result<RepositoryStatus, String> {
        Services.repository_status()
    }
    fn prepare_commit(&self) -> Result<CommitAuthoringContext, String> {
        commit::reviewed_submission().map(|(context, _, _)| context)
    }
    fn submit_commit(
        &self,
        authored: &gitserious_app::AuthoredCommit,
    ) -> Result<CommitOutput, String> {
        Services.submit_commit(authored)
    }
}

pub(super) const OPEN: [&str; 4] = ["Open Home", "Open Commit", "Open Config", "Open Help"];

pub(super) fn palette(root: &Root) -> Vec<String> {
    root.commands()
        .into_iter()
        .map(|(label, _)| label)
        .collect()
}

pub(super) fn listing(extra: &[&str]) -> Vec<String> {
    OPEN.iter()
        .chain(extra)
        .chain(&["Quit"])
        .map(|label| (*label).to_owned())
        .collect()
}

/// Runs the palette command whose label contains `query`.
pub(super) fn run(
    root: &mut Root,
    workspace: &(impl CommitWorkspace + ConfigurationWorkspace + ProjectWorkspace),
    query: &str,
) -> Option<Action> {
    root.handle_event(key(KeyCode::Char('/')), of(workspace), &[]);
    root.handle_event(Event::Paste(query.into()), of(workspace), &[]);
    root.handle_event(key(KeyCode::Enter), of(workspace), &[])
}

/// A root on Commit that retains a rejected review, with `workspace` behind it.
pub(super) fn rejected_review_with(
    workspace: &(impl CommitWorkspace + ConfigurationWorkspace + ProjectWorkspace),
) -> Result<Root, Box<dyn std::error::Error>> {
    let (context, _, checkpoint) = commit::reviewed_submission()?;
    let mut root = Root::new(InitialPage::Commit, of(workspace), &[]);
    root.commit.conclude_authoring(
        workspace,
        Some((context, checkpoint, Err("hook rejected".into()))),
    );
    Ok(root)
}

/// A root on `section` that retains a reviewed commit Git rejected.
pub(super) fn rejected_review_on(section: Section) -> Result<Root, Box<dyn std::error::Error>> {
    let (context, _, checkpoint) = commit::reviewed_submission()?;
    let mut root = Root::new(InitialPage::Commit, of(&Services), &[]);
    root.commit.conclude_authoring(
        &Services,
        Some((context, checkpoint, Err("hook rejected".into()))),
    );
    root.execute(Action::Select(section), of(&Services));
    assert_eq!(root.section, section);
    Ok(root)
}

pub(super) fn commit_page(root: &Root) -> (Option<String>, Option<RepositoryStatus>) {
    (
        root.commit.blocker().map(str::to_owned),
        root.commit.snapshot().cloned(),
    )
}
