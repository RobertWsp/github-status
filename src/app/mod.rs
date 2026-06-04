//! Application layer — UI-agnostic orchestration and state.
//!
//! Depends on `domain` and `ports`; knows nothing about ratatui or octocrab.
//! The `tui` layer drives this state; a future `web` or `json` frontend could
//! reuse it unchanged.

pub mod action;
pub mod filter;
pub mod service;
pub mod state;
pub mod toast;

pub use action::Action;
pub use filter::Filter;
pub use service::StatusService;
pub use state::{AppState, Command, InputMode, PendingDelete};
pub use toast::{Toast, ToastKind};
