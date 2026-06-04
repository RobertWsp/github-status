//! Actions — the only way to mutate [`AppState`] (Elm-style message passing).
//!
//! Terminal input and background fetch results are both normalized into
//! `Action`s, which `AppState::update` reduces. This keeps state transitions in
//! one auditable place and makes the UI trivially testable.

use crate::domain::WorkflowRun;
use crate::ports::ProviderError;

/// A message that drives a state transition.
#[derive(Debug)]
pub enum Action {
    /// Move the list selection.
    Up,
    Down,
    /// Jump to the first/last project.
    Top,
    Bottom,
    /// Force-refresh all currently visible projects now (manual `r`).
    Refresh,
    /// Scheduler tick: poll only the visible projects that are *due* per their
    /// adaptive tier. Emitted by the runtime on its timer.
    PollDue,
    /// Open the selected project's latest run in a browser.
    OpenInBrowser,
    /// Toggle the help overlay.
    ToggleHelp,
    /// Enter incremental-search mode.
    EnterSearch,
    /// Append a character to the active search query.
    SearchInput(char),
    /// Delete the last character of the search query.
    SearchBackspace,
    /// Leave search mode, keeping the current query as an active filter.
    ConfirmSearch,
    /// Limit the list to the selected project's owner (company/org).
    FilterSelectedOwner,
    /// Clear all active filters (search query + owner).
    ClearFilters,
    /// Ask to remove the selected repository (opens a confirm prompt).
    RequestDeleteRepo,
    /// Ask to remove every repo of the selected owner (company).
    RequestDeleteOwner,
    /// Confirm the pending delete.
    ConfirmDelete,
    /// Dismiss the pending delete without acting.
    CancelDelete,
    /// Context-sensitive escape: confirm prompt → cancel, search → exit,
    /// filters → clear, else quit.
    Escape,
    /// Request to quit.
    Quit,
    /// A periodic tick (auto-refresh timer / spinner animation).
    Tick,
    /// A background fetch finished for the project at `index`.
    FetchCompleted {
        index: usize,
        result: Result<Vec<WorkflowRun>, ProviderError>,
    },
    /// No-op (e.g. an unmapped key) — keeps the reducer total.
    Noop,
}
