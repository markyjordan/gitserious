use std::borrow::Borrow;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

/// The reason an open domain identifier is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentifierErrorKind {
    /// The identifier contains no characters.
    Empty,
    /// The first character is not a lowercase ASCII letter.
    InvalidStart,
    /// A later character is neither a lowercase ASCII letter, a digit, nor a hyphen.
    InvalidCharacter {
        /// The byte index of the invalid character.
        index: usize,
    },
    /// Two adjacent hyphens appear in the identifier.
    ConsecutiveHyphen {
        /// The byte index of the second hyphen.
        index: usize,
    },
    /// The identifier ends with a hyphen.
    TrailingHyphen,
}

/// An invalid commit-type, template, property, or condition identifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentifierError {
    value: Box<str>,
    kind: IdentifierErrorKind,
}

impl IdentifierError {
    /// Returns the rejected identifier text.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Returns the validation failure.
    #[must_use]
    pub const fn kind(&self) -> IdentifierErrorKind {
        self.kind
    }
}

impl Display for IdentifierError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self.kind {
            IdentifierErrorKind::Empty => formatter.write_str("identifier must not be empty"),
            IdentifierErrorKind::InvalidStart => write!(
                formatter,
                "identifier {:?} must start with a lowercase ASCII letter",
                self.value
            ),
            IdentifierErrorKind::InvalidCharacter { index } => write!(
                formatter,
                "identifier {:?} has an invalid character at byte index {index}",
                self.value
            ),
            IdentifierErrorKind::ConsecutiveHyphen { index } => write!(
                formatter,
                "identifier {:?} has consecutive hyphens at byte index {index}",
                self.value
            ),
            IdentifierErrorKind::TrailingHyphen => {
                write!(
                    formatter,
                    "identifier {:?} must not end with a hyphen",
                    self.value
                )
            }
        }
    }
}

impl Error for IdentifierError {}

fn validate_identifier(value: &str) -> Result<(), IdentifierErrorKind> {
    let bytes = value.as_bytes();
    let Some(first) = bytes.first() else {
        return Err(IdentifierErrorKind::Empty);
    };

    if !first.is_ascii_lowercase() {
        return Err(IdentifierErrorKind::InvalidStart);
    }

    let mut previous_was_hyphen = false;
    for (index, byte) in bytes.iter().copied().enumerate().skip(1) {
        if byte == b'-' {
            if previous_was_hyphen {
                return Err(IdentifierErrorKind::ConsecutiveHyphen { index });
            }
            previous_was_hyphen = true;
        } else if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
            previous_was_hyphen = false;
        } else {
            return Err(IdentifierErrorKind::InvalidCharacter { index });
        }
    }

    if previous_was_hyphen {
        return Err(IdentifierErrorKind::TrailingHyphen);
    }

    Ok(())
}

fn validate_and_box(value: String) -> Result<Box<str>, IdentifierError> {
    validate_identifier(&value).map_err(|kind| IdentifierError {
        value: value.clone().into_boxed_str(),
        kind,
    })?;
    Ok(value.into_boxed_str())
}

/// Why typed text has no identifier form.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LabelError {
    /// No ASCII letter or digit remains.
    Empty,
    /// The formatted text starts with a digit, such as "2fa".
    StartsWithDigit {
        /// The formatted text that would otherwise be the identifier.
        formatted: Box<str>,
    },
    /// A character that is not an ASCII letter or digit, an apostrophe, or a
    /// separator, such as 'é', '変', or an emoji.
    Unsupported {
        /// The first unsupported character in the typed text.
        character: char,
    },
}

impl Display for LabelError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("label has no ASCII letters or digits"),
            Self::StartsWithDigit { formatted } => {
                write!(formatter, "label {formatted:?} must start with a letter")
            }
            Self::Unsupported { character } => {
                write!(formatter, "label uses unsupported character {character:?}")
            }
        }
    }
}

impl Error for LabelError {}

