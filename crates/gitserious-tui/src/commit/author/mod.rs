mod navigation;
mod render;
mod state;
#[cfg(test)]
mod tests;

use std::io;

use gitserious_app::{AuthoredCommit, CommitAuthoringContext};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event;

use self::state::{AuthoringOutcome, AuthoringSession};

/// Author within the unified session's already initialized terminal and retain an owned review checkpoint.
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
            Some(AuthoringOutcome::Cancelled) => return Ok(None),
            Some(AuthoringOutcome::Authored(draft)) => {
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

#[cfg(test)]
pub(crate) fn reviewed_submission()
-> Result<(CommitAuthoringContext, AuthoredCommit, AuthoringCheckpoint), String> {
    let taxonomies = gitserious_core::built_in_taxonomies();
    let context = CommitAuthoringContext::new(taxonomies.to_vec(), taxonomies[0].id(), None)?;
    let session = state::AuthoringSession::with_context(&context);
    let definition = &context.initial_taxonomy().definitions()[0];
    let properties = definition
        .properties()
        .iter()
        .filter(|property| {
            property.requirement() == &gitserious_core::PropertyRequirement::Required
        })
        .map(|property| {
            let value = gitserious_core::PropertyValue::new(format!("reviewed {}", property.key()))
                .map_err(|error| error.to_string())?;
            Ok(gitserious_core::AuthoredProperty::new(
                property.key().clone(),
                value,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let draft = gitserious_core::CommitDraft::new(
        definition.id().clone(),
        None,
        gitserious_core::CommitSubject::new("open the commit page")
            .map_err(|error| error.to_string())?,
        properties,
    )
    .map_err(|error| error.to_string())?;
    let message = context
        .initial_taxonomy()
        .validate(&draft)
        .into_message()
        .map_err(|error| error.to_string())?;
    let authored = AuthoredCommit::reviewed(
        context.initial_taxonomy().id().clone(),
        draft,
        message.clone(),
    );
    let checkpoint = session.checkpoint(&authored, message);
    Ok((context, authored, checkpoint))
}
