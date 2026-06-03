//! TUI layer — the ratatui presentation + interactive runtime.
//!
//! Depends on `app` (state/actions) and `domain`, never the reverse. This layer
//! is replaceable: the same `app` could drive a plain `--json` printer.

pub mod event;
pub mod format;
pub mod keymap;
pub mod runtime;
pub mod terminal;
pub mod theme;
pub mod view;
pub mod widgets;

pub use runtime::Runtime;
pub use terminal::TerminalGuard;
