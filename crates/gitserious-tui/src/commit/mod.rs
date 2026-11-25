mod author;
mod status_tree;

use std::io;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gitserious_app::{
    AuthoredCommit, BranchState, CommitOutput, CommitWorkspace, MissingWorkTree, ProjectOverview,
    RepositoryStatus, SavedDraft, SubmitError,
};
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::shared::paths::path_lines;
use crate::shared::work_tree::guidance;
use crate::theme::{command_hints, frame_style, theme};

/// The project policy file at the repository root.
const POLICY_FILE: &str = "gitserious.toml";

#[cfg(test)]
pub(crate) use author::built_in_context;
use author::{AuthoringOutcome, AuthoringSession, Stage};
use status_tree::{StatusTree, in_group};

/// What Commit asks the root to do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitRequest {
    Refresh,
    ShowResult,
    /// Open the editor on a new commit.
    Author,
    /// Reopen the failed commit's review.
    Resume,
    /// Hand the approved commit to Git, which needs the terminal.
    Submit,
    /// Stage every change to tracked files.
    StageTracked,
    /// Stage or unstage the selected file.
    ToggleFile,
    /// Open the shared initialization editor, which returns to Commit.
    Initialize,
    /// Open `gitserious.toml` in Git's editor, which needs the terminal.
    EditPolicy,
    /// Reopen the draft saved when Commit was last left.
    ResumeDraft,
}

/// How long a changed draft waits before it is written, so typing does not write each key.
const DRAFT_SAVE_INTERVAL: Duration = Duration::from_millis(500);

/// What Commit's `enter` sets up when the directory is not ready to commit from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Setup {
    /// A repository without `gitserious.toml`: gitserious init only.
    Gitserious,
    /// No repository at all, in a directory where one may be created.
    GitAndGitserious,
}

impl Setup {
    const fn action(self) -> &'static str {
        match self {
            Self::Gitserious => "[enter] initialize gitserious",
            Self::GitAndGitserious => "[enter] initialize git and gitserious",
        }
    }

    const fn command(self) -> &'static str {
        match self {
            Self::Gitserious => "Initialize gitserious",
            Self::GitAndGitserious => "Initialize git and gitserious",
        }
    }
}

/// A read-only popup over the page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Popup {
    /// The last Git result.
    Result,
    /// What the commit being written will contain.
    Staged,
}

#[derive(Default)]
pub(crate) struct CommitView {
    snapshot: Option<RepositoryStatus>,
    blocker: Option<String>,
    notice: String,
    /// The commit being written, kept while other pages are open.
    authoring: Option<AuthoringSession>,
    /// The approved commit, from review until Git answers.
    approved: Option<AuthoredCommit>,
    /// The session of a reviewed commit Git rejected, kept so its review can resume.
    pending: Option<AuthoringSession>,
    tree: StatusTree,
    /// The open popup and its scroll offset.
    popup: Option<(Popup, u16)>,
    /// Whether the workspace can write the index, read at each refresh.
    can_stage: bool,
    /// Whether `gitserious.toml` exists but does not load, so `e` opens it.
    can_edit_policy: bool,
    /// The directory Commit started in and what its project lacks, from the last overview.
    directory: Option<PathBuf>,
    setup: Option<Setup>,
    /// What to do instead when the directory has no usable work tree or no project.
    guidance: Option<String>,
    drafts: Drafts,
}

/// The draft kept across sessions: the one offered at Commit's start, and what the open
/// session has written of its own.
#[derive(Default)]
struct Drafts {
    /// Whether the workspace keeps drafts across sessions.
    kept: bool,
    /// A draft saved when Commit was last left, offered until it is resumed or replaced, and
    /// whether its taxonomy changed since.
    offered: Option<(SavedDraft, bool)>,
    /// What the open session last wrote, and when, so an unchanged draft is not rewritten.
    written: Option<(SavedDraft, Instant)>,
    /// The saved draft belongs to the open session, which deletes it when it ends.
    owned: bool,
    /// The session ended without a commit, so its draft goes.
    ended: bool,
}

impl CommitView {
    /// Turns `code` into a request, after moving through the tree itself.
    pub(crate) fn key(&mut self, code: KeyCode) -> Option<CommitRequest> {
        match code {
            KeyCode::Char('r') => return Some(CommitRequest::Refresh),
            KeyCode::Char('o') if !self.notice.is_empty() => {
                return Some(CommitRequest::ShowResult);
            }
            KeyCode::Enter if self.has_pending_review() => return Some(CommitRequest::Resume),
            KeyCode::Enter if self.blocker.is_none() && self.drafts.offered.is_some() => {
                return Some(CommitRequest::ResumeDraft);
            }
            KeyCode::Enter | KeyCode::Char('c') if self.blocker.is_none() => {
                return Some(CommitRequest::Author);
            }
            KeyCode::Enter if self.setup.is_some() => return Some(CommitRequest::Initialize),
            KeyCode::Char('e') if self.can_edit_policy => return Some(CommitRequest::EditPolicy),
            KeyCode::Char('a') if self.stages_tracked() => {
                return Some(CommitRequest::StageTracked);
            }
            KeyCode::Char(' ') if self.toggled_file().is_some() => {
                return Some(CommitRequest::ToggleFile);
            }
            KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => {
                self.tree.key(code, self.snapshot.as_ref())
            }
            _ => {}
        }
        None
    }

