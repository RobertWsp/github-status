//! Terminal lifecycle — RAII guard for raw mode + alternate screen.
//!
//! Acquiring [`TerminalGuard`] enters raw mode and the alternate screen;
//! dropping it (even on panic, via a panic hook installed in `main`) restores
//! the terminal. Centralizing this prevents the classic "broken terminal after
//! crash" bug.

use std::io::{self, Stdout};

use color_eyre::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

/// Concrete terminal type used across the tui layer.
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// RAII guard that owns the terminal's raw/alt-screen state.
pub struct TerminalGuard {
    terminal: Tui,
}

impl TerminalGuard {
    /// Enter raw mode + alternate screen and build the ratatui terminal.
    pub fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    /// Mutable access to the underlying ratatui terminal for drawing.
    pub fn terminal(&mut self) -> &mut Tui {
        &mut self.terminal
    }

    /// Best-effort restore — also called from the panic hook.
    pub fn restore() -> io::Result<()> {
        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = Self::restore();
        let _ = self.terminal.show_cursor();
    }
}
