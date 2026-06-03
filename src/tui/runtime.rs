//! Runtime — the interactive event loop (the impure shell around pure state).
//!
//! Responsibilities:
//!   * own the terminal + event source + fetch-result channel,
//!   * translate [`Event`]s into [`Action`]s and feed the pure reducer,
//!   * interpret the [`Command`]s the reducer returns (refresh, open URL, quit),
//!   * drive auto-refresh on a timer,
//!   * redraw after each meaningful change.
//!
//! Everything domain/decision-related lives in [`AppState`]; this module is
//! deliberately thin and side-effect-only.

use std::time::{Duration, Instant};

use color_eyre::Result;
use tokio::sync::mpsc;

use crate::app::{Action, AppState, Command, StatusService};
use crate::tui::{
    event::{Event, EventSource},
    keymap,
    terminal::TerminalGuard,
    theme::Theme,
    view,
};

/// Owns and runs the TUI session.
pub struct Runtime {
    state: AppState,
    service: StatusService,
    theme: Theme,
    auto_refresh: Option<Duration>,
}

impl Runtime {
    pub fn new(state: AppState, service: StatusService, auto_refresh_secs: u64) -> Self {
        Self {
            state,
            service,
            theme: Theme::default(),
            auto_refresh: (auto_refresh_secs > 0).then(|| Duration::from_secs(auto_refresh_secs)),
        }
    }

    /// Run until the user quits. Restores the terminal on the way out.
    pub async fn run(mut self) -> Result<()> {
        let mut guard = TerminalGuard::enter()?;
        let mut events = EventSource::new(Duration::from_millis(120));
        let (fetch_tx, mut fetch_rx) = mpsc::unbounded_channel::<Action>();

        // Kick off an initial load.
        self.dispatch(Action::Refresh, &fetch_tx);
        self.draw(&mut guard)?;

        let mut last_refresh = Instant::now();

        while self.state.running {
            tokio::select! {
                // Terminal + tick events.
                maybe_event = events.next() => {
                    let Some(event) = maybe_event else { break };
                    let dirty = self.handle_event(event, &fetch_tx, &mut last_refresh);
                    if dirty {
                        self.draw(&mut guard)?;
                    }
                }
                // Background fetch results.
                maybe_action = fetch_rx.recv() => {
                    if let Some(action) = maybe_action {
                        self.dispatch(action, &fetch_tx);
                        self.draw(&mut guard)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Map an [`Event`] to an [`Action`], apply it, and handle timers. Returns
    /// whether a redraw is warranted.
    fn handle_event(
        &mut self,
        event: Event,
        fetch_tx: &mpsc::UnboundedSender<Action>,
        last_refresh: &mut Instant,
    ) -> bool {
        match event {
            Event::Key(key) => {
                let action = keymap::map_key(key, self.state.mode);
                if matches!(action, Action::Noop) {
                    return false;
                }
                self.dispatch(action, fetch_tx);
                true
            }
            Event::Resize(_, _) => true,
            Event::Tick => {
                self.state.update(Action::Tick);
                // Auto-refresh when due and idle.
                if let Some(interval) = self.auto_refresh {
                    if last_refresh.elapsed() >= interval && !self.state.is_loading() {
                        *last_refresh = Instant::now();
                        self.dispatch(Action::Refresh, fetch_tx);
                    }
                }
                // Redraw on tick while a spinner or toast is animating.
                self.state.needs_animation()
            }
        }
    }

    /// Apply an action to the state and interpret the resulting command.
    fn dispatch(&mut self, action: Action, fetch_tx: &mpsc::UnboundedSender<Action>) {
        let is_refresh = matches!(action, Action::Refresh);
        match self.state.update(action) {
            Command::None => {}
            Command::RefreshAll => {
                let projects: Vec<_> = self
                    .state
                    .projects
                    .iter()
                    .map(|p| p.project.clone())
                    .collect();
                self.service.refresh_all(&projects, fetch_tx.clone());
            }
            Command::OpenUrl(url) => {
                // Best-effort; failure to open a browser must not crash the TUI.
                let _ = open_in_browser(&url);
            }
            Command::Quit => {}
        }
        let _ = is_refresh;
    }

    fn draw(&mut self, guard: &mut TerminalGuard) -> Result<()> {
        guard
            .terminal()
            .draw(|frame| view::render(frame, &self.state, &self.theme))?;
        Ok(())
    }
}

/// Open a URL in the platform browser without pulling an extra dependency.
fn open_in_browser(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let (cmd, args) = ("open", vec![url]);
    #[cfg(target_os = "windows")]
    let (cmd, args) = ("cmd", vec!["/C", "start", "", url]);
    #[cfg(all(unix, not(target_os = "macos")))]
    let (cmd, args) = ("xdg-open", vec![url]);

    std::process::Command::new(cmd)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}