    /// Whether the editor is open, taking every key but tab and shift+tab.
    pub(crate) fn editing(&self) -> bool {
        self.authoring.is_some()
    }

    /// Hands `event` to the open editor, or a click to the tree.
    pub(crate) fn event(&mut self, event: Event) -> Option<CommitRequest> {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && key.code == KeyCode::Char('s')
            && key.modifiers.is_empty()
            && self.editor_opens_staging()
        {
            // Nothing is written in the picker; a review waits as a retained one.
            let session = self.authoring.take();
            if session
                .as_ref()
                .is_some_and(|session| session.stage == Stage::Review)
            {
                self.pending = session;
            }
            return None;
        }
        let Some(session) = &mut self.authoring else {
            if let Event::Mouse(mouse) = event
                && mouse.kind == MouseEventKind::Down(MouseButton::Left)
            {
                self.tree.click(mouse.column, mouse.row);
            }
            return None;
        };
        if let Event::Key(key) = event
            && key.code == KeyCode::Char('g')
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.popup = Some((Popup::Staged, 0));
            return None;
        }
        match session.handle_event(event)? {
            AuthoringOutcome::Cancelled => {
                self.authoring = None;
                self.drafts.ended = true;
                None
            }
            AuthoringOutcome::Authored(draft) => {
                let message = session.approved_message.clone()?;
                let taxonomy = session.taxonomy()?.id().clone();
                self.approved = Some(AuthoredCommit::reviewed(taxonomy, draft, message));
                Some(CommitRequest::Submit)
            }
        }
    }

    /// Runs `request`, and returns it when only the terminal session can.
    pub(crate) fn run(
        &mut self,
        request: CommitRequest,
        commit: &dyn CommitWorkspace,
    ) -> Option<CommitRequest> {
        match request {
            CommitRequest::Refresh => {
                self.notice.clear();
                self.refresh(commit);
            }
            CommitRequest::ShowResult => self.popup = Some((Popup::Result, 0)),
            CommitRequest::Author => {
                self.refresh(commit);
                self.author(commit);
            }
            CommitRequest::Resume => {
                self.refresh(commit);
                self.authoring = self.pending.take();
            }
            CommitRequest::ResumeDraft => {
                self.refresh(commit);
                let (saved, _) = self.drafts.offered.take()?;
                match commit
                    .prepare_commit()
                    .and_then(|context| AuthoringSession::restore(context, &saved))
                {
                    Ok(mut session) => {
                        session.set_citation_paths(commit.cite_paths());
                        session.set_citation_commits(commit.cite_commits());
                        self.authoring = Some(session);
                        self.drafts.written = Some((saved, Instant::now()));
                        self.drafts.owned = true;
                    }
                    Err(error) => {
                        self.notice = error;
                        self.drafts.offered = Some((saved, true));
                    }
                }
            }
            CommitRequest::Submit | CommitRequest::Initialize | CommitRequest::EditPolicy => {
                return Some(request);
            }
            CommitRequest::StageTracked => {
                let written = commit.stage_tracked();
                self.conclude_staging(commit, written);
            }
            CommitRequest::ToggleFile => {
                if let Some(file) = self.toggled_file().cloned() {
                    let written = if file.staged {
                        commit.unstage(&file.paths)
                    } else {
                        commit.stage(&file.paths[..1])
                    };
                    self.conclude_staging(commit, written);
                }
            }
        }
        None
    }

    fn conclude_staging(&mut self, commit: &dyn CommitWorkspace, written: Result<(), String>) {
        self.notice = written
            .err()
            .map(|error| format!("Staging failed: {error}"))
            .unwrap_or_default();
        self.refresh(commit);
    }

    /// Whether `a` would stage something: a tracked file changed in the worktree, outside a
    /// conflict Git must resolve.
    fn stages_tracked(&self) -> bool {
        self.can_stage
            && self.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot
                    .entries
                    .iter()
                    .any(|entry| !entry.untracked && !entry.conflicted && entry.worktree != '.')
            })
    }

    /// The selected file `space` stages or unstages.
    fn toggled_file(&self) -> Option<&status_tree::TreeFile> {
        self.tree
            .selected_file()
            .filter(|file| self.can_stage && !file.conflicted)
    }

    /// Whether `s` leaves the editor for staging: from the type picker, or from a review
    /// with nothing staged.
    fn editor_opens_staging(&self) -> bool {
        let Some(session) = &self.authoring else {
            return false;
        };
        self.can_stage
            && match session.stage {
                Stage::SelectType => true,
                Stage::Review => self.snapshot.as_ref().is_some_and(|snapshot| {
                    snapshot.foreign_commit_operation().is_none()
                        && snapshot.conflicted_paths().is_empty()
                        && !snapshot.has_staged_changes()
                }),
                Stage::Compose | Stage::Confirm => false,
            }
    }

    /// Opens the editor at launch when the index already holds a commit, as `enter` would,
    /// unless a saved draft waits to be resumed.
    pub(crate) fn launch(&mut self, commit: &dyn CommitWorkspace) {
        self.refresh(commit);
        if self.drafts.offered.is_none()
            && self
                .snapshot
                .as_ref()
                .is_some_and(RepositoryStatus::has_staged_changes)
        {
            self.author(commit);
        }
    }

    fn author(&mut self, commit: &dyn CommitWorkspace) {
        if self.blocker.is_some() {
            return;
        }
        match commit.prepare_commit() {
            Ok(context) => {
                let mut session = AuthoringSession::with_context(context);
                session.set_citation_paths(commit.cite_paths());
                session.set_citation_commits(commit.cite_commits());
                self.authoring = Some(session);
            }
            Err(error) => self.blocker = Some(error),
        }
    }

    /// Writes the open draft when it changed, waiting `DRAFT_SAVE_INTERVAL` between writes
    /// unless `now` asks for it. It deletes the draft once its session ends without a commit,
    /// or when the session discards what it wrote to choose another type.
    pub(crate) fn persist(&mut self, commit: &dyn CommitWorkspace, now: bool) {
        let session = self.authoring.as_ref().or(self.pending.as_ref());
        let draft = session.and_then(AuthoringSession::saved_draft);
        let discarded =
            std::mem::take(&mut self.drafts.ended) || (session.is_some() && draft.is_none());
        if discarded {
            if std::mem::take(&mut self.drafts.owned) {
                // A draft that cannot be deleted is offered again next time, which is safe.
                let _ = commit.discard_draft();
            }
            self.drafts.written = None;
            return;
        }
        let Some(draft) = draft else {
            return;
        };
        let written = self.drafts.written.as_ref();
        if written.is_some_and(|(saved, _)| *saved == draft)
            || !now && written.is_some_and(|(_, at)| at.elapsed() < DRAFT_SAVE_INTERVAL)
        {
            return;
        }
        if commit.save_draft(&draft).is_ok() {
            self.drafts.written = Some((draft, Instant::now()));
            self.drafts.owned = true;
            self.drafts.offered = None;
        }
    }

    /// Reads what the project in Commit's directory lacks, which decides what `enter` sets up
    /// and what the page says instead of the bare error.
    pub(crate) fn observe(&mut self, overview: &ProjectOverview) {
        (self.directory, self.setup, self.guidance) = match overview {
            ProjectOverview::NoRepository { directory } => (
                Some(directory.clone()),
                Some(Setup::GitAndGitserious),
                Some(guidance(&MissingWorkTree::NotRepository)),
            ),
            ProjectOverview::Uninitialized { root } => (
                Some(root.as_path().to_path_buf()),
                Some(Setup::Gitserious),
                Some(
                    "gitserious.toml is missing, so this repository has no commit policy yet."
                        .to_owned(),
                ),
            ),
            ProjectOverview::Unavailable {
                directory,
                missing: Some(missing),
                ..
            } => (Some(directory.clone()), None, Some(guidance(missing))),
            ProjectOverview::Unavailable { directory, .. } => (Some(directory.clone()), None, None),
            ProjectOverview::Initialized { root, .. } => {
                (Some(root.as_path().to_path_buf()), None, None)
            }
        };
    }

    /// Commit's palette commands.
    pub(crate) fn commands(&self) -> Vec<(&'static str, CommitRequest)> {
        let mut commands = vec![("Refresh repository", CommitRequest::Refresh)];
        if !self.notice.is_empty() {
            commands.push(("View last Git result", CommitRequest::ShowResult));
        }
        if self.blocker.is_none() {
            commands.push(("Start commit", CommitRequest::Author));
        } else if let Some(setup) = self.setup {
            commands.push((setup.command(), CommitRequest::Initialize));
        }
        if self.can_edit_policy {
            commands.push(("Edit gitserious.toml", CommitRequest::EditPolicy));
        }
        if self.has_pending_review() {
            commands.push(("Resume review", CommitRequest::Resume));
        } else if self.blocker.is_none() && self.drafts.offered.is_some() {
            commands.push(("Resume saved draft", CommitRequest::ResumeDraft));
        }
        if self.stages_tracked() {
            commands.push(("Stage all tracked changes", CommitRequest::StageTracked));
        }
        match self.toggled_file() {
            Some(file) if file.staged => {
                commands.push(("Unstage selected file", CommitRequest::ToggleFile));
            }
            Some(_) => commands.push(("Stage selected file", CommitRequest::ToggleFile)),
            None => {}
        }
        commands
    }

    pub(crate) fn hints(&self) -> Vec<(&'static str, &'static str)> {
        let Some(session) = &self.authoring else {
            let mut hints = vec![("↑/↓", "move"), ("←/→", "tree")];
            match self.toggled_file() {
                Some(file) if file.staged => hints.push(("space", "unstage")),
                Some(_) => hints.push(("space", "stage")),
                None => {}
            }
            if self.stages_tracked() {
                hints.push(("a", "stage tracked"));
            }
            if self.can_edit_policy {
                hints.push(("e", "edit gitserious.toml"));
            }
            hints.push(("r", "refresh"));
            return hints;
        };
        let mut hints = author::hints(session);
        if self.editor_opens_staging() {
            let back = hints
                .iter()
                .position(|(key, _)| *key == "esc")
                .unwrap_or(hints.len());
            hints.insert(back, ("s", "stage files"));
        }
        hints
    }

    #[cfg(test)]
    pub(crate) const fn approved(&self) -> Option<&AuthoredCommit> {
        self.approved.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn notice(&self) -> &str {
        &self.notice
    }

    #[cfg(test)]
    pub(crate) fn notice_mut(&mut self) -> &mut String {
        &mut self.notice
    }

    #[cfg(test)]
    pub(crate) fn blocker(&self) -> Option<&str> {
        self.blocker.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> Option<&RepositoryStatus> {
        self.snapshot.as_ref()
    }

    /// Whether a popup is open and takes every event.
    pub(crate) fn modal(&self) -> bool {
        self.popup.is_some()
    }

    pub(crate) fn modal_event(&mut self, event: &Event) {
        let Some((_, scroll)) = &mut self.popup else {
            return;
        };
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.popup = None,
                KeyCode::Up => *scroll = scroll.saturating_sub(1),
                KeyCode::Down => *scroll = scroll.saturating_add(1),
                KeyCode::PageUp => *scroll = scroll.saturating_sub(10),
                KeyCode::PageDown => *scroll = scroll.saturating_add(10),
                _ => {}
            }
        }
    }

    /// Draws the open popup inside the frame the root draws.
    pub(crate) fn render_modal(&mut self, frame: &mut Frame<'_>, inner: Rect) {
        let Some((popup, scroll)) = &mut self.popup else {
            return;
        };
        let [text, footer] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
        let paragraph = match popup {
            Popup::Result => Paragraph::new(self.notice.as_str()),
            Popup::Staged => Paragraph::new(
                self.snapshot
                    .as_ref()
                    .map(status_tree::staged_lines)
                    .unwrap_or_default(),
            ),
        }
        .wrap(Wrap { trim: false });
        let maximum = paragraph
            .line_count(text.width)
            .saturating_sub(usize::from(text.height));
        *scroll = (*scroll).min(u16::try_from(maximum).unwrap_or(u16::MAX));
        frame.render_widget(paragraph.scroll((*scroll, 0)), text);
        frame.render_widget(
            Paragraph::new(command_hints(&[
                ("↑/↓", "scroll"),
                ("pgup/pgdn", "page"),
                ("esc", "close"),
            ])),
            footer,
        );
    }

    /// The repository and branch, or the whole directory when no repository is usable,
    /// broken after separators to fit `width`.
    fn summary(&self, width: usize) -> Vec<String> {
        let Some(snapshot) = &self.snapshot else {
            let directory = self
                .directory
                .as_ref()
                .map(|directory| visible(&directory.to_string_lossy()))
                .unwrap_or_default();
            return std::iter::once("Directory:".to_owned())
                .chain(path_lines(&directory, width))
                .collect();
        };
        let branch = match &snapshot.branch {
            BranchState::Named(name) => visible(name),
            BranchState::Unborn(name) => format!("{} (unborn)", visible(name)),
            BranchState::Detached(oid) => format!(
                "detached HEAD ({})",
                oid.chars().take(12).collect::<String>()
            ),
        };
        let policy_untracked = snapshot
            .entries
            .iter()
            .any(|entry| entry.untracked && entry.path == Path::new(POLICY_FILE));
        vec![
            format!(
                "Repository: {}",
                visible(&snapshot.root.as_path().to_string_lossy())
            ),
            format!("Branch: {branch}"),
            if policy_untracked {
                "gitserious.toml is untracked. Commit it on its own to share the policy.".to_owned()
            } else {
                "Git commits the staged index. Use `git add -p` to stage part of a file.".to_owned()
            },
        ]
    }

    pub(crate) fn refresh(&mut self, commit: &dyn CommitWorkspace) {
        match commit.repository_status() {
            Ok(snapshot) => {
                self.drafts.kept = commit.keeps_drafts();
                if self.authoring.is_none() && self.pending.is_none() && !self.drafts.owned {
                    self.drafts.offered = commit.saved_draft().map(|saved| {
                        let changed = commit.prepare_commit().map_or(true, |context| {
                            !context.taxonomies().iter().any(|taxonomy| {
                                taxonomy.id().as_str() == saved.taxonomy
                                    && taxonomy.fingerprint().to_string() == saved.fingerprint
                            })
                        });
                        (saved, changed)
                    });
                }
                self.can_stage = commit.can_stage();
                self.can_edit_policy = commit.can_edit_policy();
                self.snapshot = Some(snapshot);
                self.blocker = commit.prepare_commit().err();
            }
            Err(error) => {
                self.snapshot = None;
                self.can_edit_policy = false;
                self.blocker = Some(error);
            }
        }
        self.tree.rebuild(self.snapshot.as_ref());
    }

    pub(crate) fn has_pending_review(&self) -> bool {
        self.pending.is_some()
    }

    /// Whether quitting now would drop a draft or a rejected review, which a workspace that
    /// keeps drafts never does.
    pub(crate) fn has_unsaved_work(&self) -> bool {
        !self.drafts.kept
            && (self.pending.is_some()
                || self
                    .authoring
                    .as_ref()
                    .is_some_and(|session| session.composer.dirty()))
    }

    /// Opens `gitserious.toml` in Git's editor with the terminal restored, then checks it again.
    pub(crate) fn edit_policy(
        &mut self,
        terminal: &mut DefaultTerminal,
        commit: &dyn CommitWorkspace,
    ) -> io::Result<()> {
        let edited = crate::runtime::suspended(terminal, || commit.edit_policy())?;
        self.notice = edited.err().unwrap_or_default();
        self.refresh(commit);
        Ok(())
    }

    /// Hands the approved commit to Git with the terminal restored, since hooks and
    /// signing may prompt in it.
    pub(crate) fn submit(
        &mut self,
        terminal: &mut DefaultTerminal,
        commit: &dyn CommitWorkspace,
    ) -> io::Result<()> {
        let Some(authored) = &self.approved else {
            return Ok(());
        };
        let result = crate::runtime::suspended(terminal, || commit.submit_commit(authored))?;
        self.conclude_submission(commit, result);
        Ok(())
    }

    /// Closes the editor on Git's answer, keeping a rejected review to resume. An index
    /// Git cannot commit, or an operation whose commit gitserious does not author, keeps the
    /// review open instead, with the reason under it.
    pub(crate) fn conclude_submission(
        &mut self,
        commit: &dyn CommitWorkspace,
        result: Result<CommitOutput, SubmitError>,
    ) {
        let session = self.authoring.take();
        let authored = self.approved.take();
        match result {
            Ok(output) => {
                self.pending = None;
                self.drafts.ended = true;
                self.persist(commit, true);
                let headline = output.created().map_or_else(
                    || "Commit created.".to_owned(),
                    |created| {
                        format!(
                            "Committed {} {}",
                            visible(&created.abbreviated_oid),
                            visible(&created.summary)
                        )
                    },
                );
                format!(
                    "{headline}\n{}{}",
                    String::from_utf8_lossy(output.stdout()),
                    String::from_utf8_lossy(output.stderr())
                )
                .trim()
                .clone_into(&mut self.notice);
            }
            Err(
                SubmitError::Operation(_) | SubmitError::Conflicts(_) | SubmitError::NothingStaged,
            ) => {
                if let (Some(mut session), Some(authored)) = (session, authored) {
                    session.reopen_review(&authored);
                    self.authoring = Some(session);
                }
            }
            Err(error) => {
                self.notice = format!("Commit failed: {error}\nThe reviewed message is retained.");
                if let (Some(mut session), Some(authored)) = (session, authored) {
                    session.reopen_review(&authored);
                    self.pending = Some(session);
                }
            }
        }
        self.refresh(commit);
    }

    pub(crate) fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        if let Some(session) = &mut self.authoring {
            let [header, rule, editor] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .areas(area);
            render_staged_header(
                frame,
                header.inner(Margin::new(1, 0)),
                self.snapshot.as_ref(),
            );
            frame.render_widget(
                Paragraph::new("─".repeat(usize::from(rule.width))).style(frame_style()),
                rule,
            );
            author::render(
                frame,
                editor,
                session,
                &commit_target(self.snapshot.as_ref()),
                index_refusal(self.snapshot.as_ref()).as_deref(),
            );
            return;
        }
        let summary_lines = self.summary(usize::from(area.width));
        let [summary, tree, status] = Layout::vertical([
            Constraint::Length(u16::try_from(summary_lines.len()).unwrap_or(u16::MAX)),
            Constraint::Min(1),
            Constraint::Length(4),
        ])
        .areas(area);
        frame.render_widget(Paragraph::new(summary_lines.join("\n")), summary);
        self.tree.render(frame, tree);
        let action = if self.has_pending_review() && self.blocker.is_none() {
            "[c] start new commit  [enter] resume review"
        } else if self.has_pending_review() {
            "[enter] resume review"
        } else if self.blocker.is_none() && self.drafts.offered.is_some() {
            "[enter] resume draft  [c] start new commit"
        } else if let (Some(_), Some(setup)) = (&self.blocker, self.setup) {
            setup.action()
        } else if self.blocker.is_some() && self.can_edit_policy {
            "[e] edit gitserious.toml  [r] check again"
        } else if self.blocker.is_some() {
            "Start commit unavailable"
        } else if self.stages_tracked()
            && !self
                .snapshot
                .as_ref()
                .is_some_and(RepositoryStatus::has_staged_changes)
        {
            "[a] stage all tracked changes  [enter] start commit"
        } else {
            "[enter] start commit"
        };
        let detail = if self.notice.is_empty() {
            self.blocker.as_ref().map_or_else(
                || {
                    self.drafts.offered.as_ref().map_or_else(
                        || self.tree.detail().map_or_else(String::new, str::to_owned),
                        saved_draft_line,
                    )
                },
                |blocker| self.guidance.clone().unwrap_or_else(|| blocker.clone()),
            )
        } else {
            self.notice.clone()
        };
        frame.render_widget(
            Paragraph::new(format!(
                "{action}{}\n{detail}",
                if self.notice.is_empty() {
                    ""
                } else {
                    "  [o] result"
                }
            ))
            .style(Style::default())
            .wrap(Wrap { trim: false }),
            status,
        );
    }
}

