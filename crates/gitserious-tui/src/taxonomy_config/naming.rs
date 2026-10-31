use std::borrow::Cow;

use gitserious_app::{
    DraftField, EditRefusal, LibraryDraft, NodeRef, Problem, ProblemKind, ProblemSite,
};
use gitserious_core::LabelError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NameField {
    Taxonomy,
    Type,
    Property,
    Condition,
}

impl NameField {
    const fn noun(self) -> &'static str {
        match self {
            Self::Taxonomy => "Taxonomy name",
            Self::Type => "Type name",
            Self::Property => "Property name",
            Self::Condition => "Condition name",
        }
    }
}

pub(super) fn name_error(field: NameField, raw: &str, error: &LabelError) -> String {
    let noun = field.noun();
    match error {
        LabelError::Empty if raw.trim().is_empty() => format!("{noun} is required."),
        LabelError::Empty => format!("{noun} needs a letter."),
        LabelError::StartsWithDigit { formatted } => {
            format!("{noun} must start with a letter: \"{formatted}\".")
        }
        LabelError::Unsupported { character } => {
            format!("{noun} can't use \"{character}\". Use letters a-z and digits.")
        }
    }
}

/// The notice for an entry whose description is blank, such as `Type description is required.`
pub(super) fn description_required(owner: &str) -> String {
    format!("{owner} description is required.")
}

/// Renders text from the domain or the application as a sentence: a capital first letter and
/// closing punctuation, so every notice reads the same way.
pub(super) fn sentence(message: &str) -> String {
    let message = message.trim();
    let mut characters = message.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    let mut text = first.to_uppercase().chain(characters).collect::<String>();
    if !text.ends_with(['.', '?', '!']) {
        text.push('.');
    }
    text
}

/// Replaces each run of line breaks with one space so a pasted name stays on one line.
pub(super) fn single_line(text: &str) -> Cow<'_, str> {
    if !text.contains(['\n', '\r']) {
        return Cow::Borrowed(text);
    }
    let mut line = String::with_capacity(text.len());
    let mut breaking = false;
    for character in text.chars() {
        if matches!(character, '\n' | '\r') {
            if !breaking {
                line.push(' ');
            }
            breaking = true;
        } else {
            breaking = false;
            line.push(character);
        }
    }
    Cow::Owned(line)
}

/// The notice for an edit the Library refused. `field` names the field a text edit targeted.
pub(super) fn refusal_sentence(refusal: EditRefusal, field: Option<DraftField>) -> String {
    match (
        refusal,
        field.and_then(|field| Some((field, field.limit()?))),
    ) {
        (EditRefusal::ReadOnly, _) => {
            "Built-in taxonomies are read-only; fork one to customize it.".into()
        }
        (EditRefusal::NameLocked, _) => {
            "A taxonomy keeps its name once it is applied; fork it to use another name.".into()
        }
        (EditRefusal::TooLong, Some((field, limit))) => {
            format!("{} is limited to {limit} characters.", field.label())
        }
        (EditRefusal::TooLong, None) => "That text is too long for this field.".into(),
        (EditRefusal::NotApplicable | EditRefusal::Missing, _) => {
            "Select a taxonomy, type, or property first.".into()
        }
    }
}

/// The sentence for a problem that keeps the Library from building.
pub(super) fn problem_sentence(library: &LibraryDraft, problem: &Problem) -> String {
    let node = problem.node().and_then(|node| library.node(node));
    let owner = match node {
        Some(NodeRef::Taxonomy(_)) | None => ("Taxonomy", NameField::Taxonomy, "the Library"),
        Some(NodeRef::Type { .. }) => ("Type", NameField::Type, "this taxonomy"),
        Some(NodeRef::Property { .. }) => ("Property", NameField::Property, "this type"),
    };
    let field = match &problem.site {
        ProblemSite::Field(_, field) => Some(*field),
        ProblemSite::Node(_) | ProblemSite::Selection => None,
    };
    match &problem.kind {
        ProblemKind::Label(error) => {
            let name = if field == Some(DraftField::Condition) {
                NameField::Condition
            } else {
                owner.1
            };
            let raw = problem
                .node()
                .zip(field)
                .and_then(|(node, field)| library.text(node, field))
                .unwrap_or_default();
            name_error(name, raw, error)
        }
        ProblemKind::Taken(id) => format!(
            "A {} named \"{id}\" already exists in {}.",
            owner.0.to_lowercase(),
            owner.2
        ),
        ProblemKind::StagedForDeletion(id) => format!(
            "Taxonomy \"{id}\" is staged for deletion; apply the deletion before reusing its name."
        ),
        ProblemKind::Blank if field == Some(DraftField::Rationale) => {
            "Condition rationale is required.".into()
        }
        ProblemKind::Blank => description_required(owner.0),
        ProblemKind::NoTypes => "Add a type before applying the taxonomy.".into(),
        ProblemKind::Selection(error) => sentence(&error.to_string()),
    }
}
