//! Detail pane — the selected project's recent run history.

use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Paragraph, Wrap},
    Frame,
};

use crate::app::AppState;
use crate::domain::{LoadState, ProjectStatus, WorkflowRun};
use crate::tui::{format, theme::Theme};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme, focused: bool) {
    let selected = state.selected_status();
    let title = selected
        .map(|s| format!(" {} ", s.project.slug()))
        .unwrap_or_else(|| " Details ".to_string());

    let block = Block::default()
        .title(Span::styled(title, theme.title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(focused))
        .padding(Padding::new(2, 2, 1, 1))
        .style(theme.base());

    let lines = match selected {
        None => vec![Line::from(Span::styled("Nothing selected.", theme.dim()))],
        Some(status) => detail_lines(status, theme),
    };

    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(para, area);
}

fn detail_lines<'a>(status: &'a ProjectStatus, theme: &Theme) -> Vec<Line<'a>> {
    match &status.load {
        LoadState::Idle => vec![hint(theme, "Press 'r' to load this project.")],
        LoadState::Loading => vec![hint(theme, "Loading…")],
        LoadState::Failed { message } => vec![
            Line::from(Span::styled(
                "Failed to fetch:",
                theme.state_style(crate::domain::RunState::Failed),
            )),
            Line::from(""),
            Line::from(Span::styled(message.clone(), theme.dim())),
        ],
        LoadState::Loaded { runs, fetched_at } => {
            if runs.is_empty() {
                return vec![hint(theme, "No workflow runs found for this project.")];
            }
            // Show how this project is being polled (adaptive tier) so the
            // cadence is transparent to the user.
            let tier = status.poll_tier(chrono::Utc::now());
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("Recent runs", theme.title()),
                    Span::styled(
                        format!("   fetched {}", format::relative_time(*fetched_at)),
                        theme.muted(),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("polling: ", theme.muted()),
                    Span::styled(tier.label(), theme.dim()),
                ]),
                Line::from(""),
            ];
            for run in runs {
                lines.extend(run_block(run, theme));
                lines.push(Line::from(""));
            }
            lines
        }
    }
}

fn run_block<'a>(run: &'a WorkflowRun, theme: &Theme) -> Vec<Line<'a>> {
    let style = theme.state_style(run.state);
    vec![
        Line::from(vec![
            Span::styled(format!("{} ", run.state.glyph()), style),
            Span::styled(run.state.label().to_string(), style),
            Span::styled(format!("  {}", run.name), theme.base().fg(theme.text)),
            Span::styled(format!("  #{}", run.run_number), theme.muted()),
        ]),
        Line::from(vec![
            Span::styled("    ⎇ ", theme.muted()),
            Span::styled(run.branch.clone(), theme.dim()),
            Span::styled(format!("  ◦ {}", run.short_sha), theme.muted()),
            Span::styled(format!("  ⚡ {}", run.event), theme.muted()),
        ]),
        Line::from(vec![
            Span::styled("    ", theme.muted()),
            Span::styled(
                format!(
                    "{} ago",
                    format::humanize_duration(chrono::Utc::now() - run.created_at)
                ),
                theme.muted(),
            ),
            Span::styled(
                format!("  ⏱ {}", format::humanize_duration(run.duration())),
                theme.muted(),
            ),
        ]),
    ]
}

fn hint<'a>(theme: &Theme, text: &'a str) -> Line<'a> {
    Line::from(Span::styled(text, theme.dim()))
}
