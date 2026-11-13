use super::cards::Cards;
use crate::confirmation::{Answer, ConfirmDialog, DISCARD};
use gitserious_app::{AuthoredCommit, CommitAuthoringContext, CommitTaxonomy};
use gitserious_core::{CommitDraft, CommitMessage, CommitTypeDefinition, ConditionalApplicability};
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

/// What the editor hands back when the user leaves it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AuthoringOutcome {
    /// The reviewed draft, approved for submission.
    Authored(CommitDraft),
    /// The user left without approving a draft.
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Stage {
    SelectType,
    Compose,
    Review,
    Confirm,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConfirmationAction {
    Cancel,
    ChangeType,
}

pub(crate) struct ReviewState {
    pub(crate) draft: CommitDraft,
    pub(crate) message: CommitMessage,
    pub(crate) scroll: u16,
    pub(crate) scrollable: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum TypeCatalogKind {
    #[default]
    Conventional,
    Taxonomy(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CatalogTab {
    pub(crate) kind: TypeCatalogKind,
    pub(crate) area: Rect,
}

pub(crate) struct AuthoringSession {
    context: Option<CommitAuthoringContext>,
    pub(crate) approved_message: Option<CommitMessage>,
    pub(crate) definitions: Vec<CommitTypeDefinition>,
    pub(crate) selected_type: usize,
    pub(crate) type_catalog: TypeCatalogKind,
    pub(crate) preselected: bool,
    pub(crate) stage: Stage,
    pub(crate) cards: Cards,
    pub(crate) review: Option<ReviewState>,
    pub(crate) confirmation: ConfirmationAction,
    pub(crate) discard_dialog: ConfirmDialog,
    pub(crate) catalog_tabs: Vec<CatalogTab>,
    /// What the user typed on the subject line while choosing a type.
    pub(crate) filter: String,
    /// The first matching type the list shows when it is taller than the picker.
    pub(crate) type_offset: usize,
    /// The definition index of each drawn type row, and where it was drawn.
    pub(crate) type_rows: Vec<(usize, Rect)>,
    pub(crate) too_small: bool,
}

impl AuthoringSession {
    pub(crate) fn new(
        definitions: &[CommitTypeDefinition],
        preselected_index: Option<usize>,
    ) -> Self {
        let selected_type = preselected_index.unwrap_or(0);
        Self {
            definitions: definitions.to_vec(),
            context: None,
            approved_message: None,
            selected_type,
            type_catalog: TypeCatalogKind::Conventional,
            preselected: preselected_index.is_some(),
            stage: if preselected_index.is_some() {
                Stage::Compose
            } else {
                Stage::SelectType
            },
            cards: Cards::new(definitions[selected_type].clone()),
            review: None,
            confirmation: ConfirmationAction::Cancel,
            discard_dialog: ConfirmDialog::default(),
            catalog_tabs: Vec::new(),
            filter: String::new(),
            type_offset: 0,
            type_rows: Vec::new(),
            too_small: false,
        }
    }

    /// Shows the review of `authored` again after Git rejected it.
    pub(crate) fn reopen_review(&mut self, authored: &AuthoredCommit) {
        self.approved_message = None;
        self.review = Some(ReviewState {
            draft: authored.draft().clone(),
            message: authored.reviewed_message().clone(),
            scroll: 0,
            scrollable: false,
        });
        self.stage = Stage::Review;
    }

    pub(crate) fn definition(&self) -> &CommitTypeDefinition {
        &self.definitions[self.selected_type]
    }

    pub(crate) fn with_context(context: CommitAuthoringContext) -> Self {
        let taxonomy = context.initial_taxonomy();
        let preselected = context.preselected_type().and_then(|selected| {
            taxonomy
                .definitions()
                .iter()
                .position(|definition| definition.id() == selected.id())
        });
        let mut session = Self::new(taxonomy.definitions(), preselected);
        session.type_catalog = TypeCatalogKind::Taxonomy(
            context
                .taxonomies()
                .iter()
                .position(|item| item.id() == taxonomy.id())
                .unwrap_or(0),
        );
        session.context = Some(context);
        session
    }

    /// The taxonomy whose types the picker lists, absent without a project context.
    pub(crate) fn taxonomy(&self) -> Option<&CommitTaxonomy> {
        catalog_taxonomy(self.context.as_ref(), self.type_catalog)
    }

    pub(crate) fn available_type_catalogs(&self) -> Vec<TypeCatalogKind> {
        self.context.as_ref().map_or_else(
            || vec![TypeCatalogKind::Conventional],
            |context| {
                (0..context.taxonomies().len())
                    .map(TypeCatalogKind::Taxonomy)
                    .collect()
            },
        )
    }

    pub(crate) fn catalog_label(&self, kind: TypeCatalogKind) -> String {
        match kind {
            TypeCatalogKind::Conventional => "conventional".to_owned(),
            TypeCatalogKind::Taxonomy(_) => catalog_taxonomy(self.context.as_ref(), kind)
                .map_or_else(String::new, |taxonomy| taxonomy.id().to_string()),
        }
    }

    fn cycle_type_catalog(&mut self, reverse: bool) {
        let catalogs = self.available_type_catalogs();
        let current = catalogs
            .iter()
            .position(|kind| *kind == self.type_catalog)
            .unwrap_or(0);
        self.select_type_catalog(
            catalogs[(current + if reverse { catalogs.len() - 1 } else { 1 }) % catalogs.len()],
        );
    }

    fn select_type_catalog(&mut self, kind: TypeCatalogKind) {
        if self.preselected || self.type_catalog == kind {
            return;
        }
        if let TypeCatalogKind::Taxonomy(_) = kind {
            let Some(taxonomy) = catalog_taxonomy(self.context.as_ref(), kind) else {
                return;
            };
            self.definitions = taxonomy.definitions().to_vec();
            self.selected_type = self.matching_types().first().copied().unwrap_or(0);
            self.cards = Cards::new(self.definition().clone());
            self.review = None;
            self.approved_message = None;
        }
        self.type_catalog = kind;
    }

    /// The indexes of the types whose key starts with the filter, in catalog order.
    ///
    /// A prefix match, because the filter is the start of the subject line, which
    /// begins with the type key.
    pub(crate) fn matching_types(&self) -> Vec<usize> {
        let filter = self.filter.to_lowercase();
        self.definitions
            .iter()
            .enumerate()
            .filter(|(_, definition)| definition.id().as_str().starts_with(&filter))
            .map(|(index, _)| index)
            .collect()
    }

    fn select_first_match(&mut self) {
        if let Some(first) = self.matching_types().first() {
            self.selected_type = *first;
        }
    }

    /// The stage on screen: a discard confirmation shows over the cards.
    pub(crate) fn visible_stage(&self) -> Stage {
        if self.stage == Stage::Confirm {
            Stage::Compose
        } else {
            self.stage
        }
    }

    pub(crate) fn handle_event(&mut self, event: Event) -> Option<AuthoringOutcome> {
        if self.stage == Stage::Confirm {
            return self.handle_confirmation_event(event);
        }
        if matches!(event, Event::Resize(_, _)) {
            self.discard_dialog.invalidate();
        }
        if self.too_small {
            return self.handle_too_small(&event);
        }
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            Event::Paste(text) if self.stage == Stage::Compose => {
                self.cards.paste(&text);
                None
            }
            Event::Resize(_, _)
            | Event::FocusGained
            | Event::FocusLost
            | Event::Paste(_)
            | Event::Key(_) => None,
        }
    }

    fn handle_too_small(&mut self, event: &Event) -> Option<AuthoringOutcome> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if self.stage == Stage::Confirm {
                    self.handle_confirmation_key(*key)
                } else if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
                    self.request_confirmation(ConfirmationAction::Cancel)
                } else {
                    None
                }
            }
            Event::Mouse(mouse) => self.handle_mouse(*mouse),
            _ => None,
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> Option<AuthoringOutcome> {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return None;
        }
        if self.stage == Stage::Confirm {
            return self.handle_confirmation_event(Event::Mouse(mouse));
        }
        if self.stage == Stage::SelectType
            && let Some(tab) = self
                .catalog_tabs
                .iter()
                .find(|tab| contains(tab.area, mouse.column, mouse.row))
        {
            self.select_type_catalog(tab.kind);
        } else if self.stage == Stage::SelectType
            && let Some((index, _)) = self
                .type_rows
                .iter()
                .find(|(_, area)| contains(*area, mouse.column, mouse.row))
        {
            self.selected_type = *index;
        }
        None
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<AuthoringOutcome> {
        match self.stage {
            Stage::SelectType => self.handle_picker_key(key),
            Stage::Compose => self.handle_cards_key(key),
            Stage::Review => self.handle_review_key(key),
            Stage::Confirm => self.handle_confirmation_key(key),
        }
    }

    fn handle_picker_key(&mut self, key: KeyEvent) -> Option<AuthoringOutcome> {
        let matches = self.matching_types();
        match key.code {
            KeyCode::Up | KeyCode::Down => {
                if let Some(position) = matches
                    .iter()
                    .position(|index| *index == self.selected_type)
                {
                    let step = if key.code == KeyCode::Up {
                        matches.len() - 1
                    } else {
                        1
                    };
                    self.selected_type = matches[(position + step) % matches.len()];
                }
            }
            KeyCode::Left | KeyCode::Right => self.cycle_type_catalog(key.code == KeyCode::Left),
            KeyCode::Enter if !matches.is_empty() => {
                self.filter.clear();
                self.cards = Cards::new(self.definition().clone());
                self.stage = Stage::Compose;
            }
            KeyCode::Esc if self.filter.is_empty() => return Some(AuthoringOutcome::Cancelled),
            KeyCode::Esc => {
                self.filter.clear();
                self.select_first_match();
            }
            KeyCode::Backspace => {
                if self.filter.pop().is_some() {
                    self.select_first_match();
                }
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.filter.push(character);
                self.select_first_match();
            }
            _ => {}
        }
        None
    }

    fn handle_cards_key(&mut self, key: KeyEvent) -> Option<AuthoringOutcome> {
        if key.modifiers.contains(KeyModifiers::ALT) && matches!(key.code, KeyCode::Char('a' | 'n'))
        {
            self.cards.decide(if key.code == KeyCode::Char('a') {
                ConditionalApplicability::Applies
            } else {
                ConditionalApplicability::DoesNotApply
            });
            return None;
        }
        if control(key, 's') {
            let taxonomy = catalog_taxonomy(self.context.as_ref(), self.type_catalog);
            if let Some((draft, message)) = self.cards.review(taxonomy) {
                self.approved_message = None;
                self.review = Some(ReviewState {
                    draft,
                    message,
                    scroll: 0,
                    scrollable: false,
                });
                self.stage = Stage::Review;
            }
            return None;
        }
        if key.code == KeyCode::Esc {
            let action = if self.preselected {
                ConfirmationAction::Cancel
            } else {
                ConfirmationAction::ChangeType
            };
            return self.request_confirmation(action);
        }
        self.cards.key(key);
        None
    }

    fn handle_review_key(&mut self, key: KeyEvent) -> Option<AuthoringOutcome> {
        match key.code {
            KeyCode::Enter => {
                let review = self.review.take()?;
                self.approved_message = Some(review.message);
                return Some(AuthoringOutcome::Authored(review.draft));
            }
            KeyCode::Esc => {
                self.review = None;
                self.stage = Stage::Compose;
            }
            KeyCode::Up => {
                if let Some(review) = &mut self.review
                    && review.scrollable
                {
                    review.scroll = review.scroll.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if let Some(review) = &mut self.review
                    && review.scrollable
                {
                    review.scroll = review.scroll.saturating_add(1);
                }
            }
            KeyCode::PageUp => {
                if let Some(review) = &mut self.review
                    && review.scrollable
                {
                    review.scroll = review.scroll.saturating_sub(10);
                }
            }
            KeyCode::PageDown => {
                if let Some(review) = &mut self.review
                    && review.scrollable
                {
                    review.scroll = review.scroll.saturating_add(10);
                }
            }
            _ => {}
        }
        None
    }

    fn handle_confirmation_key(&mut self, key: KeyEvent) -> Option<AuthoringOutcome> {
        self.handle_confirmation_event(Event::Key(key))
    }

    fn handle_confirmation_event(&mut self, event: Event) -> Option<AuthoringOutcome> {
        match self.discard_dialog.handle_event(DISCARD, &event)? {
            Answer::Confirm => self.confirm_discard(),
            Answer::Keep | Answer::Alternate => {
                self.resume_after_confirmation();
                None
            }
        }
    }

    fn confirm_discard(&mut self) -> Option<AuthoringOutcome> {
        self.discard_dialog.invalidate();
        match self.confirmation {
            ConfirmationAction::Cancel => Some(AuthoringOutcome::Cancelled),
            ConfirmationAction::ChangeType => {
                self.cards = Cards::new(self.definition().clone());
                self.review = None;
                self.stage = Stage::SelectType;
                None
            }
        }
    }

    fn resume_after_confirmation(&mut self) {
        self.discard_dialog.invalidate();
        self.stage = Stage::Compose;
    }

    fn request_confirmation(&mut self, action: ConfirmationAction) -> Option<AuthoringOutcome> {
        if !self.cards.dirty() {
            return match action {
                ConfirmationAction::Cancel => Some(AuthoringOutcome::Cancelled),
                ConfirmationAction::ChangeType => {
                    self.cards = Cards::new(self.definition().clone());
                    self.review = None;
                    self.stage = Stage::SelectType;
                    None
                }
            };
        }
        self.confirmation = action;
        self.discard_dialog.invalidate();
        self.stage = Stage::Confirm;
        None
    }
}

fn catalog_taxonomy(
    context: Option<&CommitAuthoringContext>,
    kind: TypeCatalogKind,
) -> Option<&CommitTaxonomy> {
    match kind {
        TypeCatalogKind::Conventional => None,
        TypeCatalogKind::Taxonomy(index) => context?.taxonomies().get(index),
    }
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x && column < area.right() && row >= area.y && row < area.bottom()
}

fn control(key: KeyEvent, character: char) -> bool {
    key.code == KeyCode::Char(character) && key.modifiers.contains(KeyModifiers::CONTROL)
}

#[cfg(test)]
mod confirmation_tests {
    use super::super::render;
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }
    fn click(area: Rect) -> Event {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: area.x + 1,
            row: area.y,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn shared_commit_dialog_keeps_the_draft_and_can_cancel_by_mouse()
    -> Result<(), Box<dyn std::error::Error>> {
        let draw = |frame: &mut ratatui::Frame<'_>, session: &mut AuthoringSession| {
            render::render(frame, frame.area(), session, "commit to main");
        };
        for (width, height) in [(60, 18), (100, 24)] {
            let mut session =
                AuthoringSession::new(gitserious_core::built_in_commit_types(), Some(0));
            session.cards.paste(": draft");
            let text = session.cards.subject.lines().to_vec();
            let cursor = session.cards.subject.cursor();
            session.request_confirmation(ConfirmationAction::Cancel);
            let mut terminal = Terminal::new(TestBackend::new(width, height))?;
            terminal.draw(|frame| draw(frame, &mut session))?;
            let buttons = session.discard_dialog.buttons.ok_or("missing buttons")?;
            session.handle_event(click(Rect::new(0, 0, 1, 1)));
            session.handle_event(Event::Paste("ignored".into()));
            assert_eq!(session.stage, Stage::Confirm);
            session.handle_event(click(buttons.keep));
            assert_eq!(session.stage, Stage::Compose);
            assert_eq!(session.cards.subject.lines(), text);
            assert_eq!(session.cards.subject.cursor(), cursor);
            assert!(session.discard_dialog.buttons.is_none());
            session.request_confirmation(ConfirmationAction::Cancel);
            terminal.draw(|frame| draw(frame, &mut session))?;
            let buttons = session.discard_dialog.buttons.ok_or("missing buttons")?;
            session.handle_event(Event::Resize(30, 7));
            assert!(session.discard_dialog.buttons.is_none());
            assert!(session.handle_event(click(buttons.confirm)).is_none());
            assert_eq!(session.stage, Stage::Confirm);
            session.handle_event(key(KeyCode::Enter));
            session.request_confirmation(ConfirmationAction::Cancel);
            terminal.draw(|frame| draw(frame, &mut session))?;
            let buttons = session.discard_dialog.buttons.ok_or("missing buttons")?;
            assert!(matches!(
                session.handle_event(click(buttons.confirm)),
                Some(AuthoringOutcome::Cancelled)
            ));
            assert!(session.discard_dialog.buttons.is_none());
        }
        Ok(())
    }

    #[test]
    fn empty_required_property_is_named_by_its_key() {
        let mut session = AuthoringSession::new(gitserious_core::built_in_commit_types(), Some(0));
        session.handle_event(Event::Paste(": add a thing".into()));
        session.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('s'),
            KeyModifiers::CONTROL,
        )));
        let messages = session
            .cards
            .notes
            .iter()
            .map(|note| note.message.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            messages,
            [
                "complete required property \"intent\"",
                "complete required property \"decision\"",
                "choose whether conditional property \"constraints\" applies",
            ]
        );
    }

    #[test]
    fn shared_commit_dialog_preserves_change_type_behavior()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut session = AuthoringSession::new(gitserious_core::built_in_commit_types(), None);
        session.handle_event(key(KeyCode::Enter));
        session.handle_event(Event::Paste("draft".into()));
        session.handle_event(key(KeyCode::Esc));
        assert_eq!(session.stage, Stage::Confirm);
        assert_eq!(session.confirmation, ConfirmationAction::ChangeType);
        session.handle_event(key(KeyCode::Esc));
        assert_eq!(session.stage, Stage::Compose);
        assert!(session.cards.dirty());
        session.handle_event(key(KeyCode::Esc));
        let mut terminal = Terminal::new(TestBackend::new(100, 24))?;
        terminal
            .draw(|frame| render::render(frame, frame.area(), &mut session, "commit to main"))?;
        let buttons = session.discard_dialog.buttons.ok_or("missing buttons")?;
        assert!(session.handle_event(click(buttons.confirm)).is_none());
        assert_eq!(session.stage, Stage::SelectType);
        assert!(!session.cards.dirty());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitserious_core::{
        CommitTypeId, Description, SchemaVersion, Taxonomy, TaxonomyId, TaxonomyVersion,
    };

    fn notes_taxonomy() -> Result<Taxonomy, Box<dyn std::error::Error>> {
        Ok(Taxonomy::new(
            TaxonomyId::new("notes")?,
            Some(TaxonomyVersion::V1),
            Description::new("Notes")?,
            None,
            vec![CommitTypeDefinition::new(
                Some(SchemaVersion::V1),
                CommitTypeId::new("note")?,
                "Record a note",
                Vec::new(),
            )?],
        )?)
    }

    fn context() -> Result<CommitAuthoringContext, Box<dyn std::error::Error>> {
        Ok(CommitAuthoringContext::new(
            vec![notes_taxonomy()?],
            &TaxonomyId::new("notes")?,
            None,
        )?)
    }

    #[test]
    fn a_reopened_review_preserves_editor_and_exact_review_bytes()
    -> Result<(), Box<dyn std::error::Error>> {
        let context = context()?;
        let mut session = AuthoringSession::with_context(context.clone());
        session.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));
        session.handle_event(Event::Paste(": Preserve this message".into()));
        let original_lines = session.cards.subject.lines().to_vec();
        session.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('s'),
            KeyModifiers::CONTROL,
        )));
        assert_eq!(session.stage, Stage::Review);
        let expected = session
            .review
            .as_ref()
            .ok_or("missing review")?
            .message
            .clone();
        let Some(AuthoringOutcome::Authored(draft)) = session.handle_event(Event::Key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        )) else {
            return Err("submission did not return authored draft".into());
        };
        let authored = gitserious_app::AuthoredCommit::reviewed(
            context.initial_taxonomy().id().clone(),
            draft,
            expected.clone(),
        );
        session.reopen_review(&authored);
        assert_eq!(session.stage, Stage::Review);
        assert_eq!(session.cards.subject.lines(), original_lines);
        assert_eq!(
            session
                .review
                .as_ref()
                .ok_or("missing reopened review")?
                .message,
            expected
        );
        assert!(session.approved_message.is_none());
        session.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert_eq!(session.stage, Stage::Compose);
        assert_eq!(session.cards.subject.lines(), original_lines);
        Ok(())
    }

    #[test]
    fn taxonomy_picker_cycles_forward_and_backward_with_wrapping()
    -> Result<(), Box<dyn std::error::Error>> {
        let first = notes_taxonomy()?;
        let second = Taxonomy::new(
            TaxonomyId::new("second")?,
            Some(TaxonomyVersion::V1),
            Description::new("Second")?,
            None,
            first.commit_types().to_vec(),
        )?;
        let third = Taxonomy::new(
            TaxonomyId::new("third")?,
            Some(TaxonomyVersion::V1),
            Description::new("Third")?,
            None,
            first.commit_types().to_vec(),
        )?;
        let taxonomies = vec![first, second, third];
        let context =
            CommitAuthoringContext::new(taxonomies.clone(), &TaxonomyId::new("notes")?, None)?;
        let mut session = AuthoringSession::with_context(context.clone());
        session.handle_event(Event::Key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)));
        assert_eq!(
            session.taxonomy().ok_or("missing taxonomy")?.id().as_str(),
            "third"
        );
        session.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Right,
            KeyModifiers::NONE,
        )));
        assert_eq!(
            session.taxonomy().ok_or("missing taxonomy")?.id().as_str(),
            "notes"
        );
        session.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Right,
            KeyModifiers::NONE,
        )));
        assert_eq!(
            session.taxonomy().ok_or("missing taxonomy")?.id().as_str(),
            "second"
        );
        session.handle_event(Event::Key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)));
        assert_eq!(
            session.taxonomy().ok_or("missing taxonomy")?.id().as_str(),
            "notes"
        );
        let selected = CommitTypeId::new("note")?;
        let preselected =
            CommitAuthoringContext::new(taxonomies, &TaxonomyId::new("notes")?, Some(&selected))?;
        let mut session = AuthoringSession::with_context(preselected);
        session.stage = Stage::SelectType;
        session.handle_event(Event::Key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)));
        assert_eq!(
            session.taxonomy().ok_or("missing taxonomy")?.id().as_str(),
            "notes"
        );
        Ok(())
    }
}