/// Formats typed text as a lowercase ASCII kebab identifier.
///
/// ASCII letters are lowercased, ASCII digits are kept, and the apostrophes
/// U+0027, U+2019, and U+02BC are deleted so "user's guide" becomes
/// `users-guide`. Separators are ASCII punctuation and symbols, every
/// [`char::is_whitespace`] character, hyphens and dashes U+2010 to U+2015,
/// the quotes U+2018, U+201C, U+201D, and U+201E, the guillemets U+00AB,
/// U+00BB, U+2039, and U+203A, the ellipsis U+2026, the middle dot U+00B7,
/// and the bullet U+2022. Each run of separators becomes one hyphen and edge
/// separators are dropped. Every other character, including non-ASCII
/// letters, combining marks, invisible format characters, and emoji, is
/// rejected at its first occurrence rather than dropped, so no part of the
/// typed text disappears silently.
///
/// Every `Ok` value passes the strict identifier constructors, every strictly
/// valid identifier is returned unchanged, and the function is idempotent.
///
/// # Errors
///
/// Returns [`LabelError`] when the text has no letter or digit, formats to
/// text that starts with a digit, or contains an unsupported character.
pub fn normalize_identifier(label: &str) -> Result<String, LabelError> {
    let mut formatted = String::with_capacity(label.len());
    let mut separated = false;
    for character in label.chars() {
        match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' => {
                if separated && !formatted.is_empty() {
                    formatted.push('-');
                }
                separated = false;
                formatted.push(character.to_ascii_lowercase());
            }
            '\'' | '\u{2019}' | '\u{02BC}' => {}
            _ if is_label_separator(character) => separated = true,
            _ => return Err(LabelError::Unsupported { character }),
        }
    }
    match formatted.as_bytes().first() {
        None => Err(LabelError::Empty),
        Some(first) if first.is_ascii_digit() => Err(LabelError::StartsWithDigit {
            formatted: formatted.into_boxed_str(),
        }),
        Some(_) => Ok(formatted),
    }
}

fn is_label_separator(character: char) -> bool {
    character.is_ascii_punctuation()
        || character.is_whitespace()
        || matches!(
            character,
            '\u{2010}'..='\u{2015}'
                | '\u{2018}'
                | '\u{201C}'..='\u{201E}'
                | '\u{00AB}'
                | '\u{00BB}'
                | '\u{2039}'
                | '\u{203A}'
                | '\u{2026}'
                | '\u{00B7}'
                | '\u{2022}'
        )
}

macro_rules! define_identifier {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(Box<str>);

        impl $name {
            /// Creates a validated identifier.
            ///
            /// # Errors
            ///
            /// Returns [`IdentifierError`] when the value does not use lowercase
            /// ASCII kebab syntax.
            pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
                validate_and_box(value.into()).map(Self)
            }

            /// Returns the identifier text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Creates an identifier from typed text by formatting it with
            /// [`normalize_identifier`]. Stored and wire data use `new`.
            ///
            /// # Errors
            ///
            /// Returns [`LabelError`] when the text has no identifier form.
            #[allow(
                dead_code,
                reason = "crate-private identifiers have no typed-text editor"
            )]
            pub fn from_label(label: &str) -> Result<Self, LabelError> {
                let value = normalize_identifier(label)?;
                debug_assert!(
                    validate_identifier(&value).is_ok(),
                    "formatted label {value:?} is not a valid identifier"
                );
                Ok(Self(value.into_boxed_str()))
            }

            pub(crate) fn from_trusted(value: &'static str) -> Self {
                Self(Box::from(value))
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                self.as_str()
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = IdentifierError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = IdentifierError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdentifierError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
    };
}

define_identifier!(
    ChangeTypeId,
    "An open, validated identifier for a software change type."
);
/// Backward-compatible name for a software change-type identifier.
pub type CommitTypeId = ChangeTypeId;
define_identifier!(
    TaxonomyId,
    "An open, validated identifier for a software change taxonomy."
);
define_identifier!(
    TypesetId,
    "An open, validated identifier for a durable-property typeset."
);
define_identifier!(
    TemplateId,
    "An open, validated identifier for a commit-message template."
);
define_identifier!(
    PropertyKey,
    "An open, validated identifier for a durable message property."
);
define_identifier!(
    ConditionId,
    "An open, validated identifier for a conditional requirement rule."
);

#[cfg(test)]
mod tests {
    use std::borrow::Borrow;
    use std::collections::{BTreeSet, HashSet};
    use std::error::Error;

    use super::{
        CommitTypeId, ConditionId, IdentifierErrorKind, LabelError, PropertyKey, TaxonomyId,
        normalize_identifier,
    };
    use crate::{PropertyRequirement, built_in_taxonomies};

    #[test]
    fn accepts_lowercase_ascii_kebab_identifiers() -> Result<(), Box<dyn Error>> {
        let valid = [
            "a",
            "feat",
            "fix2",
            "a1",
            "a-1",
            "expected-behavior",
            "workflow-permissions-change",
            "x0-y9-z",
        ];

        for value in valid {
            assert_eq!(CommitTypeId::new(value)?.as_str(), value);
            assert_eq!(PropertyKey::new(value)?.as_str(), value);
            assert_eq!(ConditionId::new(value)?.as_str(), value);
        }

        Ok(())
    }

