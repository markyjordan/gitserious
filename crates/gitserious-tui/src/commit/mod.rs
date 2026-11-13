mod author;

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use gitserious_app::{
    AuthoredCommit, BranchState, CommitOutput, CommitWorkspace, RepositoryStatus, StatusEntry,
};
use ratatui::crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::theme::{
    block_title, command_hints, frame_style, navigation_key_style, section_heading_style, theme,
};

#[cfg(test)]
pub(crate) use author::built_in_context;
use author::{AuthoringOutcome, AuthoringSession};

#[derive(Clone)]
struct TreeRow {
    label: String,
    directory: Option<(usize, PathBuf)>,
    detail: String,
}

#[derive(Default)]
struct Node<'a> {
    children: BTreeMap<OsString, Node<'a>>,
    entry: Option<&'a StatusEntry>,
}

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
    collapsed: BTreeSet<(usize, PathBuf)>,
    rows: Vec<TreeRow>,
    selected: usize,
    list: ListState,
    list_area: Rect,
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
            KeyCode::Enter => {
                return Some(if self.has_pending_review() {
                    CommitRequest::Resume
                } else {
                    CommitRequest::Author
                });
            }
            KeyCode::Char('c') => return Some(CommitRequest::Author),
            KeyCode::Up | KeyCode::Down => self.move_selection(code == KeyCode::Down),
            KeyCode::Left | KeyCode::Right => self.directory(code == KeyCode::Right),
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
                self.click(mouse.column, mouse.row);
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
                match commit.prepare_commit() {
                    Ok(context) => {
                        let mut session = AuthoringSession::with_context(context);
                        session.set_cite_paths(commit.cite_paths());
                        session.set_cite_commits(commit.cite_commits());
                        self.authoring = Some(session);
                    }
                    Err(error) => self.notice = error,
                }
            }
            CommitRequest::Resume => {
                self.refresh(commit);
                self.authoring = self.pending.take();
            }
            CommitRequest::Submit => return Some(request),
        }
        None
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
                    .map(|snapshot| group_rows(snapshot, 0, &BTreeSet::new()))
                    .unwrap_or_default()
                    .into_iter()
                    .map(|row| {
                        if row.directory.is_none() && row.detail.is_empty() {
                            Line::styled(row.label, section_heading_style())
                        } else {
                            Line::from(row.label)
                        }
                    })
                    .collect::<Vec<_>>(),
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
        self.rebuild();
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
                .is_some_and(|session| session.cards.dirty())
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

    /// Closes the editor on Git's answer, keeping a rejected review to resume.
    pub(crate) fn conclude_submission(
        &mut self,
        commit: &dyn CommitWorkspace,
        result: Result<CommitOutput, String>,
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

    fn rebuild(&mut self) {
        self.rows.clear();
        if let Some(snapshot) = &self.snapshot {
            for group in 0..3 {
                self.rows
                    .extend(group_rows(snapshot, group, &self.collapsed));
            }
        }
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
        self.list
            .select((!self.rows.is_empty()).then_some(self.selected));
    }

    fn move_selection(&mut self, down: bool) {
        self.selected = if down {
            (self.selected + 1).min(self.rows.len().saturating_sub(1))
        } else {
            self.selected.saturating_sub(1)
        };
        self.list
            .select((!self.rows.is_empty()).then_some(self.selected));
    }

    fn directory(&mut self, expand: bool) {
        if let Some(key) = self
            .rows
            .get(self.selected)
            .and_then(|row| row.directory.clone())
        {
            if expand {
                self.collapsed.remove(&key);
            } else {
                self.collapsed.insert(key);
            }
            self.rebuild();
        }
    }

    fn click(&mut self, x: u16, y: u16) {
        if self.list_area.contains((x, y).into()) {
            let index = self.list.offset() + usize::from(y.saturating_sub(self.list_area.y));
            if index < self.rows.len() {
                self.selected = index;
                self.list.select(Some(index));
            }
        }
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
        let block = Block::bordered()
            .border_style(frame_style())
            .title(block_title("Changes"));
        self.list_area = block.inner(tree);
        let items: Vec<_> = self
            .rows
            .iter()
            .map(|row| {
                let item = ListItem::new(row.label.clone());
                if row.directory.is_none() && row.detail.is_empty() {
                    item.style(section_heading_style())
                } else {
                    item
                }
            })
            .collect();
        frame.render_stateful_widget(
            List::new(items)
                .block(block)
                .highlight_style(navigation_key_style()),
            tree,
            &mut self.list,
        );
        let action = if self.has_pending_review() {
            "[c] start new commit  [enter] resume review"
        } else if self.blocker.is_some() {
            "Start commit unavailable"
        } else {
            "[enter] start commit"
        };
        let detail = if self.notice.is_empty() {
            self.blocker.clone().unwrap_or_else(|| {
                self.rows
                    .get(self.selected)
                    .map_or_else(String::new, |row| row.detail.clone())
            })
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

/// One group of the tree: 0 staged, 1 unstaged, 2 untracked, under its heading.
fn group_rows(
    snapshot: &RepositoryStatus,
    group: usize,
    collapsed: &BTreeSet<(usize, PathBuf)>,
) -> Vec<TreeRow> {
    let entries: Vec<_> = snapshot
        .entries
        .iter()
        .filter(|entry| in_group(entry, group))
        .collect();
    let name = ["Staged", "Unstaged", "Untracked"][group];
    let mut rows = vec![TreeRow {
        label: format!("{name} ({})", entries.len()),
        directory: None,
        detail: String::new(),
    }];
    let mut root = Node::default();
    for entry in entries {
        let mut node = &mut root;
        for component in entry.path.components() {
            node = node
                .children
                .entry(component.as_os_str().to_os_string())
                .or_default();
        }
        node.entry = Some(entry);
    }
    append_rows(&root, Path::new(""), group, 1, collapsed, &mut rows);
    rows
}

fn in_group(entry: &StatusEntry, group: usize) -> bool {
    match group {
        0 => !entry.untracked && entry.index != '.',
        1 => !entry.untracked && entry.worktree != '.',
        _ => entry.untracked,
    }
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

fn append_rows(
    node: &Node<'_>,
    parent: &Path,
    group: usize,
    depth: usize,
    collapsed: &BTreeSet<(usize, PathBuf)>,
    rows: &mut Vec<TreeRow>,
) {
    for (name, child) in &node.children {
        let path = parent.join(name);
        let indent = "  ".repeat(depth);
        let name = visible(&name.to_string_lossy());
        if let Some(entry) = child.entry {
            let marker = if entry.conflicted {
                '!'
            } else if entry.untracked {
                '?'
            } else if group == 0 {
                entry.index
            } else {
                entry.worktree
            };
            let renamed = entry
                .original_path
                .as_ref()
                .map_or_else(String::new, |original| {
                    format!(" ← {}", visible(&original.to_string_lossy()))
                });
            let submodule = if entry.submodule == "N..." {
                String::new()
            } else {
                format!("  submodule {}", entry.submodule)
            };
            rows.push(TreeRow {
                label: format!("{indent}{marker} {name}{renamed}{submodule}"),
                directory: None,
                detail: visible(&path.to_string_lossy()),
            });
        }
        if !child.children.is_empty() {
            let key = (group, path.clone());
            let closed = collapsed.contains(&key);
            rows.push(TreeRow {
                label: format!("{indent}{} {name}/", if closed { '▸' } else { '▾' }),
                directory: Some(key),
                detail: visible(&path.to_string_lossy()),
            });
            if !closed {
                append_rows(child, &path, group, depth + 1, collapsed, rows);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitserious_app::RepositoryRoot;
    use ratatui::{Terminal, backend::TestBackend};

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
    fn partial_staging_appears_in_both_groups_and_directories_collapse()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut view = CommitView {
            snapshot: Some(RepositoryStatus {
                root: RepositoryRoot::new(std::env::temp_dir())?,
                branch: BranchState::Named("main".into()),
                entries: vec![StatusEntry {
                    path: "src/nested/file.rs".into(),
                    original_path: None,
                    index: 'M',
                    worktree: 'M',
                    untracked: false,
                    conflicted: false,
                    submodule: "N...".into(),
                }],
            }),
            ..CommitView::default()
        };
        view.rebuild();
        assert_eq!(
            view.rows
                .iter()
                .filter(|row| row.label.contains("file.rs"))
                .count(),
            2
        );
        view.selected = 1;
        view.directory(false);
        assert_eq!(
            view.rows
                .iter()
                .filter(|row| row.label.contains("file.rs"))
                .count(),
            1
        );
        view.directory(true);
        assert_eq!(
            view.rows
                .iter()
                .filter(|row| row.label.contains("file.rs"))
                .count(),
            2
        );
        Ok(())
    }
}
