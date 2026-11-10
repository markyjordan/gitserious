use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};

use super::*;
use std::cell::RefCell;

use gitserious_app::{
    BranchState, CommitAuthoringContext, CommitOutput, CommitWorkspace, ConfigurationWorkspace,
    GlobalConfiguration, LibraryDraft, ProjectConfiguration, ProjectOverview, ProjectSnapshot,
    ProjectWorkspace, RepositoryRoot, RepositoryStatus,
};

use crate::commit;
use crate::config::RootKeys;
use crate::help::{HelpLine, HelpSpan};
use crate::home::{self, HomeAction};
use crate::theme::{FOOTER_HEIGHT, MINIMUM_HEIGHT, MINIMUM_WIDTH, theme};
use gitserious_core::TaxonomyVersion;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::{Terminal, backend::TestBackend};
use support::*;

mod support;

mod commit_tests;
mod config_tests;
mod help_tests;
mod home_tests;
mod navigation_tests;
mod project_overview_tests;
