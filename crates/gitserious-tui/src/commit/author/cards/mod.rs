//! The cards a commit is written in: the subject line, one card for each
//! property of the chosen type, then breaking-change.

mod layout;
mod render;
#[cfg(test)]
mod tests;

pub(crate) use render::render;

use gitserious_app::CommitTaxonomy;
use gitserious_core::{
    COMMIT_MESSAGE_WIDTH, CommitDraft, CommitMessage, CommitScope, CommitSubject,
    CommitSubjectError, CommitTypeDefinition, CommitValidationError, ConditionalApplicability,
    PropertyKey, PropertyRequirement, PropertyResponse, PropertyValidationIssueKind, PropertyValue,
    ValidationSeverity, validate_commit_draft,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_textarea::{CursorMove, Input, TextArea};

/// A card, in the order the page shows them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CardId {
    Subject,
    /// The property at this index of the type's definition.
    Property(usize),
    BreakingChange,
}

/// How the type asks for a property, with the answer to a conditional one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Policy {
    Required,
    Recommended,
    Optional,
    /// Asked for only when it applies, which stays `None` until the user says.
    Conditional(Option<ConditionalApplicability>),
}

impl Policy {
    const fn new(requirement: &PropertyRequirement) -> Self {
        match requirement {
            PropertyRequirement::Required => Self::Required,
            PropertyRequirement::Recommended => Self::Recommended,
            PropertyRequirement::Optional => Self::Optional,
            PropertyRequirement::Conditional(_) => Self::Conditional(None),
        }
    }

    /// The word on the right of the card.
    pub(crate) const fn word(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Recommended => "recommended",
            Self::Optional => "optional",
            Self::Conditional(None) => "conditional",
            Self::Conditional(Some(ConditionalApplicability::Applies)) => "applies",
            Self::Conditional(Some(ConditionalApplicability::DoesNotApply)) => "doesn't apply",
        }
    }

    const fn decision(self) -> Option<ConditionalApplicability> {
        match self {
            Self::Conditional(decision) => decision,
            Self::Required | Self::Recommended | Self::Optional => None,
        }
    }
}

/// A property's card: its text, with its own undo history, and how the type
/// asks for it.
pub(crate) struct Card {
    pub(crate) body: TextArea<'static>,
    pub(crate) policy: Policy,
}

impl Card {
    fn new(policy: Policy) -> Self {
        Self {
            body: TextArea::default(),
            policy,
        }
    }
}

/// The glyph left of a card's key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mark {
    /// ✓ Nothing is left to write.
    Done,
    /// ● The card being written.
    Focused,
    /// ◐ A conditional property nobody has said applies or not.
    Undecided,
    /// ○ Empty, or a subject that does not read `(scope): description` yet.
    Open,
}

/// What the last review attempt said about a card, shown under it until the
/// card changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Note {
    pub(crate) card: CardId,
    pub(crate) severity: ValidationSeverity,
    pub(crate) message: String,
}

pub(crate) struct Cards {
    definition: CommitTypeDefinition,
    /// What follows the type key on the subject line: `(scope): description`
    /// or `: description`. The type itself is not text, so it cannot be edited
    /// into another type.
    pub(crate) subject: TextArea<'static>,
    pub(crate) properties: Vec<Card>,
    pub(crate) breaking_change: Card,
    pub(crate) focus: CardId,
    pub(crate) notes: Vec<Note>,
    /// The first row of the card list on screen, kept so the list scrolls only
    /// when the cursor would leave it.
    pub(crate) scroll: usize,
    /// The column bodies wrap at on screen, which ↑ and ↓ follow.
    pub(crate) wrap: usize,
}

impl Cards {
    pub(crate) fn new(definition: CommitTypeDefinition) -> Self {
        let properties = definition
            .properties()
            .iter()
            .map(|property| Card::new(Policy::new(property.requirement())))
            .collect();
        Self {
            definition,
            subject: TextArea::default(),
            properties,
            breaking_change: Card::new(Policy::Optional),
            focus: CardId::Subject,
            notes: Vec::new(),
            scroll: 0,
            wrap: usize::from(COMMIT_MESSAGE_WIDTH),
        }
    }

