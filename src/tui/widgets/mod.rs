//! Widgets — stateless rendering functions. Each takes `&AppState` (read-only)
//! plus the [`Theme`](crate::tui::theme::Theme) and draws into a `Rect`. They
//! never mutate state, keeping rendering a pure projection of state.

pub mod confirm;
pub mod detail;
pub mod footer;
pub mod header;
pub mod help;
pub mod project_list;
pub mod toast;
