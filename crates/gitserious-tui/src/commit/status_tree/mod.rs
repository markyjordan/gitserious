//! The Commit page's status tree: staged, unstaged, and untracked entries
//! grouped by directory, which directories are closed, the selected row, and
//! where the list was last drawn, which clicks hit.

mod render;

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use gitserious_app::{RepositoryStatus, StatusEntry};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::widgets::ListState;

pub(super) use render::staged_lines;

use super::visible;

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
pub(super) struct StatusTree {
    collapsed: BTreeSet<(usize, PathBuf)>,
    rows: Vec<TreeRow>,
    selected: usize,
    list: ListState,
    list_area: Rect,
}

impl StatusTree {
    /// Up and Down move the selection. Left and Right close and open the
    /// selected directory, rebuilding the rows from `snapshot`.
    pub(super) fn key(&mut self, code: KeyCode, snapshot: Option<&RepositoryStatus>) {
        match code {
            KeyCode::Up | KeyCode::Down => self.move_selection(code == KeyCode::Down),
            KeyCode::Left | KeyCode::Right => self.directory(code == KeyCode::Right, snapshot),
            _ => {}
        }
    }

    /// The selected row's path, which the status line shows.
    pub(super) fn detail(&self) -> Option<&str> {
        self.rows.get(self.selected).map(|row| row.detail.as_str())
    }

    /// Rebuilds the rows from `snapshot`, keeping the selection in range.
    pub(super) fn rebuild(&mut self, snapshot: Option<&RepositoryStatus>) {
        self.rows.clear();
        if let Some(snapshot) = snapshot {
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

    fn directory(&mut self, expand: bool, snapshot: Option<&RepositoryStatus>) {
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
            self.rebuild(snapshot);
        }
    }

    /// Selects the row drawn under a click in the last frame.
    pub(super) fn click(&mut self, x: u16, y: u16) {
        if self.list_area.contains((x, y).into()) {
            let index = self.list.offset() + usize::from(y.saturating_sub(self.list_area.y));
            if index < self.rows.len() {
                self.selected = index;
                self.list.select(Some(index));
            }
        }
    }
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

pub(super) fn in_group(entry: &StatusEntry, group: usize) -> bool {
    match group {
        0 => !entry.untracked && entry.index != '.',
        1 => !entry.untracked && entry.worktree != '.',
        _ => entry.untracked,
    }
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
    use gitserious_app::{BranchState, RepositoryRoot};

    #[test]
    fn partial_staging_appears_in_both_groups_and_directories_collapse()
    -> Result<(), Box<dyn std::error::Error>> {
        let snapshot = RepositoryStatus {
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
            operation: None,
        };
        let mut tree = StatusTree::default();
        tree.rebuild(Some(&snapshot));
        assert_eq!(
            tree.rows
                .iter()
                .filter(|row| row.label.contains("file.rs"))
                .count(),
            2
        );
        tree.selected = 1;
        tree.directory(false, Some(&snapshot));
        assert_eq!(
            tree.rows
                .iter()
                .filter(|row| row.label.contains("file.rs"))
                .count(),
            1
        );
        tree.directory(true, Some(&snapshot));
        assert_eq!(
            tree.rows
                .iter()
                .filter(|row| row.label.contains("file.rs"))
                .count(),
            2
        );
        Ok(())
    }
}
