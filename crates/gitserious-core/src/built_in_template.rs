use std::sync::LazyLock;

use crate::{CommitMessageTemplateDefinition, TemplateId, TemplateVersion, built_in_commit_types};

static DEFAULT_COMMIT_MESSAGE_TEMPLATE: LazyLock<CommitMessageTemplateDefinition> =
    LazyLock::new(|| {
        CommitMessageTemplateDefinition::from_trusted(
            TemplateVersion::V1,
            TemplateId::from_trusted("conventional"),
            "The built-in Conventional Commit types with durable, type-specific properties.",
            built_in_commit_types().to_vec(),
        )
    });

/// Returns the concrete template currently selected by the `default` channel.
#[must_use]
pub fn default_commit_message_template() -> &'static CommitMessageTemplateDefinition {
    &DEFAULT_COMMIT_MESSAGE_TEMPLATE
}
