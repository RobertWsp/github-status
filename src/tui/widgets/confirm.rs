//! Confirm modal — a centered yes/no prompt for destructive actions.
//!
//! Rendered as an overlay (with a [`Clear`]) so it floats above the panes. The
//! styling leans on the theme's failure color to signal a destructive action.

use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph},
    Frame,
};

use crate::app::PendingDelete;
use crate::tui::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, pending: &PendingDelete, theme: &Theme) {
    let popup = centered(area, 60, 8);
    frame.render_widget(Clear, popup);

    let lines = vec![
        Line::from(Span::styled(pending.prompt(), theme.base().fg(theme.text))),
        Line::from(""),
        Line::from(Span::styled(
            "This only removes it from your dashboard — your GitHub repo is untouched.",
            theme.muted(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  y / Enter ", theme.confirm_yes()),
            Span::styled("confirm", theme.dim()),
            Span::styled("      n / Esc ", theme.key_hint()),
            Span::styled("cancel", theme.dim()),
        ]),
    ];

    let block = Block::default()
        .title(Span::styled(" Confirm removal ", theme.danger_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.danger_border())
        .padding(Padding::new(2, 2, 1, 1))
        .style(theme.base());

    frame.render_widget(Paragraph::new(lines).block(block), popup);
}

/// Center a fixed-size rect within `area`.
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [h] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [v] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(h);
    v
}