    #[test]
    fn rejects_invalid_identifier_shapes_with_precise_reasons() {
        let invalid = [
            ("", IdentifierErrorKind::Empty),
            ("Feat", IdentifierErrorKind::InvalidStart),
            ("1feat", IdentifierErrorKind::InvalidStart),
            ("-feat", IdentifierErrorKind::InvalidStart),
            (
                "feat_type",
                IdentifierErrorKind::InvalidCharacter { index: 4 },
            ),
            (
                "feat type",
                IdentifierErrorKind::InvalidCharacter { index: 4 },
            ),
            ("féat", IdentifierErrorKind::InvalidCharacter { index: 1 }),
            (
                "feat--type",
                IdentifierErrorKind::ConsecutiveHyphen { index: 5 },
            ),
            ("feat-", IdentifierErrorKind::TrailingHyphen),
        ];

        for (value, expected_kind) in invalid {
            let commit_type_error = CommitTypeId::new(value).err();
            let property_error = PropertyKey::new(value).err();
            let condition_error = ConditionId::new(value).err();

            assert_eq!(
                commit_type_error.as_ref().map(super::IdentifierError::kind),
                Some(expected_kind)
            );
            assert_eq!(
                property_error.as_ref().map(super::IdentifierError::kind),
                Some(expected_kind)
            );
            assert_eq!(
                condition_error.as_ref().map(super::IdentifierError::kind),
                Some(expected_kind)
            );
            assert_eq!(
                commit_type_error
                    .as_ref()
                    .map(super::IdentifierError::value),
                Some(value)
            );
        }
    }

    #[test]
    fn supports_parsing_conversion_display_borrowing_hashing_and_ordering()
    -> Result<(), Box<dyn Error>> {
        let parsed = "custom-type".parse::<CommitTypeId>()?;
        let borrowed = CommitTypeId::try_from("custom-type")?;
        let owned = CommitTypeId::try_from(String::from("custom-type"))?;

        assert_eq!(parsed, borrowed);
        assert_eq!(borrowed, owned);
        assert_eq!(parsed.to_string(), "custom-type");
        assert_eq!(AsRef::<str>::as_ref(&parsed), "custom-type");
        assert_eq!(Borrow::<str>::borrow(&parsed), "custom-type");

        let hash_values = HashSet::from([parsed.clone(), borrowed]);
        assert_eq!(hash_values.len(), 1);
        assert!(hash_values.contains("custom-type"));

        let ordered = BTreeSet::from([CommitTypeId::new("z-type")?, CommitTypeId::new("a-type")?]);
        let ordered_values = ordered.iter().map(CommitTypeId::as_str).collect::<Vec<_>>();
        assert_eq!(ordered_values, ["a-type", "z-type"]);

        Ok(())
    }

    #[test]
    fn identifier_errors_describe_the_rejected_rule() {
        let errors = [
            CommitTypeId::new("").err(),
            CommitTypeId::new("Upper").err(),
            CommitTypeId::new("bad_value").err(),
            CommitTypeId::new("bad--value").err(),
            CommitTypeId::new("bad-").err(),
        ];

        for error in errors.into_iter().flatten() {
            assert!(!error.to_string().is_empty());
            let source: &dyn Error = &error;
            assert!(source.source().is_none());
        }
    }

    #[test]
    fn formats_typed_labels_as_kebab_identifiers() {
        let cases = [
            ("two words", "two-words"),
            ("Team Changes", "team-changes"),
            ("team_changes", "team-changes"),
            ("Team", "team"),
            ("  lead--trail- ", "lead-trail"),
            ("a.b/c:d", "a-b-c-d"),
            ("a+b", "a-b"),
            ("user's guide", "users-guide"),
            ("user\u{2019}s guide", "users-guide"),
            ("GitHub", "github"),
            ("v2 API", "v2-api"),
            ("feat\n", "feat"),
            ("line one\r\nline two", "line-one-line-two"),
            ("pre\u{2013}release", "pre-release"),
            ("no\u{a0}break", "no-break"),
            (
                "\u{201C}quoted\u{201D} \u{2022} item\u{2026}",
                "quoted-item",
            ),
            ("already-kebab", "already-kebab"),
        ];

        for (label, expected) in cases {
            assert_eq!(
                normalize_identifier(label).as_deref(),
                Ok(expected),
                "{label:?}"
            );
        }
    }

