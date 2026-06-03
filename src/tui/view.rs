//! View — composes the overall layout from the individual widgets.
//!
//! Pure projection: given `&AppState`, draw the frame. No state mutation, no
//! I/O. This is the single place the screen layout is defined.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    widgets::Block,
    Frame,
};

use crate::app::AppState;
use crate::tui::{theme::Theme, widgets};

/// Draw the entire UI for the current frame.
pub fn render(frame: &mut Frame, state: &AppState, theme: &Theme) {
    // Paint the background.
    frame.render_widget(Block::default().style(theme.base()), frame.area());

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    widgets::header::render(frame, pad(header), state, theme);

    // Master/detail split: 45% list, 55% detail.
    let [list_area, detail_area] =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(body);

    // The list is focused for navigation only in normal mode; in search mode
    // the focus visually moves to the footer search box.
    let list_focused = matches!(state.mode, crate::app::InputMode::Normal);
    widgets::project_list::render(frame, list_area, state, theme, list_focused);
    widgets::detail::render(frame, detail_area, state, theme, false);
    widgets::footer::render(frame, pad(footer), state, theme);

    // Transient toast floats over the body (bottom-right), above the footer.
    if let Some(toast) = &state.toast {
        widgets::toast::render(frame, body, toast, theme);
    }

    if state.show_help {
        widgets::help::render(frame, frame.area(), theme);
    }

    // The confirm modal is the top-most overlay and captures focus.
    if let Some(pending) = &state.pending_delete {
        widgets::confirm::render(frame, frame.area(), pending, theme);
    }
}

/// Add one column of left padding to a single-row bar.
fn pad(area: Rect) -> Rect {
    Rect {
        x: area.x + 1,
        width: area.width.saturating_sub(1),
        ..area
    }
}
