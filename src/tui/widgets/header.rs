//! Header bar — app title, counts, active-filter chips, and activity spinner.

use ratatui::{
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::AppState;
use crate::domain::RunState;
use crate::tui::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let total = state.projects.len();
    let visible = state.visible_count();

    let mut spans = vec![
        Span::styled(" ⬡ ", theme.title()),
        Span::styled("GitHub Actions Status", theme.title()),
    ];

    // Count: "12 projects" or "3 / 12 shown" when filtering.
    if state.has_active_filter() {
        spans.push(Span::styled(
            format!("  ·  {visible} / {total} shown"),
            theme.dim(),
        ));
    } else {
        spans.push(Span::styled(
            format!("  ·  {total} project{}", if total == 1 { "" } else { "s" }),
            theme.dim(),
        ));
    }

    // Filter chips.
    if let Some(owner) = &state.filter.owner {
        spans.push(Span::raw("  "));
        spans.push(chip(theme, "owner", owner));
    }
    if let Some(q) = state.filter.query_text() {
        spans.push(Span::raw("  "));
        spans.push(chip(theme, "search", q));
    }

    // Activity spinner.
    if state.is_loading() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            theme.spinner_frame(state.spinner_frame),
            theme.state_style(RunState::InProgress),
        ));
        spans.push(Span::styled(
            format!(" refreshing {} …", state.inflight),
            theme.dim(),
        ));
    }

    let para = Paragraph::new(Line::from(spans))
        .style(theme.base())
        .alignment(Alignment::Left);
    frame.render_widget(para, area);
}

/// A small "key:value" filter chip.
fn chip<'a>(theme: &Theme, key: &'a str, value: &'a str) -> Span<'a> {
    Span::styled(format!(" {key}:{value} "), theme.chip())
}
