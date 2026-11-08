use std::io;

use ratatui::DefaultTerminal;
use ratatui::crossterm::{
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, enable_raw_mode},
};

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
