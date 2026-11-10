use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;

/// A run of help text; `strong` marks a heading or a command or flag name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelpSpan {
    pub text: String,
    pub strong: bool,
}

/// One line of help, already wrapped to the width it was rendered at.
pub type HelpLine = Vec<HelpSpan>;

/// One page of the Help section, rendered at the width the frame gives it.
#[derive(Clone)]
pub struct HelpPage {
    pub title: String,
    /// The command whose help the page shows; the overview documents none.
    pub command: Option<String>,
    render: Arc<dyn Fn(u16) -> Vec<HelpLine> + Send + Sync>,
}

impl HelpPage {
    pub fn new(
        title: impl Into<String>,
        command: Option<&str>,
        render: impl Fn(u16) -> Vec<HelpLine> + Send + Sync + 'static,
    ) -> Self {
        Self {
            title: title.into(),
            command: command.map(str::to_owned),
            render: Arc::new(render),
        }
    }

    /// A page whose `render` returns text in which SGR bold marks the strong spans.
    pub fn styled(
        title: impl Into<String>,
        command: Option<&str>,
        render: impl Fn(u16) -> String + Send + Sync + 'static,
    ) -> Self {
        Self::new(title, command, move |width| help_lines(&render(width)))
    }

    /// The page wrapped to `width` columns.
    #[must_use]
    pub fn lines(&self, width: u16) -> Vec<HelpLine> {
        (self.render)(width)
    }
}

impl Debug for HelpPage {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HelpPage")
            .field("title", &self.title)
            .field("command", &self.command)
            .finish_non_exhaustive()
    }
}

/// Splits rendered help into lines of spans that are strong while SGR bold is on.
fn help_lines(ansi: &str) -> Vec<HelpLine> {
    let mut pieces = ansi.split('\x1b');
    let mut lines = Lines::default();
    lines.push(pieces.next().unwrap_or_default());
    for piece in pieces {
        let Some(sequence) = piece.strip_prefix('[') else {
            lines.push(piece);
            continue;
        };
        let end = sequence
            .find(|character| ('@'..='~').contains(&character))
            .unwrap_or(sequence.len());
        let (parameters, rest) = sequence.split_at(end);
        let mut text = rest.chars();
        if text.next() == Some('m') {
            for parameter in parameters.split(';') {
                match parameter {
                    "1" => lines.strong = true,
                    "" | "0" | "22" => lines.strong = false,
                    _ => {}
                }
            }
        }
        lines.push(text.as_str());
    }
    lines.finish()
}

#[derive(Default)]
struct Lines {
    done: Vec<HelpLine>,
    current: HelpLine,
    strong: bool,
}

impl Lines {
    fn push(&mut self, text: &str) {
        for (index, part) in text.split('\n').enumerate() {
            if index > 0 {
                self.done.push(std::mem::take(&mut self.current));
            }
            if part.is_empty() {
                continue;
            }
            match self.current.last_mut() {
                Some(span) if span.strong == self.strong => span.text.push_str(part),
                _ => self.current.push(HelpSpan {
                    text: part.into(),
                    strong: self.strong,
                }),
            }
        }
    }

    /// The lines without trailing blank ones, as `printed` trims them.
    fn finish(mut self) -> Vec<HelpLine> {
        self.done.push(self.current);
        while self
            .done
            .last()
            .is_some_and(|line| line.iter().all(|span| span.text.trim().is_empty()))
        {
            self.done.pop();
        }
        self.done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str, strong: bool) -> HelpSpan {
        HelpSpan {
            text: text.into(),
            strong,
        }
    }

    #[test]
    fn help_spans_are_strong_only_while_sgr_bold_is_on() {
        let help = "\x1b[1mUsage:\x1b[m a\x1b[1;4mb\x1b[22mc\x1b[1m\nd\x1b[0me\x1b[32mf\x1b[1m\n\n";

        assert_eq!(
            help_lines(help),
            [
                vec![
                    span("Usage:", true),
                    span(" a", false),
                    span("b", true),
                    span("c", false),
                ],
                vec![span("d", true), span("ef", false)],
            ]
        );
    }
}
