use std::io::{self, IsTerminal};

use crate::help::HelpPage;
use crate::root::{Capabilities, InitialPage, SessionRequest, Shell};

/// Unified Home, Commit, Config, and Help terminal adapter.
#[derive(Clone, Debug)]
pub struct RatatuiInteractiveUi {
    start: InitialPage,
    help: Vec<HelpPage>,
}

impl RatatuiInteractiveUi {
    /// Opens at `start`, with `help` as the pages of the Help section.
    #[must_use]
    pub const fn at(start: InitialPage, help: Vec<HelpPage>) -> Self {
        Self { start, help }
    }

    /// Runs the session on `capabilities` until the user quits.
    pub fn run(&self, capabilities: Capabilities<'_>) -> Result<(), String> {
        let help = &self.help;
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(match self.start {
                InitialPage::Config => "configuration editing requires an interactive terminal",
                InitialPage::Commit => "commit requires an interactive terminal",
                InitialPage::Home | InitialPage::Help { .. } => {
                    "interactive shell requires an interactive terminal"
                }
            }
            .into());
        }
        crate::runtime::run(|terminal| {
            let mut shell = Shell::new(self.start.clone(), capabilities, help);
            loop {
                terminal.draw(|frame| shell.render(frame, help))?;
                let event = ratatui::crossterm::event::read()?;
                match shell.session_event(event, capabilities, help) {
                    Some(SessionRequest::Quit) => return Ok(()),
                    Some(SessionRequest::Author { resume }) => {
                        shell.author(terminal, capabilities, resume)?;
                    }
                    None => {}
                }
            }
        })
        .map_err(|error| format!("interactive shell failed: {error}"))
    }
}
