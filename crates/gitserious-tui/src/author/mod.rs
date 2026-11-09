mod render;
mod state;

use std::io;

use gitserious_app::{AuthoredCommit, CommitAuthoringContext, CommitDraftAuthorOutcome};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event;

use self::state::AuthoringSession;

/// Author within the already initialized shell terminal and retain an owned review checkpoint.
pub(crate) fn embedded_author(
    terminal: &mut DefaultTerminal,
    context: &CommitAuthoringContext,
    checkpoint: Option<state::AuthoringCheckpoint>,
) -> io::Result<Option<(AuthoredCommit, state::AuthoringCheckpoint)>> {
    let mut session = checkpoint.map_or_else(
        || AuthoringSession::with_context(context),
        |checkpoint| AuthoringSession::resume(context, checkpoint),
    );
    loop {
        terminal.draw(|frame| render::render(frame, &mut session))?;
        match session.handle_event(event::read()?) {
            Some(CommitDraftAuthorOutcome::Cancelled) => return Ok(None),
            Some(CommitDraftAuthorOutcome::Authored(draft)) => {
                let message = session
                    .approved_message
                    .clone()
                    .ok_or_else(|| io::Error::other("approved message is missing"))?;
                let taxonomy = session
                    .taxonomy
                    .ok_or_else(|| io::Error::other("selected taxonomy is missing"))?;
                let authored =
                    AuthoredCommit::reviewed(taxonomy.id().clone(), draft, message.clone());
                let checkpoint = session.checkpoint(&authored, message);
                return Ok(Some((authored, checkpoint)));
            }
            None => {}
        }
    }
}

pub(crate) use state::AuthoringCheckpoint;
