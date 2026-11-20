//! Keys and pastes the composer takes: editing, moving between rows and
//! fields, and answering a conditional property.

use gitserious_core::ConditionalApplicability;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_textarea::{CursorMove, Input, Key, TextArea};

use super::{FieldRequirementState, MessageComposer, MessageFieldId, layout, value};

impl MessageComposer {
    /// Edits or moves through the focused field.
    pub(crate) fn key(&mut self, key: KeyEvent) {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Up | KeyCode::Down if key.modifiers.is_empty() => {
                self.move_vertically(key.code == KeyCode::Down);
            }
            KeyCode::Enter => self.enter(),
            KeyCode::Char('m') if control => self.enter(),
            KeyCode::Char(character)
                if self.focus == MessageFieldId::Subject
                    && (key.modifiers - KeyModifiers::SHIFT).is_empty()
                    && !character.is_control() =>
            {
                self.edit(|text| type_subject(text, character));
            }
            _ if self.focus == MessageFieldId::Subject => {
                self.edit(|text| edit_subject(text, Input::from(key)));
            }
            _ => self.edit(|text| text.input(Input::from(key))),
        }
    }

    /// Inserts `text` at the cursor. The subject is one line, so it refuses a
    /// paste that has several.
    pub(crate) fn paste(&mut self, text: &str) {
        if self.focus == MessageFieldId::Subject && text.contains(['\n', '\r']) {
            return;
        }
        self.edit(|area| area.insert_str(text));
    }

    /// Answers whether the focused conditional property applies. Saying it
    /// does not clears the field, which ctrl+u undoes.
    pub(crate) fn decide(&mut self, decision: ConditionalApplicability) {
        let focus = self.focus;
        let Some(field) = self.field_mut(focus) else {
            return;
        };
        let FieldRequirementState::Conditional(answer) = &mut field.requirement else {
            return;
        };
        *answer = Some(decision);
        if decision == ConditionalApplicability::DoesNotApply {
            field.body.clear();
        }
        self.feedback.retain(|feedback| feedback.field != focus);
    }

    fn edit(&mut self, change: impl FnOnce(&mut TextArea<'static>) -> bool) {
        let focus = self.focus;
        if !change(self.text_mut(focus)) {
            return;
        }
        self.feedback.retain(|feedback| feedback.field != focus);
        // Writing in a conditional field answers that it applies.
        if let Some(field) = self.field_mut(focus)
            && let FieldRequirementState::Conditional(answer) = &mut field.requirement
            && value(&field.body).is_some()
        {
            *answer = Some(ConditionalApplicability::Applies);
        }
    }

    fn move_vertically(&mut self, down: bool) {
        let text = self.text(self.focus);
        let within = (self.focus != MessageFieldId::Subject)
            .then(|| layout::vertical(text.lines(), text.cursor(), self.layout.wrap_width, down))
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

    /// Enter leaves the subject for the next field and inserts a line break
    /// in every body, including an empty one.
    fn enter(&mut self) {
        if self.focus == MessageFieldId::Subject {
            self.step(true);
        } else {
            self.edit(|text| {
                text.insert_newline();
                true
            });
        }
    }

    /// Focuses the next field at its start, or the previous field at its end.
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

fn edit_subject(text: &mut TextArea<'_>, input: Input) -> bool {
    let deletes = matches!(
        (input.key, input.ctrl, input.alt),
        (Key::Backspace | Key::Delete, false, _)
            | (Key::Char('h' | 'd'), false, true)
            | (Key::Char('h' | 'd' | 'j' | 'k' | 'w' | 'x'), true, false)
            | (Key::Cut, _, _)
    );
    let tail = &text.lines()[0];
    let Some((scope, rest)) = tail
        .strip_prefix('(')
        .and_then(|tail| tail.split_once(')'))
        .filter(|(scope, rest)| !scope.is_empty() && rest.starts_with(':'))
        .filter(|(scope, _)| deletes && text.cursor().1 <= scope.chars().count() + 2)
    else {
        return text.input(input);
    };

    let mut edited = text.clone();
    let changed = edited.input(input);
    let punctuation = edited.lines()[0].strip_suffix(rest);
    if !changed || !matches!(punctuation, Some("()" | "(" | ")" | "")) {
        *text = edited;
        return changed;
    }

    let removed = if rest[1..].trim().is_empty() {
        tail.chars().count()
    } else {
        scope.chars().count() + 2
    };
    text.cancel_selection();
    text.move_cursor(CursorMove::Head);
    let changed = text.delete_str(removed);
    text.set_yank_text(edited.yank_text());
    changed
}

fn type_subject(text: &mut TextArea<'_>, character: char) -> bool {
    const DESCRIPTION_START: &str = ": ";

    match character {
        ' ' if text.is_empty() => return text.insert_str(DESCRIPTION_START),
        ' ' if text.selection_range().is_none() => {
            let (_, column) = text.cursor();
            let tail = &text.lines()[0];
            if let Some((scope, rest)) =
                tail.strip_prefix('(').and_then(|tail| tail.split_once(')'))
                && rest == DESCRIPTION_START
                && column == scope.chars().count() + 1
            {
                let changed = if scope.is_empty() {
                    text.move_cursor(CursorMove::Head);
                    text.delete_str(2)
                } else {
                    false
                };
                text.move_cursor(CursorMove::End);
                return changed;
            }
        }
        '(' | ')' | ':' => {}
        _ if text.is_empty() && !character.is_whitespace() => {
            let changed = text.insert_str(format!("({character}){DESCRIPTION_START}"));
            text.move_cursor(CursorMove::Jump(0, 2));
            return changed;
        }
        _ => {}
    }
    text.insert_char(character);
    true
}

fn terminal(index: usize) -> u16 {
    u16::try_from(index).unwrap_or(u16::MAX)
}
