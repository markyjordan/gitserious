use ratatui::crossterm::event::{self, KeyEvent};

use super::*;
use std::cell::RefCell;

use gitserious_app::{
    BranchState, CommitAuthoringContext, CommitOutput, CommitWorkspace, ConfigurationWorkspace,
    GlobalConfiguration, LibraryDraft, ProjectConfiguration, ProjectOverview, ProjectSnapshot,
    ProjectWorkspace, RepositoryRoot, RepositoryStatus,
};

use super::help_page::{HelpLine, HelpSpan};
use gitserious_core::TaxonomyVersion;
use ratatui::{Terminal, backend::TestBackend};
use support::*;

mod support;

mod commit_tests;
mod config_tests;
mod help_tests;
mod home_tests;
mod navigation_tests;
mod project_overview_tests;