fn visible(text: &str) -> String {
    text.chars()
        .flat_map(|character| {
            if character.is_control() {
                character.escape_default().collect::<Vec<_>>()
            } else {
                vec![character]
            }
        })
        .collect()
}

/// The saved draft's type and subject, and a warning when its taxonomy changed since.
fn saved_draft_line((saved, changed): &(SavedDraft, bool)) -> String {
    let subject = saved.subject.lines().next().unwrap_or_default();
    let warning = if *changed {
        format!(
            " The {} policy changed since it was saved; check the draft before committing.",
            visible(&saved.taxonomy)
        )
    } else {
        String::new()
    };
    format!(
        "Saved draft: {}{}.{warning}",
        visible(&saved.commit_type),
        visible(subject)
    )
}

/// Why the commit `snapshot` describes would be refused, as Review warns before `enter`.
fn index_refusal(snapshot: Option<&RepositoryStatus>) -> Option<String> {
    let snapshot = snapshot?;
    if let Some(operation) = snapshot.foreign_commit_operation() {
        return Some(SubmitError::Operation(operation).to_string());
    }
    let conflicts = snapshot.conflicted_paths();
    if !conflicts.is_empty() {
        let paths: Vec<_> = conflicts
            .iter()
            .map(|path| visible(&path.to_string_lossy()))
            .collect();
        return Some(format!(
            "Resolve conflicts in Git before committing: {}",
            paths.join(", ")
        ));
    }
    (!snapshot.has_staged_changes()).then(|| "Nothing is staged.".to_owned())
}

