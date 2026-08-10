mod render;
mod state;

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::{self, IsTerminal};

use gitserious_app::{CommitDraftAuthor, CommitDraftAuthorOutcome};
use gitserious_core::CommitTypeDefinition;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event;

use self::state::AuthoringSession;

/// Ratatui implementation of structured commit-draft authoring.
#[derive(Clone, Copy, Debug, Default)]
pub struct RatatuiCommitDraftAuthor;

impl CommitDraftAuthor for RatatuiCommitDraftAuthor {
    type Error = RatatuiCommitDraftAuthorError;

    fn author(
        &self,
        definitions: &[CommitTypeDefinition],
        preselected: Option<&CommitTypeDefinition>,
    ) -> Result<CommitDraftAuthorOutcome, Self::Error> {
        if definitions.is_empty() {
            return Err(RatatuiCommitDraftAuthorError::EmptyCatalog);
        }
        let preselected_index = preselected
            .map(|selected| {
                definitions
                    .iter()
                    .position(|definition| definition.id() == selected.id())
                    .ok_or(RatatuiCommitDraftAuthorError::UnknownPreselection)
            })
            .transpose()?;
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(RatatuiCommitDraftAuthorError::NotTerminal);
        }

        ratatui::run(|terminal| run_author(terminal, definitions, preselected_index))
            .map_err(RatatuiCommitDraftAuthorError::Terminal)
    }
}

/// Failure to present or operate structured commit authoring.
#[derive(Debug)]
pub enum RatatuiCommitDraftAuthorError {
    /// Effective project policy contains no commit types.
    EmptyCatalog,
    /// The requested preselection is absent from the effective catalog.
    UnknownPreselection,
    /// Authoring was requested without interactive terminal streams.
    NotTerminal,
    /// Terminal initialization, rendering, input, or restoration failed.
    Terminal(io::Error),
}

impl Display for RatatuiCommitDraftAuthorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCatalog => {
                formatter.write_str("cannot author from an empty commit-type catalog")
            }
            Self::UnknownPreselection => {
                formatter.write_str("preselected commit type is unavailable")
            }
            Self::NotTerminal => {
                formatter.write_str("commit authoring requires an interactive terminal")
            }
            Self::Terminal(_) => formatter.write_str("commit authoring failed"),
        }
    }
}

impl Error for RatatuiCommitDraftAuthorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Terminal(error) => Some(error),
            Self::EmptyCatalog | Self::UnknownPreselection | Self::NotTerminal => None,
        }
    }
}

fn run_author(
    terminal: &mut DefaultTerminal,
    definitions: &[CommitTypeDefinition],
    preselected_index: Option<usize>,
) -> io::Result<CommitDraftAuthorOutcome> {
    let mut session = AuthoringSession::new(definitions, preselected_index);
    loop {
        terminal.draw(|frame| render::render(frame, &mut session))?;
        let event = event::read()?;
        if let Some(outcome) = session.handle_event(event) {
            return Ok(outcome);
        }
    }
}
