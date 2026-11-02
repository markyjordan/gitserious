use std::io;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event,
    },
    execute,
    terminal::{EnterAlternateScreen, enable_raw_mode},
};

pub(crate) fn next_event(deadline: Option<Duration>) -> io::Result<Option<Event>> {
    read_until(deadline, event::poll, event::read)
}

fn read_until<T>(
    deadline: Option<Duration>,
    mut poll: impl FnMut(Duration) -> io::Result<bool>,
    mut read: impl FnMut() -> io::Result<T>,
) -> io::Result<Option<T>> {
    if let Some(delay) = deadline
        && !poll(delay)?
    {
        return Ok(None);
    }
    read().map(Some)
}

/// Additional input modes are scoped to the terminal runtime, including unwinding.
struct InputModes;
impl InputModes {
    fn enable() -> io::Result<Self> {
        execute!(io::stdout(), EnableMouseCapture, EnableBracketedPaste)?;
        Ok(Self)
    }
    fn disable() -> io::Result<()> {
        execute!(io::stdout(), DisableBracketedPaste, DisableMouseCapture)
    }
}
impl Drop for InputModes {
    fn drop(&mut self) {
        let _ = Self::disable();
    }
}

pub(crate) fn run<T>(
    operation: impl FnOnce(&mut DefaultTerminal) -> io::Result<T>,
) -> io::Result<T> {
    ratatui::run(|terminal| {
        let _modes = InputModes::enable()?;
        operation(terminal)
    })
}

/// Let Git, signing agents, and hooks use a normal terminal, then resume the same UI.
pub(crate) fn suspended<T>(
    terminal: &mut DefaultTerminal,
    operation: impl FnOnce() -> T,
) -> io::Result<T> {
    InputModes::disable()?;
    terminal.show_cursor()?;
    ratatui::restore();
    let result = operation();
    enable_raw_mode()?;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    // Fullscreen resize resets both buffers without a cursor-position query. A bare PTY
    // need not answer terminal device-status reports, and Git may have moved the cursor.
    terminal.resize(terminal.size()?.into())?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_views_block_for_input_without_polling() -> io::Result<()> {
        let result = read_until(None, |_| unreachable!("no deadline"), || Ok(7))?;
        assert_eq!(result, Some(7));
        Ok(())
    }

    #[test]
    fn animation_deadline_redraws_without_reading_a_key() -> io::Result<()> {
        let delay = Duration::from_millis(150);
        let result: Option<u8> = read_until(
            Some(delay),
            |received| {
                assert_eq!(received, delay);
                Ok(false)
            },
            || unreachable!("timeout must return to rendering"),
        )?;
        assert_eq!(result, None);
        let result = read_until(Some(delay), |_| Ok(true), || Ok(9))?;
        assert_eq!(result, Some(9));
        Ok(())
    }
}
