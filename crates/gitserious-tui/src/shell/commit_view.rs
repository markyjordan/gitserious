use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use gitserious_app::{BranchState, InteractiveWorkspace, RepositoryStatus, StatusEntry};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};

use crate::theme::{frame_style, navigation_key_style, section_heading_style};

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

#[derive(Default)]
pub(super) struct CommitView {
    pub(super) snapshot: Option<RepositoryStatus>,
    pub(super) blocker: Option<String>,
    pub(super) notice: String,
    collapsed: BTreeSet<(usize, PathBuf)>,
    rows: Vec<TreeRow>,
    selected: usize,
    list: ListState,
    list_area: Rect,
}

impl CommitView {
    pub(super) fn refresh(&mut self, workspace: &dyn InteractiveWorkspace) {
        match workspace.repository_status() {
            Ok(snapshot) => {
                self.snapshot = Some(snapshot);
                self.blocker = workspace.prepare_commit().err();
            }
            Err(error) => {
                self.snapshot = None;
                self.blocker = Some(error);
            }
        }
        self.rebuild();
    }

    fn rebuild(&mut self) {
        self.rows.clear();
        if let Some(snapshot) = &self.snapshot {
            for (group, name) in ["Staged", "Unstaged", "Untracked"].into_iter().enumerate() {
                let entries: Vec<_> = snapshot
                    .entries
                    .iter()
                    .filter(|entry| match group {
                        0 => !entry.untracked && entry.index != '.',
                        1 => !entry.untracked && entry.worktree != '.',
                        _ => entry.untracked,
                    })
                    .collect();
                self.rows.push(TreeRow {
                    label: format!("{name} ({})", entries.len()),
                    directory: None,
                    detail: String::new(),
                });
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
                append_rows(
                    &root,
                    Path::new(""),
                    group,
                    1,
                    &self.collapsed,
                    &mut self.rows,
                );
            }
        }
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
        self.list
            .select((!self.rows.is_empty()).then_some(self.selected));
    }

    pub(super) fn move_selection(&mut self, down: bool) {
        self.selected = if down {
            (self.selected + 1).min(self.rows.len().saturating_sub(1))
        } else {
            self.selected.saturating_sub(1)
        };
        self.list
            .select((!self.rows.is_empty()).then_some(self.selected));
    }

    pub(super) fn directory(&mut self, expand: bool) {
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

    pub(super) fn click(&mut self, x: u16, y: u16) {
        if self.list_area.contains((x, y).into()) {
            let index = self.list.offset() + usize::from(y.saturating_sub(self.list_area.y));
            if index < self.rows.len() {
                self.selected = index;
                self.list.select(Some(index));
            }
        }
    }

    pub(super) fn render(&mut self, frame: &mut Frame<'_>, area: Rect, pending: bool) {
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
            .title("Changes");
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
        let action = if pending {
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
