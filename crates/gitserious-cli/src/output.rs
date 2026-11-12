use std::fmt::Display;
use std::io::Write;
use std::process::ExitCode;

use gitserious_app::{InitOutcome, InitStatus};
use gitserious_core::TaxonomyReference;

use crate::invocation::Answer;

/// Writes `answer` and returns the exit code it carries.
pub fn write_answer(
    answer: Answer,
    stdout: &mut (impl Write + ?Sized),
    stderr: &mut (impl Write + ?Sized),
) -> ExitCode {
    match answer {
        Answer::Print(text) => write_all(stdout, &text),
        Answer::Reject { message, code } => {
            if stderr.write_all(message.as_bytes()).is_ok() {
                code
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

fn write_all(stdout: &mut (impl Write + ?Sized), text: &str) -> ExitCode {
    if stdout.write_all(text.as_bytes()).is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Writes the line that reports `outcome`.
pub fn write_init_outcome(output: &mut (impl Write + ?Sized), outcome: &InitOutcome) -> ExitCode {
    let action = match outcome.status() {
        InitStatus::Initialized => "Initialized gitserious in",
        InitStatus::AlreadyInitialized => "gitserious is already initialized in",
    };
    if writeln!(
        output,
        "{action} {} ({}).",
        outcome.root().as_path().display(),
        TaxonomyReference::new(outcome.taxonomy(), outcome.version())
    )
    .is_ok()
    {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Writes `error` as an `error: ` line and returns failure.
pub fn write_error(output: &mut (impl Write + ?Sized), error: impl Display) -> ExitCode {
    let _ = writeln!(output, "error: {error}");
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Closed;

    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn answers_write_their_text_to_one_stream_with_their_exit_code() {
        let rejected = || Answer::Reject {
            message: "error: unexpected argument\n".into(),
            code: ExitCode::from(2),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let printed = write_answer(Answer::Print("help\n".into()), &mut stdout, &mut stderr);
        let rejection = write_answer(rejected(), &mut stdout, &mut stderr);

        assert_eq!((printed, rejection), (ExitCode::SUCCESS, ExitCode::from(2)));
        assert_eq!(stdout, b"help\n");
        assert_eq!(stderr, b"error: unexpected argument\n");
        assert_eq!(
            write_answer(Answer::Print("help\n".into()), &mut Closed, &mut Vec::new()),
            ExitCode::FAILURE
        );
        assert_eq!(
            write_answer(rejected(), &mut Vec::new(), &mut Closed),
            ExitCode::FAILURE
        );
    }

    #[test]
    fn errors_are_one_prefixed_line_and_fail_even_when_unwritable() {
        let mut stderr = Vec::new();

        assert_eq!(
            write_error(&mut stderr, "help shell rejected"),
            ExitCode::FAILURE
        );
        assert_eq!(stderr, b"error: help shell rejected\n");
        assert_eq!(write_error(&mut Closed, "lost"), ExitCode::FAILURE);
    }
}