/// Where Git puts the commit, as the review's commit row reads.
fn commit_target(snapshot: Option<&RepositoryStatus>) -> String {
    match snapshot.map(|snapshot| &snapshot.branch) {
        Some(BranchState::Named(name)) => format!("commit to {}", visible(name)),
        Some(BranchState::Unborn(name)) => format!("commit to {} (first commit)", visible(name)),
        Some(BranchState::Detached(_)) => "commit on detached HEAD".to_owned(),
        None => "commit".to_owned(),
    }
}

/// `N staged  path · path` on the left, cut to fit, and the staged view's key on the right.
fn render_staged_header(frame: &mut Frame<'_>, area: Rect, snapshot: Option<&RepositoryStatus>) {
    let key = Line::from(vec![
        Span::styled(
            "ctrl+g",
            Style::new().fg(theme().text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" change", Style::new().fg(theme().dim)),
    ]);
    let key_width = u16::try_from(key.width()).unwrap_or(u16::MAX);
    let staged: Vec<_> = snapshot
        .map(|snapshot| {
            snapshot
                .entries
                .iter()
                .filter(|entry| in_group(entry, 0))
                .map(|entry| visible(&entry.path.to_string_lossy()))
                .collect()
        })
        .unwrap_or_default();
    let count = format!("{} staged", staged.len());
    let room = usize::from(area.width.saturating_sub(key_width.saturating_add(2)))
        .saturating_sub(count.width() + 2);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                count,
                Style::new().fg(theme().text).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                truncate(&staged.join(" · "), room),
                Style::new().fg(theme().dim),
            ),
        ])),
        area,
    );
    frame.render_widget(Paragraph::new(key).right_aligned(), area);
}

