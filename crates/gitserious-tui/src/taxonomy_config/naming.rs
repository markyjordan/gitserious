use std::borrow::Cow;

use gitserious_core::{LabelError, normalize_identifier};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use super::taxonomy_heading_line;
use crate::theme::{frame_style, section_heading_style};

const MINIMUM_NOTE_WIDTH: usize = 12;

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

fn name_note(raw: &str) -> Option<String> {
    if raw.trim().is_empty() {
        return None;
    }
    match normalize_identifier(raw) {
        Ok(formatted) if formatted == raw => None,
        Ok(formatted) => Some(format!("saves as {formatted}")),
        Err(LabelError::StartsWithDigit { .. }) => Some("start with a letter".into()),
        Err(LabelError::Empty | LabelError::Unsupported { .. }) => Some("use letters a-z".into()),
    }
}

/// A Name heading whose rule ends with the identifier the typed text saves as.
pub(super) fn name_heading_line(title: &str, raw: &str, width: u16) -> Line<'static> {
    noted_heading_line(title, name_note(raw), width)
}

/// A heading whose rule ends with `note`, or a plain heading when there is no room for it.
pub(super) fn noted_heading_line(title: &str, note: Option<String>, width: u16) -> Line<'static> {
    let available = usize::from(width).saturating_sub(title.len() + 1);
    let Some(note) = note.filter(|_| available >= MINIMUM_NOTE_WIDTH) else {
        return taxonomy_heading_line(title, width);
    };
    // Notes are ASCII, so byte length is display width and slicing is safe.
    let room = available - 2;
    let (note, note_width) = if note.len() > room {
        (format!("{}…", &note[..room - 1]), room)
    } else {
        let note_width = note.len();
        (note, note_width)
    };
    let rule_width = available - 1 - note_width;
    Line::from(vec![
        Span::styled(title.to_owned(), section_heading_style().fg(Color::White)),
        Span::styled(format!(" {} ", "⠒".repeat(rule_width)), frame_style()),
        Span::styled(note, Style::default().fg(Color::White)),
    ])
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

/// Trims edited prose at its edges. Unchanged loaded prose stays byte for byte.
pub(super) fn edited_prose(text: String, loaded: &str) -> String {
    if text == loaded {
        text
    } else {
        text.trim().to_owned()
    }
}
