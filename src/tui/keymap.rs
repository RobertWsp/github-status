//! Keymap — pure mapping from key events to [`Action`]s.
//!
//! Isolated so the bindings are documented in one place and unit-testable
//! without a terminal. The help overlay is rendered from [`HELP_BINDINGS`] so
//! the docs and behavior can never drift (SSoT for key bindings).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{Action, InputMode};

/// Human-readable binding rows for the help overlay: `(keys, description)`.
/// SSoT for key bindings — the help overlay renders straight from this.
pub const HELP_BINDINGS: &[(&str, &str)] = &[
    ("↑/k  ↓/j", "Move up / down"),
    ("g / G", "Jump to top / bottom"),
    ("r", "Refresh all projects"),
    ("/", "Search — type to filter, Enter applies"),
    ("f", "Show only the selected owner"),
    ("c", "Clear search & filters"),
    ("o / Enter", "Open latest run in browser"),
    ("?", "Toggle this help"),
    ("Esc", "Exit search / clear filter / quit"),
    ("q / Ctrl-C", "Quit"),
];

/// Translate a key event into an action, given the current input [`InputMode`].
/// Returns [`Action::Noop`] for unbound keys so the reducer stays total.
pub fn map_key(key: KeyEvent, mode: InputMode) -> Action {
    // Ctrl-C always quits, regardless of mode.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Action::Quit;
    }

    match mode {
        InputMode::Search => map_search_key(key),
        InputMode::Normal => map_normal_key(key),
    }
}

/// Bindings while typing a search query.
fn map_search_key(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => Action::Escape,
        KeyCode::Enter => Action::ConfirmSearch,
        KeyCode::Backspace => Action::SearchBackspace,
        KeyCode::Up => Action::Up,
        KeyCode::Down => Action::Down,
        // Plain characters extend the query (ignore modified chords).
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            Action::SearchInput(c)
        }
        _ => Action::Noop,
    }
}

/// Bindings in normal navigation mode.
fn map_normal_key(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Esc => Action::Escape,
        KeyCode::Up | KeyCode::Char('k') => Action::Up,
        KeyCode::Down | KeyCode::Char('j') => Action::Down,
        KeyCode::Char('g') => Action::Top,
        KeyCode::Char('G') => Action::Bottom,
        KeyCode::Char('r') => Action::Refresh,
        KeyCode::Char('/') => Action::EnterSearch,
        KeyCode::Char('f') => Action::FilterSelectedOwner,
        KeyCode::Char('c') => Action::ClearFilters,
        KeyCode::Char('o') | KeyCode::Enter => Action::OpenInBrowser,
        KeyCode::Char('?') => Action::ToggleHelp,
        _ => Action::Noop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn ctrl_c_quits_in_any_mode() {
        let k = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(matches!(map_key(k, InputMode::Normal), Action::Quit));
        assert!(matches!(map_key(k, InputMode::Search), Action::Quit));
    }

    #[test]
    fn vim_and_arrows_navigate_in_normal() {
        assert!(matches!(
            map_key(key(KeyCode::Char('j')), InputMode::Normal),
            Action::Down
        ));
        assert!(matches!(
            map_key(key(KeyCode::Down), InputMode::Normal),
            Action::Up | Action::Down
        ));
        assert!(matches!(
            map_key(key(KeyCode::Char('k')), InputMode::Normal),
            Action::Up
        ));
    }

    #[test]
    fn slash_enters_search() {
        assert!(matches!(
            map_key(key(KeyCode::Char('/')), InputMode::Normal),
            Action::EnterSearch
        ));
    }

    #[test]
    fn typing_in_search_mode_feeds_query() {
        assert!(matches!(
            map_key(key(KeyCode::Char('a')), InputMode::Search),
            Action::SearchInput('a')
        ));
        assert!(matches!(
            map_key(key(KeyCode::Enter), InputMode::Search),
            Action::ConfirmSearch
        ));
        assert!(matches!(
            map_key(key(KeyCode::Backspace), InputMode::Search),
            Action::SearchBackspace
        ));
    }

    #[test]
    fn unbound_is_noop() {
        assert!(matches!(
            map_key(key(KeyCode::Char('z')), InputMode::Normal),
            Action::Noop
        ));
    }
}