/// `text` cut to `width` columns, ending in an ellipsis when cut.
fn truncate(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let mut kept = String::new();
    let mut used = '…'.width().unwrap_or(1);
    for character in text.chars() {
        used += character.width().unwrap_or(0);
        if used > width {
            break;
        }
        kept.push(character);
    }
    if width > 0 {
        kept.push('…');
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::navigation_key_style;
    use gitserious_app::{GitOperation, RepositoryRoot, StatusEntry};
    use ratatui::{Terminal, backend::TestBackend};
    use std::path::Path;

    #[test]
    fn a_long_staged_list_is_cut_to_keep_the_staged_view_key_visible()
    -> Result<(), Box<dyn std::error::Error>> {
        let entry = |path: &str| StatusEntry {
            path: path.into(),
            original_path: None,
            index: 'M',
            worktree: '.',
            untracked: false,
            conflicted: false,
            submodule: "N...".into(),
        };
        let snapshot = RepositoryStatus {
            root: RepositoryRoot::new(std::env::temp_dir())?,
            branch: BranchState::Named("main".into()),
            entries: vec![entry("src/net/retry.rs"), entry("tests/retry.rs")],
            operation: None,
        };
        let mut terminal = Terminal::new(TestBackend::new(40, 1))?;
        terminal.draw(|frame| render_staged_header(frame, frame.area(), Some(&snapshot)))?;
        let row: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();
        assert_eq!(row, "2 staged  src/net/retry.…  ctrl+g change");
        Ok(())
    }

    #[test]
    fn review_names_an_operation_then_conflicts_then_an_empty_index()
    -> Result<(), Box<dyn std::error::Error>> {
        let entry = |path: &str, index: char| StatusEntry {
            path: path.into(),
            original_path: None,
            index,
            worktree: '.',
            untracked: false,
            conflicted: index == 'U',
            submodule: "N...".into(),
        };
        let during = |operation: Option<GitOperation>,
                      entries: Vec<StatusEntry>|
         -> Result<Option<String>, Box<dyn std::error::Error>> {
            Ok(index_refusal(Some(&RepositoryStatus {
                root: RepositoryRoot::new(std::env::temp_dir())?,
                branch: BranchState::Named("main".into()),
                entries,
                operation,
            })))
        };

        let merge = during(Some(GitOperation::Merge), vec![entry("a.rs", 'U')])?;
        assert!(
            merge
                .as_deref()
                .is_some_and(|merge| merge.starts_with("A merge is in progress")),
            "{merge:?}"
        );
        assert_eq!(
            during(Some(GitOperation::Rebase), vec![entry("c.rs", 'M')])?,
            None,
            "a rebase stop commits normally"
        );
        assert_eq!(
            during(
                None,
                vec![entry("a.rs", 'U'), entry("b.rs", 'U'), entry("c.rs", 'M')]
            )?
            .as_deref(),
            Some("Resolve conflicts in Git before committing: a.rs, b.rs")
        );
        assert_eq!(
            during(None, Vec::new())?.as_deref(),
            Some("Nothing is staged.")
        );
        assert_eq!(during(None, vec![entry("c.rs", 'M')])?, None);
        assert_eq!(index_refusal(None), None);
        Ok(())
    }

    fn tree_view(
        entries: &[(&str, char, char, bool)],
    ) -> Result<CommitView, Box<dyn std::error::Error>> {
        let mut view = CommitView {
            snapshot: Some(RepositoryStatus {
                root: RepositoryRoot::new(std::env::temp_dir())?,
                branch: BranchState::Named("main".into()),
                entries: entries
                    .iter()
                    .map(|(path, index, worktree, untracked)| StatusEntry {
                        path: (*path).into(),
                        original_path: None,
                        index: *index,
                        worktree: *worktree,
                        untracked: *untracked,
                        conflicted: false,
                        submodule: "N...".into(),
                    })
                    .collect(),
                operation: None,
            }),
            ..CommitView::default()
        };
        view.tree.rebuild(view.snapshot.as_ref());
        Ok(view)
    }

    fn drawn(
        view: &mut CommitView,
        width: u16,
        height: u16,
    ) -> Result<ratatui::buffer::Buffer, Box<dyn std::error::Error>> {
        let mut terminal = Terminal::new(TestBackend::new(width, height))?;
        terminal.draw(|frame| view.render(frame, frame.area()))?;
        Ok(terminal.backend().buffer().clone())
    }

    fn row_text(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
            .trim_matches(|character: char| character == ' ' || character == '│')
            .to_owned()
    }

    /// The rows drawn with the tree's highlight, read inside its border.
    fn highlighted(buffer: &ratatui::buffer::Buffer) -> Vec<String> {
        let style = navigation_key_style();
        (0..buffer.area.height)
            .filter(|y| {
                let cell = &buffer[(1, *y)];
                Some(cell.fg) == style.fg && cell.modifier.contains(Modifier::BOLD)
            })
            .map(|y| row_text(buffer, y))
            .collect()
    }

    fn row_at(buffer: &ratatui::buffer::Buffer, text: &str) -> Option<u16> {
        (0..buffer.area.height).find(|y| row_text(buffer, *y) == text)
    }

    fn press(view: &mut CommitView, code: KeyCode) {
        assert_eq!(view.key(code), None, "{code:?} stays in the tree");
    }

    fn click(view: &mut CommitView, column: u16, row: u16) {
        let event = Event::Mouse(ratatui::crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(view.event(event), None);
    }

    #[test]
    fn tree_keys_stay_inside_the_rows_and_open_and_close_directories()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut view = tree_view(&[
            ("src/a.rs", 'M', '.', false),
            ("src/b.rs", 'M', '.', false),
            ("README.md", '.', 'M', false),
            ("notes.txt", '?', '?', true),
        ])?;
        assert_eq!(highlighted(&drawn(&mut view, 80, 24)?), ["Staged (2)"]);
        press(&mut view, KeyCode::Up);
        assert_eq!(highlighted(&drawn(&mut view, 80, 24)?), ["Staged (2)"]);
        press(&mut view, KeyCode::Down);
        assert_eq!(highlighted(&drawn(&mut view, 80, 24)?), ["▾ src/"]);
        press(&mut view, KeyCode::Left);
        let closed = drawn(&mut view, 80, 24)?;
        assert_eq!(highlighted(&closed), ["▸ src/"]);
        assert_eq!(
            row_at(&closed, "M a.rs"),
            None,
            "a closed directory hides its files"
        );
        press(&mut view, KeyCode::Right);
        press(&mut view, KeyCode::Down);
        let open = drawn(&mut view, 80, 24)?;
        assert_eq!(highlighted(&open), ["M a.rs"]);
        assert_eq!(
            row_text(&open, 21),
            Path::new("src").join("a.rs").to_string_lossy(),
            "the status line shows the selected path in the platform's form"
        );
        press(&mut view, KeyCode::Left);
        press(&mut view, KeyCode::Right);
        let file = drawn(&mut view, 80, 24)?;
        assert_eq!(
            highlighted(&file),
            ["M a.rs"],
            "left and right leave a file alone"
        );
        assert!(row_at(&file, "M b.rs").is_some());
        for _ in 0..20 {
            press(&mut view, KeyCode::Down);
        }
        let last = drawn(&mut view, 80, 24)?;
        assert_eq!(
            highlighted(&last),
            ["? notes.txt"],
            "down stops at the last row"
        );
        assert_eq!(row_text(&last, 21), "notes.txt");
        Ok(())
    }

    #[test]
    fn a_click_selects_the_row_drawn_there_after_scrolling_and_resizing()
    -> Result<(), Box<dyn std::error::Error>> {
        let paths: Vec<String> = (0..30)
            .map(|index| format!("src/file{index:02}.rs"))
            .collect();
        let entries: Vec<_> = paths
            .iter()
            .map(|path| (path.as_str(), 'M', '.', false))
            .collect();
        let mut view = tree_view(&entries)?;
        let buffer = drawn(&mut view, 80, 24)?;
        let row = row_at(&buffer, "M file02.rs").ok_or("missing file02")?;
        click(&mut view, 10, row);
        assert_eq!(highlighted(&drawn(&mut view, 80, 24)?), ["M file02.rs"]);

        for _ in 0..25 {
            press(&mut view, KeyCode::Down);
        }
        let scrolled = drawn(&mut view, 80, 24)?;
        assert_eq!(
            row_at(&scrolled, "Staged (30)"),
            None,
            "the list has scrolled"
        );
        let row = row_at(&scrolled, "M file20.rs").ok_or("missing file20")?;
        click(&mut view, 10, row);
        assert_eq!(highlighted(&drawn(&mut view, 80, 24)?), ["M file20.rs"]);
        click(&mut view, 10, 0);
        assert_eq!(
            highlighted(&drawn(&mut view, 80, 24)?),
            ["M file20.rs"],
            "a click outside the list does nothing"
        );

        drawn(&mut view, 80, 24)?;
        let tall = drawn(&mut view, 100, 40)?;
        assert_eq!(
            row_text(&tall, 4),
            "M file13.rs",
            "a taller list keeps its scroll and leaves blank rows below"
        );
        let row = row_at(&tall, "Untracked (0)").ok_or("missing untracked heading")?;
        assert!(row > 19, "the row lies below the shorter list's last row");
        click(&mut view, 10, row);
        assert_eq!(
            highlighted(&drawn(&mut view, 100, 40)?),
            ["Untracked (0)"],
            "a click after a resize uses the rows just drawn"
        );
        Ok(())
    }
}