    #[test]
    fn reports_why_a_label_has_no_identifier_form() {
        let cases = [
            ("", LabelError::Empty),
            ("   ", LabelError::Empty),
            ("---", LabelError::Empty),
            ("'", LabelError::Empty),
            (
                "2fa",
                LabelError::StartsWithDigit {
                    formatted: "2fa".into(),
                },
            ),
            (
                " 9 lives",
                LabelError::StartsWithDigit {
                    formatted: "9-lives".into(),
                },
            ),
            ("Café", LabelError::Unsupported { character: 'é' }),
            (
                "e\u{301}clair",
                LabelError::Unsupported {
                    character: '\u{301}',
                },
            ),
            ("Δ coverage", LabelError::Unsupported { character: 'Δ' }),
            ("変更", LabelError::Unsupported { character: '変' }),
            (
                "x\u{663}",
                LabelError::Unsupported {
                    character: '\u{663}',
                },
            ),
            (
                "e\u{1DC4}",
                LabelError::Unsupported {
                    character: '\u{1DC4}',
                },
            ),
            (
                "v\u{20D7}",
                LabelError::Unsupported {
                    character: '\u{20D7}',
                },
            ),
            (
                "text\u{FE0E}",
                LabelError::Unsupported {
                    character: '\u{FE0E}',
                },
            ),
            (
                "co\u{AD}op",
                LabelError::Unsupported {
                    character: '\u{AD}',
                },
            ),
            (
                "a\u{200D}b",
                LabelError::Unsupported {
                    character: '\u{200D}',
                },
            ),
            (
                "\u{1F680} launch",
                LabelError::Unsupported {
                    character: '\u{1F680}',
                },
            ),
        ];

        for (label, expected) in cases {
            assert_eq!(normalize_identifier(label), Err(expected), "{label:?}");
        }
    }

    #[test]
    fn every_label_from_a_small_alphabet_formats_to_its_predicted_result() {
        const SEPARATORS: [char; 4] = ['-', ' ', '_', '\n'];
        const UNSUPPORTED: [char; 3] = ['é', 'Δ', '\u{AD}'];
        const ALPHABET: [char; 11] = ['a', 'Z', '7', '\'', '-', ' ', '_', '\n', 'é', 'Δ', '\u{AD}'];

        let mut labels = vec![String::new()];
        let mut frontier = vec![String::new()];
        for _ in 0..4 {
            frontier = frontier
                .iter()
                .flat_map(|prefix| {
                    ALPHABET.iter().map(move |character| {
                        let mut label = prefix.clone();
                        label.push(*character);
                        label
                    })
                })
                .collect();
            labels.extend(frontier.iter().cloned());
        }
        assert_eq!(labels.len(), 16_105);

        for label in &labels {
            let expected = if let Some(character) = label.chars().find(|c| UNSUPPORTED.contains(c))
            {
                Err(LabelError::Unsupported { character })
            } else {
                let formatted = label
                    .replace('\'', "")
                    .to_ascii_lowercase()
                    .split(SEPARATORS)
                    .filter(|word| !word.is_empty())
                    .collect::<Vec<_>>()
                    .join("-");
                if formatted.is_empty() {
                    Err(LabelError::Empty)
                } else if formatted.starts_with('7') {
                    Err(LabelError::StartsWithDigit {
                        formatted: formatted.into_boxed_str(),
                    })
                } else {
                    Ok(formatted)
                }
            };
            let result = normalize_identifier(label);
            assert_eq!(result, expected, "{label:?}");
            assert_eq!(
                TaxonomyId::from_label(label).map(|id| id.as_str().to_owned()),
                result,
                "{label:?}"
            );
            if let Ok(formatted) = result {
                assert!(CommitTypeId::new(formatted.as_str()).is_ok(), "{label:?}");
                assert_eq!(normalize_identifier(&formatted), Ok(formatted.clone()));
                if CommitTypeId::new(label.as_str()).is_ok() {
                    assert_eq!(formatted, *label);
                }
            }
        }
    }

    #[test]
    fn formatting_leaves_valid_identifiers_unchanged() -> Result<(), Box<dyn Error>> {
        let mut identifiers = vec![
            "a".to_owned(),
            "fix2".to_owned(),
            "x0-y9-z".to_owned(),
            "workflow-permissions-change".to_owned(),
        ];
        for taxonomy in built_in_taxonomies() {
            identifiers.push(taxonomy.id().to_string());
            for kind in taxonomy.commit_types() {
                identifiers.push(kind.id().to_string());
                for property in kind.properties() {
                    identifiers.push(property.key().to_string());
                    if let PropertyRequirement::Conditional(condition) = property.requirement() {
                        identifiers.push(condition.id().to_string());
                    }
                }
            }
        }

        for identifier in &identifiers {
            assert_eq!(normalize_identifier(identifier)?, *identifier);
            assert_eq!(
                TaxonomyId::from_label(identifier)?,
                TaxonomyId::new(identifier.as_str())?
            );
            assert_eq!(
                ConditionId::from_label(identifier)?,
                ConditionId::new(identifier.as_str())?
            );
        }
        Ok(())
    }
}