    /// Keeps the subject, breaking change, and every property card whose key
    /// `definition` also asks for. A carried body makes a conditional property
    /// apply. A decision carries only when the new property is conditional too.
    pub(crate) fn retarget(&mut self, definition: CommitTypeDefinition) {
        let focus_key = match self.focus {
            CardId::Subject => None,
            CardId::BreakingChange => Some(String::from("breaking-change")),
            CardId::Property(index) => self
                .definition
                .properties()
                .get(index)
                .map(|property| property.key().as_str().to_owned()),
        };
        let mut previous: Vec<(String, Card)> = self
            .definition
            .properties()
            .iter()
            .map(|property| property.key().as_str().to_owned())
            .zip(std::mem::take(&mut self.properties))
            .collect();
        self.properties = definition
            .properties()
            .iter()
            .map(|property| {
                previous
                    .iter()
                    .position(|(key, _)| key == property.key().as_str())
                    .map_or_else(
                        || Card::new(Policy::new(property.requirement())),
                        |index| adopt(previous.swap_remove(index).1, property.requirement()),
                    )
            })
            .collect();
        self.definition = definition;
        self.notes.clear();
        self.scroll = 0;
        self.focus = match focus_key.as_deref() {
            Some("breaking-change") => CardId::BreakingChange,
            Some(key) => self
                .definition
                .properties()
                .iter()
                .position(|property| property.key().as_str() == key)
                .map_or(CardId::Subject, CardId::Property),
            None => CardId::Subject,
        };
    }

    pub(crate) const fn definition(&self) -> &CommitTypeDefinition {
        &self.definition
    }

    /// Every card, top to bottom.
    pub(crate) fn ids(&self) -> impl Iterator<Item = CardId> + use<> {
        std::iter::once(CardId::Subject)
            .chain((0..self.properties.len()).map(CardId::Property))
            .chain(std::iter::once(CardId::BreakingChange))
    }

    /// The property card `id` names, which the subject is not.
    pub(crate) fn card(&self, id: CardId) -> Option<&Card> {
        match id {
            CardId::Subject => None,
            CardId::Property(index) => self.properties.get(index),
            CardId::BreakingChange => Some(&self.breaking_change),
        }
    }

    fn card_mut(&mut self, id: CardId) -> Option<&mut Card> {
        match id {
            CardId::Subject => None,
            CardId::Property(index) => self.properties.get_mut(index),
            CardId::BreakingChange => Some(&mut self.breaking_change),
        }
    }

