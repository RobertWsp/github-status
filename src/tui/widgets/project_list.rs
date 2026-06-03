//! Project list — the master pane: one row per *visible* project with its
//! headline status badge, branch, and last CI activity time.
//!
//! Iterates [`AppState::visible_indices`] (the derived, filtered projection) so
//! search and owner filters apply without duplicating the project list.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Padding, Paragraph},
    Frame,
};

use crate::app::AppState;
use crate::domain::{LoadState, ProjectStatus, RunState};
use crate::tui::{format, theme::Theme};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme, focused: bool) {
    let visible = state.visible_indices();

    let title = if state.has_active_filter() {
        format!(" Projects ({}/{}) ", visible.len(), state.projects.len())
    } else {
        " Projects ".to_string()
    };

    let block = Block::default()
        .title(Line::from(vec![Span::styled(title, theme.title())]))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(focused))
        .padding(Padding::horizontal(1))
        .style(theme.base());

    // Empty states: distinguish "no projects at all" from "filtered to nothing".
    if state.projects.is_empty() {
        frame.render_widget(empty_no_projects(theme).block(block), area);
        return;
    }
    if visible.is_empty() {
        frame.render_widget(empty_no_matches(theme).block(block), area);
        return;
    }

    let items: Vec<ListItem> = visible
        .iter()
        .map(|&i| row(&state.projects[i], theme, state.spinner_frame))
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(theme.selected_row())
        .highlight_symbol("▌ ");

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn empty_no_projects(theme: &Theme) -> Paragraph<'static> {
    Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled("  No projects yet.", theme.dim())),
        Line::from(""),
        Line::from(Span::styled(
            "  Add repositories from your shell:",
            theme.muted(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("    ghs add ", theme.key_hint()),
            Span::styled("owner/repo", theme.dim()),
        ]),
        Line::from(vec![
            Span::styled("    ghs import --local", theme.key_hint()),
            Span::styled("   scan git clones", theme.muted()),
        ]),
        Line::from(vec![
            Span::styled("    ghs import --me", theme.key_hint()),
            Span::styled("      your GitHub repos", theme.muted()),
        ]),
    ])
}

fn empty_no_matches(theme: &Theme) -> Paragraph<'static> {
    Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled("  No projects match the filter.", theme.dim())),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Press ", theme.muted()),
            Span::styled("c", theme.key_hint()),
            Span::styled(" to clear filters.", theme.muted()),
        ]),
    ])
}

/// Render a single project row as a styled line.
fn row<'a>(p: &'a ProjectStatus, theme: &Theme, spinner: usize) -> ListItem<'a> {
    let name = p.project.display_name();

    let (badge, trailing): (Span, Vec<Span>) = match &p.load {
        LoadState::Idle => (
            Span::styled("  · ", theme.muted()),
            vec![Span::styled("idle", theme.muted())],
        ),
        LoadState::Loading => (
            Span::styled(
                format!(" {} ", theme.spinner_frame(spinner)),
                theme.state_style(RunState::InProgress),
            ),
            vec![Span::styled("loading…", theme.dim())],
        ),
        LoadState::Failed { message } => (
            Span::styled(" ✗ ", theme.state_style(RunState::Failed)),
            vec![Span::styled(
                truncate(message, 40),
                theme.state_style(RunState::Failed),
            )],
        ),
        LoadState::Loaded { runs, .. } => {
            if let Some(run) = runs.first() {
                let style = theme.state_style(run.state);
                let badge = Span::styled(format!(" {} ", run.state.glyph()), style);
                let trailing = vec![
                    Span::styled(run.state.label().to_string(), style),
                    Span::styled(format!("  ⎇ {}", run.branch), theme.dim()),
                    // Show when the CI actually last ran (not when we fetched).
                    Span::styled(
                        format!("  · {}", format::relative_time(run.updated_at)),
                        theme.muted(),
                    ),
                ];
                (badge, trailing)
            } else {
                (
                    Span::styled("  – ", theme.muted()),
                    vec![Span::styled("no runs", theme.muted())],
                )
            }
        }
    };

    let mut spans = vec![
        badge,
        Span::styled(pad_name(&name, 26), theme.base().fg(theme.text)),
    ];
    spans.extend(trailing);
    ListItem::new(Line::from(spans))
}

fn pad_name(s: &str, width: usize) -> String {
    let t = truncate(s, width);
    format!("{t:<width$}", width = width)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}
