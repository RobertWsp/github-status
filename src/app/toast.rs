//! Transient toast notifications — lightweight, time-decaying user feedback.
//!
//! The reducer sets a toast on meaningful events (refresh started, filter
//! applied, browser opened, errors). Each [`Tick`](super::Action::Tick) ages it
//! and it disappears on its own, so the UI gives feedback without modal noise.

/// Severity/kind of a toast, mapped to a color by the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

/// A transient message shown briefly in the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
    /// Remaining lifetime in ticks. Decremented each [`Tick`](super::Action::Tick).
    pub ttl: u16,
}

impl Toast {
    /// Default lifetime in ticks. With a ~120 ms tick this is ~2.5 s.
    pub const DEFAULT_TTL: u16 = 20;

    pub fn new(message: impl Into<String>, kind: ToastKind) -> Self {
        Self {
            message: message.into(),
            kind,
            ttl: Self::DEFAULT_TTL,
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self::new(message, ToastKind::Info)
    }
    pub fn success(message: impl Into<String>) -> Self {
        Self::new(message, ToastKind::Success)
    }
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(message, ToastKind::Warning)
    }
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(message, ToastKind::Error)
    }

    /// Age the toast by one tick. Returns `true` while it is still alive.
    pub fn tick(&mut self) -> bool {
        self.ttl = self.ttl.saturating_sub(1);
        self.ttl > 0
    }
}
