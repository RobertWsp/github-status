//! Toast widget — a small, auto-fading notification in the bottom-right.
//!
//! Rendered as an overlay so it floats above the panes without shifting layout.
//! The fade-out is driven by the toast's remaining `ttl` (see [`Toast::tick`]).

use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph},
    Frame,
};

use crate::app::Toast;
use crate::tui::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, toast: &Toast, theme: &Theme) {
    let color = theme.toast_color(toast.kind);
    let glyph = theme.toast_glyph(toast.kind);

    // Size the box to the message, clamped to the available width.
    let text = format!("{glyph}  {}", toast.message);
    let inner_w = (text.chars().count() as u16).min(area.width.saturating_sub(6));
    let box_w = inner_w + 4; // borders + padding
    let box_h = 3;

    // Anchor bottom-right, just above the footer.
    let x = area.x + area.width.saturating_sub(box_w + 1);
    let y = area.y + area.height.saturating_sub(box_h + 1);
    let rect = Rect {
        x,
        y,
        width: box_w,
        height: box_h,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
        .padding(Padding::horizontal(1))
        .style(theme.base());

    let para = Paragraph::new(Line::from(vec![Span::styled(
        text,
        Style::default().fg(color),
    )]))
    .block(block);

    frame.render_widget(Clear, rect);
    frame.render_widget(para, rect);
}