    pub(crate) fn text(&self, id: CardId) -> &TextArea<'static> {
        self.card(id).map_or(&self.subject, |card| &card.body)
    }

    fn text_mut(&mut self, id: CardId) -> &mut TextArea<'static> {
        match id {
            CardId::Subject => &mut self.subject,
            CardId::Property(index) => &mut self.properties[index].body,
            CardId::BreakingChange => &mut self.breaking_change.body,
        }
    }

    /// The key a card shows, or `subject`.
    pub(crate) fn name(&self, id: CardId) -> &str {
        match id {
            CardId::Subject => "subject",
            CardId::Property(index) => self.definition.properties()[index].key().as_str(),
            CardId::BreakingChange => "breaking-change",
        }
    }

    /// What an empty card says it is for while it is focused.
    pub(crate) fn guidance(&self, id: CardId) -> &str {
        match id {
            CardId::Subject => "(scope): description",
            CardId::Property(index) => self.definition.properties()[index].description(),
            CardId::BreakingChange => {
                "An incompatible change. Review adds ! before the subject's colon and a BREAKING CHANGE footer."
            }
        }
    }

    /// Whether leaving now would drop anything the user wrote or decided.
    pub(crate) fn dirty(&self) -> bool {
        !self.subject.is_empty()
            || self
                .properties
                .iter()
                .chain(std::iter::once(&self.breaking_change))
                .any(|card| !card.body.is_empty() || card.policy.decision().is_some())
    }

    pub(crate) fn mark(&self, id: CardId) -> Mark {
        if id == self.focus {
            return Mark::Focused;
        }
        let Some(card) = self.card(id) else {
            return if self.subject_parts().is_ok() {
                Mark::Done
            } else {
                Mark::Open
            };
        };
        match (card.policy, value(&card.body).is_some()) {
            (Policy::Conditional(None), _) => Mark::Undecided,
            (Policy::Conditional(Some(ConditionalApplicability::DoesNotApply)), _) | (_, true) => {
                Mark::Done
            }
            (_, false) => Mark::Open,
        }
    }

    /// The cards review waits on, top to bottom.
    pub(crate) fn to_go(&self) -> Vec<CardId> {
        self.ids().filter(|id| self.blocks(*id)).collect()
    }

    fn blocks(&self, id: CardId) -> bool {
        let Some(card) = self.card(id) else {
            return self.subject_parts().is_err();
        };
        let empty = value(&card.body).is_none();
        match card.policy {
            Policy::Required | Policy::Conditional(Some(ConditionalApplicability::Applies)) => {
                empty
            }
            Policy::Conditional(None) => true,
            Policy::Recommended
            | Policy::Optional
            | Policy::Conditional(Some(ConditionalApplicability::DoesNotApply)) => false,
        }
    }

    /// Whether alt+a and alt+n answer the focused card.
    pub(crate) fn focused_conditional(&self) -> bool {
        self.card(self.focus)
            .is_some_and(|card| matches!(card.policy, Policy::Conditional(_)))
    }

    /// The subject line's scope and description.
    pub(crate) fn subject_parts(&self) -> Result<(Option<CommitScope>, CommitSubject), String> {
        parse_subject(&self.subject.lines()[0])
    }

    /// The recommendations the last review attempt made, top to bottom.
    pub(crate) fn recommendations(&self) -> impl Iterator<Item = &str> {
        self.notes
            .iter()
            .filter(|note| note.severity == ValidationSeverity::Warning)
            .map(|note| note.message.as_str())
    }

    /// Edits or moves through the focused card.
    pub(crate) fn key(&mut self, key: KeyEvent) {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Up | KeyCode::Down if key.modifiers.is_empty() => {
                self.move_vertically(key.code == KeyCode::Down);
            }
            KeyCode::Enter => self.enter(),
            KeyCode::Char('m') if control => self.enter(),
            _ => self.edit(|text| text.input(Input::from(key))),
        }
    }

    /// Inserts `text` at the cursor. The subject is one line, so it refuses a
    /// paste that has several.
    pub(crate) fn paste(&mut self, text: &str) {
        if self.focus == CardId::Subject && text.contains(['\n', '\r']) {
            return;
        }
        self.edit(|area| area.insert_str(text));
    }

    /// Answers whether the focused conditional property applies. Saying it
    /// does not clears the card, which ctrl+u undoes.
    pub(crate) fn decide(&mut self, decision: ConditionalApplicability) {
        let focus = self.focus;
        let Some(card) = self.card_mut(focus) else {
            return;
        };
        let Policy::Conditional(answer) = &mut card.policy else {
            return;
        };
        *answer = Some(decision);
        if decision == ConditionalApplicability::DoesNotApply {
            card.body.clear();
        }
        self.notes.retain(|note| note.card != focus);
    }

    /// Validates the cards as a draft and renders its message, or notes what
    /// blocks it on the cards and focuses the first.
    pub(crate) fn review(
        &mut self,
        taxonomy: Option<&CommitTaxonomy>,
    ) -> Option<(CommitDraft, CommitMessage)> {
        self.notes.clear();
        let draft = match self.draft() {
            Ok(draft) => draft,
            Err(message) => {
                self.note(CardId::Subject, ValidationSeverity::Error, message);
                return self.reject();
            }
        };
        let report = taxonomy.map_or_else(
            || validate_commit_draft(&self.definition, &draft),
            |taxonomy| taxonomy.validate(&draft),
        );
        for error in report.errors() {
            let card = self.error_card(error);
            self.note(card, ValidationSeverity::Error, error.to_string());
        }
        for warning in report.warnings() {
            let card = self.property_card(issue_key(warning.kind()));
            self.note(
                card,
                ValidationSeverity::Warning,
                warning.kind().to_string(),
            );
        }
        match report.into_message() {
            Ok(message) => Some((draft, message)),
            Err(_) => self.reject(),
        }
    }

    fn draft(&self) -> Result<CommitDraft, String> {
        let (scope, subject) = self.subject_parts()?;
        let responses = self
            .definition
            .properties()
            .iter()
            .zip(&self.properties)
            .map(|(property, card)| {
                PropertyResponse::new(
                    property.key().clone(),
                    value(&card.body),
                    card.policy.decision(),
                )
            })
            .collect();
        let draft =
            CommitDraft::from_responses(self.definition.id().clone(), scope, subject, responses)
                .map_err(|error| error.to_string())?;
        Ok(match value(&self.breaking_change.body) {
            Some(breaking_change) => draft.with_breaking_change(breaking_change),
            None => draft,
        })
    }

    fn note(&mut self, card: CardId, severity: ValidationSeverity, message: String) {
        self.notes.push(Note {
            card,
            severity,
            message,
        });
    }

    fn reject(&mut self) -> Option<(CommitDraft, CommitMessage)> {
        if let Some(first) = self.ids().find(|id| {
            self.notes
                .iter()
                .any(|note| note.card == *id && note.severity == ValidationSeverity::Error)
        }) {
            self.focus = first;
        }
        None
    }

    fn error_card(&self, error: &CommitValidationError) -> CardId {
        match error {
            CommitValidationError::UnknownProperty(key)
            | CommitValidationError::MissingRequired(key) => self.property_card(key),
            CommitValidationError::PropertyResponse(kind) => self.property_card(issue_key(kind)),
            CommitValidationError::UnknownCommitType { .. }
            | CommitValidationError::TypeMismatch { .. } => CardId::Subject,
        }
    }

    fn property_card(&self, key: &PropertyKey) -> CardId {
        self.definition
            .properties()
            .iter()
            .position(|property| property.key() == key)
            .map_or(CardId::Subject, CardId::Property)
    }

    fn edit(&mut self, change: impl FnOnce(&mut TextArea<'static>) -> bool) {
        let focus = self.focus;
        if !change(self.text_mut(focus)) {
            return;
        }
        self.notes.retain(|note| note.card != focus);
        // Writing in a conditional card answers that it applies.
        if let Some(card) = self.card_mut(focus)
            && let Policy::Conditional(answer) = &mut card.policy
            && value(&card.body).is_some()
        {
            *answer = Some(ConditionalApplicability::Applies);
        }
    }

    fn move_vertically(&mut self, down: bool) {
        let text = self.text(self.focus);
        let within = (self.focus != CardId::Subject)
            .then(|| layout::vertical(text.lines(), text.cursor(), self.wrap, down))
            .flatten();
        match within {
            Some((line, column)) => {
                let focus = self.focus;
                self.text_mut(focus)
                    .move_cursor(CursorMove::Jump(terminal(line), terminal(column)));
            }
            None => self.step(down),
        }
    }

    /// Enter leaves the subject for the next card and inserts a line break
    /// in every body, including an empty one.
    fn enter(&mut self) {
        if self.focus == CardId::Subject {
            self.step(true);
        } else {
            self.edit(|text| {
                text.insert_newline();
                true
            });
        }
    }

    /// Focuses the next card at its start, or the previous card at its end.
    fn step(&mut self, down: bool) {
        let ids: Vec<_> = self.ids().collect();
        let position = ids.iter().position(|id| *id == self.focus).unwrap_or(0);
        let next = if down {
            ids.get(position + 1)
        } else {
            position
                .checked_sub(1)
                .and_then(|previous| ids.get(previous))
        };
        let Some(&next) = next else {
            return;
        };
        self.focus = next;
        let text = self.text_mut(next);
        if down {
            text.move_cursor(CursorMove::Jump(0, 0));
        } else {
            text.move_cursor(CursorMove::Bottom);
            text.move_cursor(CursorMove::End);
        }
    }
}

