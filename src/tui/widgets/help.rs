//! Help overlay — a centered modal listing all key bindings (from the keymap
//! SSoT) plus a brief legend of status glyphs.

use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph},
    Frame,
};

use crate::domain::RunState;
use crate::tui::{keymap::HELP_BINDINGS, theme::Theme};

pub fn render(frame: &mut Frame, area: Rect, theme: &Theme) {
    let popup = centered(area, 64, 24);
    frame.render_widget(Clear, popup);

    let mut lines = vec![
        Line::from(Span::styled("Key bindings", theme.title())),
        Line::from(""),
    ];
    for (keys, desc) in HELP_BINDINGS {
        lines.push(Line::from(vec![
            Span::styled(format!("  {keys:<18}"), theme.key_hint()),
            Span::styled(*desc, theme.dim()),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Status legend", theme.title())));
    lines.push(Line::from(""));
    for state in LEGEND {
        lines.push(Line::from(vec![
            Span::styled(format!("  {}  ", state.glyph()), theme.state_style(*state)),
            Span::styled(state.label(), theme.state_style(*state)),
        ]));
    }

    let block = Block::default()
        .title(Span::styled(" Help ", theme.title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(true))
        .padding(Padding::new(2, 2, 1, 1))
        .style(theme.base());

    frame.render_widget(Paragraph::new(lines).block(block), popup);
}

const LEGEND: &[RunState] = &[
    RunState::Success,
    RunState::Failed,
    RunState::InProgress,
    RunState::Queued,
    RunState::Cancelled,
];

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
