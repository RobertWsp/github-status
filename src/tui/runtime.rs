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

use std::time::Duration;

use std::sync::Arc;

use color_eyre::Result;
use tokio::sync::mpsc;

use crate::app::{Action, AppState, Command, StatusService};
use crate::ports::{CacheStore, ProjectStore};
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
    store: Arc<dyn ProjectStore>,
    cache: Arc<dyn CacheStore>,
    theme: Theme,
    /// Tick counter; the scheduler evaluates due-times every Nth tick.
    ticks: u64,
}

impl Runtime {
    pub fn new(
        state: AppState,
        service: StatusService,
        store: Arc<dyn ProjectStore>,
        cache: Arc<dyn CacheStore>,
    ) -> Self {
        Self {
            state,
            service,
            store,
            cache,
            theme: Theme::default(),
            ticks: 0,
        }
    }

    /// Evaluate the polling schedule every this many ticks. With a 120 ms tick
    /// that's ~2 s — cheap, since the actual cadence is decided by per-project
    /// due-times in the pure reducer.
    const SCHEDULER_EVERY_TICKS: u64 = 16;

    /// Run until the user quits. Restores the terminal on the way out.
    pub async fn run(mut self) -> Result<()> {
        let mut guard = TerminalGuard::enter()?;
        let mut events = EventSource::new(Duration::from_millis(120));
        let (fetch_tx, mut fetch_rx) = mpsc::unbounded_channel::<Action>();

        // Hydrate from cache so the dashboard is populated instantly without an
        // API burst; the adaptive scheduler then only fetches stale repos.
        let cached = self.cache.load();
        if !cached.is_empty() {
            self.state.hydrate_from_cache(&cached);
        }
        self.draw(&mut guard)?;

        // Initial poll: only the projects that are due (idle/stale) get fetched.
        self.dispatch(Action::PollDue, &fetch_tx);
        self.draw(&mut guard)?;

        while self.state.running {
            tokio::select! {
                // Terminal + tick events.
                maybe_event = events.next() => {
                    let Some(event) = maybe_event else { break };
                    let dirty = self.handle_event(event, &fetch_tx);
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

    /// Map an [`Event`] to an [`Action`], apply it, and run the scheduler.
    /// Returns whether a redraw is warranted.
    fn handle_event(&mut self, event: Event, fetch_tx: &mpsc::UnboundedSender<Action>) -> bool {
        match event {
            Event::Key(key) => {
                let confirming = self.state.pending_delete.is_some();
                let action = keymap::map_key(key, self.state.mode, confirming);
                if matches!(action, Action::Noop) {
                    return false;
                }
                self.dispatch(action, fetch_tx);
                true
            }
            Event::Resize(_, _) => true,
            Event::Tick => {
                self.state.update(Action::Tick);
                self.ticks = self.ticks.wrapping_add(1);
                // Periodically ask the pure reducer which visible projects are
                // due; it decides cadence per adaptive tier (and respects any
                // rate-limit cooldown). Only fires when idle to avoid overlap.
                if self.ticks % Self::SCHEDULER_EVERY_TICKS == 0 && !self.state.is_loading() {
                    self.dispatch(Action::PollDue, fetch_tx);
                }
                // Redraw on tick while a spinner or toast is animating.
                self.state.needs_animation()
            }
        }
    }

    /// Apply an action to the state and interpret the resulting command.
    fn dispatch(&mut self, action: Action, fetch_tx: &mpsc::UnboundedSender<Action>) {
        match self.state.update(action) {
            Command::None => {}
            Command::Fetch(targets) => {
                // The reducer already marked these Loading & stamped attempts.
                self.service.refresh(targets, fetch_tx.clone());
            }
            Command::OpenUrl(url) => {
                // Best-effort; failure to open a browser must not crash the TUI.
                let _ = open_in_browser(&url);
            }
            Command::PersistProjects => self.persist_projects(),
            Command::PersistCache => self.persist_cache(),
            Command::Quit => {}
        }
    }

    /// Persist the current project list to the store. A failure surfaces as a
    /// toast but never crashes the session.
    fn persist_projects(&mut self) {
        let projects = self.state.project_list();
        if let Err(e) = self.store.save(&projects) {
            self.state.toast = Some(crate::app::Toast::error(format!(
                "Could not save config: {e}"
            )));
        }
    }

    /// Persist fetched runs to the cache. Best-effort: a failure is silent
    /// (the cache is an optimization, never correctness).
    fn persist_cache(&self) {
        let entries = self.state.cache_entries();
        let _ = self.cache.save(&entries);
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
