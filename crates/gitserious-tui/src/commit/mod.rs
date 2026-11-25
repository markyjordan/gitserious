mod author;
mod status_tree;

use std::io;

use gitserious_app::{
    AuthoredCommit, BranchState, CommitOutput, CommitWorkspace, RepositoryStatus, SubmitError,
};
use ratatui::crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::theme::{command_hints, frame_style, theme};

#[cfg(test)]
pub(crate) use author::built_in_context;
use author::{AuthoringOutcome, AuthoringSession};
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
            KeyCode::Enter | KeyCode::Char('c') if self.blocker.is_none() => {
                return Some(CommitRequest::Author);
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
            CommitRequest::Submit => return Some(request),
        }
        None
    }

    /// Opens the editor at launch when the index already holds a commit, as `enter` would.
    pub(crate) fn launch(&mut self, commit: &dyn CommitWorkspace) {
        self.refresh(commit);
        if self
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

    /// Commit's palette commands.
    pub(crate) fn commands(&self) -> Vec<(&'static str, CommitRequest)> {
        let mut commands = vec![("Refresh repository", CommitRequest::Refresh)];
        if !self.notice.is_empty() {
            commands.push(("View last Git result", CommitRequest::ShowResult));
        }
        if self.blocker.is_none() {
            commands.push(("Start commit", CommitRequest::Author));
        }
        if self.has_pending_review() {
            commands.push(("Resume review", CommitRequest::Resume));
        }
        commands
    }

    pub(crate) fn hints(&self) -> Vec<(&'static str, &'static str)> {
        self.authoring.as_ref().map_or_else(
            || vec![("↑/↓", "move"), ("←/→", "tree"), ("r", "refresh")],
            author::hints,
        )
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

    pub(crate) fn refresh(&mut self, commit: &dyn CommitWorkspace) {
        match commit.repository_status() {
            Ok(snapshot) => {
                self.snapshot = Some(snapshot);
                self.blocker = commit.prepare_commit().err();
            }
            Err(error) => {
                self.snapshot = None;
                self.blocker = Some(error);
            }
        }
        self.tree.rebuild(self.snapshot.as_ref());
    }

    pub(crate) fn has_pending_review(&self) -> bool {
        self.pending.is_some()
    }

    /// Whether quitting now would drop a draft or a rejected review.
    pub(crate) fn has_unsaved_work(&self) -> bool {
        self.pending.is_some()
            || self
                .authoring
                .as_ref()
                .is_some_and(|session| session.composer.dirty())
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
    /// Git cannot commit keeps the review open instead, with the reason under it.
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
                format!(
                    "Commit created.\n{}{}",
                    String::from_utf8_lossy(output.stdout()),
                    String::from_utf8_lossy(output.stderr())
                )
                .trim()
                .clone_into(&mut self.notice);
            }
            Err(SubmitError::Conflicts(_) | SubmitError::NothingStaged) => {
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
        let [summary, tree, status] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(4),
        ])
        .areas(area);
        let summary_text = self.snapshot.as_ref().map_or_else(|| "Repository unavailable".into(), |snapshot| {
            let branch = match &snapshot.branch {
                BranchState::Named(name) => visible(name), BranchState::Unborn(name) => format!("{} (unborn)", visible(name)),
                BranchState::Detached(oid) => format!("detached HEAD ({})", oid.chars().take(12).collect::<String>()),
            };
            format!("Repository: {}\nBranch: {branch}\nGit commits the staged index. Stage changes with Git.", visible(&snapshot.root.as_path().to_string_lossy()))
        });
        frame.render_widget(Paragraph::new(summary_text), summary);
        self.tree.render(frame, tree);
        let action = if self.has_pending_review() && self.blocker.is_none() {
            "[c] start new commit  [enter] resume review"
        } else if self.has_pending_review() {
            "[enter] resume review"
        } else if self.blocker.is_some() {
            "Start commit unavailable"
        } else {
            "[enter] start commit"
        };
        let detail = if self.notice.is_empty() {
            self.blocker
                .clone()
                .unwrap_or_else(|| self.tree.detail().map_or_else(String::new, str::to_owned))
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

/// Why Git would refuse to commit the index `snapshot` read, as Review warns before `enter`.
fn index_refusal(snapshot: Option<&RepositoryStatus>) -> Option<String> {
    let snapshot = snapshot?;
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
    (!snapshot.has_staged_changes())
        .then(|| "Nothing is staged. Stage changes with Git before committing.".to_owned())
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
    use gitserious_app::{RepositoryRoot, StatusEntry};
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
    fn review_names_conflicts_before_an_empty_index() -> Result<(), Box<dyn std::error::Error>> {
        let entry = |path: &str, index: char| StatusEntry {
            path: path.into(),
            original_path: None,
            index,
            worktree: '.',
            untracked: false,
            conflicted: index == 'U',
            submodule: "N...".into(),
        };
        let refusal =
            |entries: Vec<StatusEntry>| -> Result<Option<String>, Box<dyn std::error::Error>> {
                Ok(index_refusal(Some(&RepositoryStatus {
                    root: RepositoryRoot::new(std::env::temp_dir())?,
                    branch: BranchState::Named("main".into()),
                    entries,
                })))
            };

        assert_eq!(
            refusal(vec![
                entry("a.rs", 'U'),
                entry("b.rs", 'U'),
                entry("c.rs", 'M')
            ])?
            .as_deref(),
            Some("Resolve conflicts in Git before committing: a.rs, b.rs")
        );
        assert_eq!(
            refusal(Vec::new())?.as_deref(),
            Some("Nothing is staged. Stage changes with Git before committing.")
        );
        assert_eq!(refusal(vec![entry("c.rs", 'M')])?, None);
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
