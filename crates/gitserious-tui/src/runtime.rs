use std::io;

use ratatui::DefaultTerminal;
use ratatui::crossterm::{
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
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