fn adopt(mut card: Card, requirement: &PropertyRequirement) -> Card {
    card.policy = match (card.policy, Policy::new(requirement)) {
        (Policy::Conditional(decision @ Some(_)), Policy::Conditional(_)) => {
            Policy::Conditional(decision)
        }
        (_, Policy::Conditional(_)) if value(&card.body).is_some() => {
            Policy::Conditional(Some(ConditionalApplicability::Applies))
        }
        (_, policy) => policy,
    };
    card
}

/// Reads `(scope): description` or `: description`, the subject after its type.
fn parse_subject(tail: &str) -> Result<(Option<CommitScope>, CommitSubject), String> {
    if tail.trim().is_empty() {
        return Err(CommitSubjectError::Blank.to_string());
    }
    let (scope, rest) = match tail.strip_prefix('(') {
        Some(rest) => {
            let (scope, rest) = rest
                .split_once(')')
                .ok_or_else(|| "close the scope with \")\"".to_owned())?;
            let scope = CommitScope::new(scope.trim()).map_err(|error| error.to_string())?;
            (Some(scope), rest)
        }
        None => (None, tail),
    };
    let description = rest
        .strip_prefix(':')
        .ok_or_else(|| "put \": \" before the description".to_owned())?;
    let subject = CommitSubject::new(description.trim()).map_err(|error| error.to_string())?;
    Ok((scope, subject))
}

/// A card's text as the message carries it: no trailing whitespace on any
/// line and no blank lines around it.
fn value(text: &TextArea<'_>) -> Option<PropertyValue> {
    let lines = text.lines();
    let start = lines.iter().position(|line| !line.trim().is_empty())?;
    let end = lines.iter().rposition(|line| !line.trim().is_empty())?;
    PropertyValue::new(
        lines[start..=end]
            .iter()
            .map(|line| line.trim_end())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .ok()
}

fn issue_key(kind: &PropertyValidationIssueKind) -> &PropertyKey {
    use PropertyValidationIssueKind as Kind;
    match kind {
        Kind::UnknownProperty(key)
        | Kind::DuplicateProperty(key)
        | Kind::MissingRequired(key)
        | Kind::MissingRecommended(key)
        | Kind::MissingConditionalDecision(key)
        | Kind::MissingApplicableValue(key)
        | Kind::ValueForNonApplicableProperty(key)
        | Kind::UnexpectedConditionalDecision(key) => key,
    }
}

fn terminal(index: usize) -> u16 {
    u16::try_from(index).unwrap_or(u16::MAX)
}
