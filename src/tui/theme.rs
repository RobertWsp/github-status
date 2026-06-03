//! Theme — the Single Source of Truth for the color palette and styling.
//!
//! Every widget pulls colors and styles from here; no hard-coded `Color`
//! literals are allowed elsewhere in the `tui` layer. Swapping the palette
//! (e.g. a light variant) means editing only this file.
//!
//! Palette: a calm "midnight" scheme with semantic accent colors tuned for
//! truecolor terminals, with graceful meaning on 256-color terminals.

use ratatui::style::{Color, Modifier, Style};

use crate::app::ToastKind;
use crate::domain::RunState;

/// Centralized palette + style factory.
pub struct Theme {
    // Base surfaces
    pub bg: Color,
    pub surface: Color,
    pub overlay: Color,
    // Text
    pub text: Color,
    pub text_dim: Color,
    pub text_muted: Color,
    // Accents
    pub primary: Color,
    pub border: Color,
    pub border_focus: Color,
    // Semantic
    pub success: Color,
    pub failure: Color,
    pub warning: Color,
    pub running: Color,
    pub neutral: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::midnight()
    }
}

impl Theme {
    /// The default dark palette.
    pub fn midnight() -> Self {
        Self {
            bg: Color::Rgb(0x0d, 0x11, 0x17),
            surface: Color::Rgb(0x16, 0x1b, 0x22),
            overlay: Color::Rgb(0x1c, 0x21, 0x2b),
            text: Color::Rgb(0xc9, 0xd1, 0xd9),
            text_dim: Color::Rgb(0x8b, 0x94, 0x9e),
            text_muted: Color::Rgb(0x56, 0x5f, 0x6b),
            primary: Color::Rgb(0x58, 0xa6, 0xff),
            border: Color::Rgb(0x30, 0x36, 0x3d),
            border_focus: Color::Rgb(0x58, 0xa6, 0xff),
            success: Color::Rgb(0x3f, 0xb9, 0x50),
            failure: Color::Rgb(0xf8, 0x51, 0x49),
            warning: Color::Rgb(0xd2, 0x99, 0x22),
            running: Color::Rgb(0x58, 0xa6, 0xff),
            neutral: Color::Rgb(0x8b, 0x94, 0x9e),
        }
    }

    /// The semantic color for a run state — the only mapping from domain status
    /// to color, keeping presentation logic centralized.
    pub fn state_color(&self, state: RunState) -> Color {
        match state {
            RunState::Success => self.success,
            RunState::Failed | RunState::TimedOut => self.failure,
            RunState::Cancelled => self.text_muted,
            RunState::ActionRequired => self.warning,
            RunState::InProgress => self.running,
            RunState::Queued => self.warning,
            RunState::Skipped | RunState::Neutral => self.neutral,
            RunState::Unknown => self.text_dim,
        }
    }

    pub fn state_style(&self, state: RunState) -> Style {
        Style::default().fg(self.state_color(state))
    }

    /// Color for a toast of the given kind.
    pub fn toast_color(&self, kind: ToastKind) -> Color {
        match kind {
            ToastKind::Info => self.primary,
            ToastKind::Success => self.success,
            ToastKind::Warning => self.warning,
            ToastKind::Error => self.failure,
        }
    }

    /// Glyph for a toast of the given kind.
    pub fn toast_glyph(&self, kind: ToastKind) -> &'static str {
        match kind {
            ToastKind::Info => "ℹ",
            ToastKind::Success => "✓",
            ToastKind::Warning => "⚠",
            ToastKind::Error => "✗",
        }
    }

    // --- Style helpers (consistent text styling) ---

    pub fn base(&self) -> Style {
        Style::default().fg(self.text).bg(self.bg)
    }

    pub fn title(&self) -> Style {
        Style::default()
            .fg(self.primary)
            .add_modifier(Modifier::BOLD)
    }

    pub fn dim(&self) -> Style {
        Style::default().fg(self.text_dim)
    }

    pub fn muted(&self) -> Style {
        Style::default().fg(self.text_muted)
    }

    pub fn border_style(&self, focused: bool) -> Style {
        Style::default().fg(if focused {
            self.border_focus
        } else {
            self.border
        })
    }

    pub fn selected_row(&self) -> Style {
        Style::default()
            .bg(self.overlay)
            .add_modifier(Modifier::BOLD)
    }

    pub fn key_hint(&self) -> Style {
        Style::default()
            .fg(self.primary)
            .add_modifier(Modifier::BOLD)
    }

    /// Style for a filter chip in the header.
    pub fn chip(&self) -> Style {
        Style::default()
            .fg(self.bg)
            .bg(self.primary)
            .add_modifier(Modifier::BOLD)
    }

    /// Style for the active search input text.
    pub fn search_text(&self) -> Style {
        Style::default().fg(self.text).add_modifier(Modifier::BOLD)
    }

    /// Title style for a destructive modal.
    pub fn danger_title(&self) -> Style {
        Style::default()
            .fg(self.failure)
            .add_modifier(Modifier::BOLD)
    }

    /// Border style for a destructive modal.
    pub fn danger_border(&self) -> Style {
        Style::default().fg(self.failure)
    }

    /// Highlight style for the affirmative (destructive) confirm key.
    pub fn confirm_yes(&self) -> Style {
        Style::default()
            .fg(self.failure)
            .add_modifier(Modifier::BOLD)
    }

    /// Frames for the activity spinner (braille animation).
    pub const SPINNER: [&'static str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

    pub fn spinner_frame(&self, frame: usize) -> &'static str {
        Self::SPINNER[frame % Self::SPINNER.len()]
    }
}
