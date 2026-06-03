//! Footer — context-sensitive key hints + an inline search prompt.
//!
//! The hint set changes with the input mode and whether filters are active, so
//! the bottom bar always reflects what the user can do *right now*.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{AppState, InputMode};
use crate::tui::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let line = match state.mode {
        InputMode::Search => search_prompt(state, theme),
        InputMode::Normal => normal_hints(state, theme),
    };
    frame.render_widget(Paragraph::new(line).style(theme.base()), area);
}

/// The live search prompt with a blinking-style caret.
fn search_prompt<'a>(state: &'a AppState, theme: &Theme) -> Line<'a> {
    Line::from(vec![
        Span::styled(" / ", theme.key_hint()),
        Span::styled(&state.query, theme.search_text()),
        Span::styled("▏", theme.title()), // caret
        Span::styled(
            format!("    {} match(es)", state.visible_count()),
            theme.muted(),
        ),
        Span::styled("    Enter ", theme.key_hint()),
        Span::styled("apply", theme.dim()),
        Span::styled("  Esc ", theme.key_hint()),
        Span::styled("cancel", theme.dim()),
    ])
}

/// Normal-mode hint bar; includes "clear" only when a filter is active.
fn normal_hints<'a>(state: &AppState, theme: &Theme) -> Line<'a> {
    let mut hints: Vec<(&str, &str)> = vec![
        ("↑↓", "navigate"),
        ("/", "search"),
        ("f", "owner"),
        ("r", "refresh"),
        ("o", "open"),
    ];
    if state.has_active_filter() {
        hints.push(("c", "clear"));
    }
    hints.push(("?", "help"));
    hints.push(("q", "quit"));

    let mut spans = vec![Span::raw(" ")];
    for (i, (key, desc)) in hints.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ·  ", theme.muted()));
        }
        spans.push(Span::styled(key.to_string(), theme.key_hint()));
        spans.push(Span::styled(format!(" {desc}"), theme.dim()));
    }
    Line::from(spans)
}
