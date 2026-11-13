mod render;
mod state;
#[cfg(test)]
mod tests;

pub(crate) use render::{hints, render};
pub(crate) use state::{AuthoringOutcome, AuthoringSession};

/// A project context over the built-in taxonomies, with no type chosen.
#[cfg(test)]
pub(crate) fn built_in_context() -> Result<gitserious_app::CommitAuthoringContext, String> {
    let taxonomies = gitserious_core::built_in_taxonomies();
    gitserious_app::CommitAuthoringContext::new(taxonomies.to_vec(), taxonomies[0].id(), None)
}
