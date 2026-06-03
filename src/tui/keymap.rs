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
    ("d", "Remove the selected repo"),
    ("D", "Remove ALL repos of the owner"),
    ("o / Enter", "Open latest run in browser"),
    ("?", "Toggle this help"),
    ("Esc", "Exit search / clear filter / quit"),
    ("q / Ctrl-C", "Quit"),
];

/// Translate a key event into an action, given the current input [`InputMode`]
/// and whether a delete-confirmation prompt is open. Returns [`Action::Noop`]
/// for unbound keys so the reducer stays total.
pub fn map_key(key: KeyEvent, mode: InputMode, confirming: bool) -> Action {
    // Ctrl-C always quits, regardless of mode.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Action::Quit;
    }

    // A confirmation modal captures all other input until answered.
    if confirming {
        return map_confirm_key(key);
    }

    match mode {
        InputMode::Search => map_search_key(key),
        InputMode::Normal => map_normal_key(key),
    }
}

/// Bindings while a yes/no delete prompt is open.
fn map_confirm_key(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => Action::ConfirmDelete,
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => Action::CancelDelete,
        _ => Action::Noop,
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
        KeyCode::Char('d') => Action::RequestDeleteRepo,
        KeyCode::Char('D') => Action::RequestDeleteOwner,
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

    /// Map in normal mode without a confirm prompt.
    fn mk(code: KeyCode, mode: InputMode) -> Action {
        map_key(key(code), mode, false)
    }

    #[test]
    fn ctrl_c_quits_in_any_mode() {
        let k = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(matches!(map_key(k, InputMode::Normal, false), Action::Quit));
        assert!(matches!(map_key(k, InputMode::Search, true), Action::Quit));
    }

    #[test]
    fn delete_keys_request_removal() {
        assert!(matches!(
            mk(KeyCode::Char('d'), InputMode::Normal),
            Action::RequestDeleteRepo
        ));
        assert!(matches!(
            mk(KeyCode::Char('D'), InputMode::Normal),
            Action::RequestDeleteOwner
        ));
    }

    #[test]
    fn confirm_modal_captures_yes_no() {
        assert!(matches!(
            map_key(key(KeyCode::Char('y')), InputMode::Normal, true),
            Action::ConfirmDelete
        ));
        assert!(matches!(
            map_key(key(KeyCode::Char('n')), InputMode::Normal, true),
            Action::CancelDelete
        ));
        assert!(matches!(
            map_key(key(KeyCode::Enter), InputMode::Normal, true),
            Action::ConfirmDelete
        ));
        // While confirming, navigation keys are inert.
        assert!(matches!(
            map_key(key(KeyCode::Char('j')), InputMode::Normal, true),
            Action::Noop
        ));
    }

    #[test]
    fn vim_and_arrows_navigate_in_normal() {
        assert!(matches!(
            mk(KeyCode::Char('j'), InputMode::Normal),
            Action::Down
        ));
        assert!(matches!(
            mk(KeyCode::Down, InputMode::Normal),
            Action::Up | Action::Down
        ));
        assert!(matches!(
            mk(KeyCode::Char('k'), InputMode::Normal),
            Action::Up
        ));
    }

    #[test]
    fn slash_enters_search() {
        assert!(matches!(
            mk(KeyCode::Char('/'), InputMode::Normal),
            Action::EnterSearch
        ));
    }

    #[test]
    fn typing_in_search_mode_feeds_query() {
        assert!(matches!(
            mk(KeyCode::Char('a'), InputMode::Search),
            Action::SearchInput('a')
        ));
        assert!(matches!(
            mk(KeyCode::Enter, InputMode::Search),
            Action::ConfirmSearch
        ));
        assert!(matches!(
            mk(KeyCode::Backspace, InputMode::Search),
            Action::SearchBackspace
        ));
    }

    #[test]
    fn unbound_is_noop() {
        assert!(matches!(
            mk(KeyCode::Char('z'), InputMode::Normal),
            Action::Noop
        ));
    }
}
