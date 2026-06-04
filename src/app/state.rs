//! Application state — the Single Source of Truth for the UI.
//!
//! Holds the projects with their load states plus pure UI state (selection,
//! search query, owner filter, help overlay, toast). All mutations flow through
//! [`AppState::update`], which reduces an [`Action`] into a state change and
//! optionally yields a [`Command`] describing side effects for the runtime to
//! perform. The state never performs I/O — that keeps it deterministic and
//! unit-testable.
//!
//! ## Derived, not duplicated (SSoT)
//! `projects` is the only list of truth. The filtered/searched list the user
//! sees is *derived* on demand via [`AppState::visible_indices`] — never stored
//! as a second copy that could drift out of sync.
//!
//! ## Stable selection
//! `selected` is an index into the **visible** list and is intentionally *not*
//! made to follow a project across re-sorts. When a refresh reorders projects
//! (failures floating up), the cursor stays put at its position instead of
//! being dragged down with whatever row it started on.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use super::action::Action;
use super::filter::Filter;
use super::toast::{Toast, ToastKind};
use crate::domain::{LoadState, PollIntervals, Project, ProjectStatus};
use crate::ports::Clock;

/// Side effects the runtime should perform after an update.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Nothing to do.
    None,
    /// Fetch this specific set of `(physical index, project)` pairs. The
    /// reducer has already marked them `Loading` and recorded the attempt time;
    /// the runtime just hands them to the [`StatusService`](super::StatusService).
    Fetch(Vec<(usize, Project)>),
    /// Open this URL in the system browser.
    OpenUrl(String),
    /// Persist the current project list (after a delete) via the store.
    PersistProjects,
    /// Persist the current run data to the cache (after a poll batch settles).
    PersistCache,
    /// Tear down and exit.
    Quit,
}

/// A confirmation pending the user's yes/no — drives the delete modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingDelete {
    /// Remove a single repository (`owner/repo`).
    Repo { slug: String },
    /// Remove every repository of an owner/company; `count` is how many.
    Owner { owner: String, count: usize },
}

impl PendingDelete {
    /// Human-readable prompt body.
    pub fn prompt(&self) -> String {
        match self {
            Self::Repo { slug } => format!("Remove {slug} from your dashboard?"),
            Self::Owner { owner, count } => {
                format!("Remove ALL {count} repo(s) of \"{owner}\" from your dashboard?")
            }
        }
    }
}

/// Keyboard input mode — determines how keys are interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Normal navigation.
    Normal,
    /// Typing into the search box.
    Search,
}

/// The whole UI state.
pub struct AppState {
    /// Projects with their load state — the single source of truth (physical
    /// order; re-sorted by attention/recency when a refresh batch settles).
    pub projects: Vec<ProjectStatus>,
    /// Selection index into the **visible** (filtered) list.
    pub selected: usize,
    /// Current input mode.
    pub mode: InputMode,
    /// Active view filters (search query + owner). SSoT for visibility.
    pub filter: Filter,
    /// Whether the help overlay is visible.
    pub show_help: bool,
    /// Set false to break the event loop.
    pub running: bool,
    /// Number of in-flight fetches (drives the global "loading" indicator).
    pub inflight: usize,
    /// Animation frame counter for spinners.
    pub spinner_frame: usize,
    /// Optional transient notification.
    pub toast: Option<Toast>,
    /// A delete awaiting confirmation (modal). `None` means no prompt.
    pub pending_delete: Option<PendingDelete>,
    /// Adaptive polling intervals (from config).
    pub intervals: PollIntervals,
    /// While `Some(until)`, polling is paused due to a rate-limit response.
    pub rate_limited_until: Option<DateTime<Utc>>,
    /// Set when run data changed and the cache should be re-persisted.
    pub cache_dirty: bool,
    /// SSoT for "now" — injected so the reducer stays deterministic/testable.
    clock: Arc<dyn Clock>,
}

impl AppState {
    /// Build the application state. This is the canonical constructor: the
    /// composition root injects the [`Clock`] (system clock in production, a
    /// fixed clock in tests), so the reducer never reads the wall clock
    /// directly — keeping it deterministic and the time source single.
    pub fn with_config(
        projects: Vec<Project>,
        intervals: PollIntervals,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            projects: projects.into_iter().map(ProjectStatus::new).collect(),
            selected: 0,
            mode: InputMode::Normal,
            filter: Filter::default(),
            show_help: false,
            running: true,
            inflight: 0,
            spinner_frame: 0,
            toast: None,
            pending_delete: None,
            intervals,
            rate_limited_until: None,
            cache_dirty: false,
            clock,
        }
    }

    /// The current time, sampled from the injected clock (SSoT for "now").
    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    /// Hydrate already-loaded projects from cached snapshots (matched by slug).
    /// Unmatched projects stay `Idle`. Called once at startup.
    pub fn hydrate_from_cache(&mut self, cached: &[crate::ports::CachedProject]) {
        use std::collections::HashMap;
        let by_slug: HashMap<&str, &crate::ports::CachedProject> =
            cached.iter().map(|c| (c.slug.as_str(), c)).collect();
        for status in &mut self.projects {
            if let Some(c) = by_slug.get(status.project.slug().as_str()) {
                *status =
                    ProjectStatus::from_cache(status.project.clone(), c.runs.clone(), c.fetched_at);
            }
        }
        self.projects.sort_by(ProjectStatus::cmp_display);
        self.clamp_selection();
    }

    /// Snapshot the loaded projects as cache entries to persist.
    pub fn cache_entries(&self) -> Vec<crate::ports::CachedProject> {
        self.projects
            .iter()
            .filter_map(|s| match &s.load {
                LoadState::Loaded { runs, fetched_at } => Some(crate::ports::CachedProject {
                    slug: s.project.slug(),
                    runs: runs.clone(),
                    fetched_at: *fetched_at,
                }),
                _ => None,
            })
            .collect()
    }

    // --- Derived views (computed, never stored) ------------------------------

    /// Iterator over the physical indices of projects passing the active
    /// filters, in display order. Allocation-free — callers that need a `Vec`
    /// can `.collect()`, but counting/nth/iteration stay zero-alloc.
    pub fn visible(&self) -> impl Iterator<Item = usize> + '_ {
        self.projects
            .iter()
            .enumerate()
            .filter(|(_, s)| self.filter.passes(&s.project))
            .map(|(i, _)| i)
    }

    /// Physical indices of visible projects as a `Vec` (when one is needed).
    pub fn visible_indices(&self) -> Vec<usize> {
        self.visible().collect()
    }

    /// Number of currently visible projects (no allocation).
    pub fn visible_count(&self) -> usize {
        self.visible().count()
    }

    /// Physical index of the `n`th visible project (no allocation).
    pub fn nth_visible(&self, n: usize) -> Option<usize> {
        self.visible().nth(n)
    }

    /// Whether any filter (search query or owner) is active.
    pub fn has_active_filter(&self) -> bool {
        self.filter.is_active()
    }

    /// The currently selected project status (resolving the visible index).
    pub fn selected_status(&self) -> Option<&ProjectStatus> {
        self.nth_visible(self.selected).map(|i| &self.projects[i])
    }

    /// Whether any background fetch is currently running.
    pub fn is_loading(&self) -> bool {
        self.inflight > 0
    }

    /// Whether the UI needs continuous repaint (spinner or live toast).
    pub fn needs_animation(&self) -> bool {
        self.is_loading() || self.toast.is_some()
    }

    // --- Reducer -------------------------------------------------------------

    /// Reduce an action into a state mutation, returning any side effect.
    pub fn update(&mut self, action: Action) -> Command {
        match action {
            Action::Up => {
                self.move_selection(-1);
                Command::None
            }
            Action::Down => {
                self.move_selection(1);
                Command::None
            }
            Action::Top => {
                self.selected = 0;
                Command::None
            }
            Action::Bottom => {
                self.selected = self.visible_count().saturating_sub(1);
                Command::None
            }
            Action::ToggleHelp => {
                self.show_help = !self.show_help;
                Command::None
            }
            Action::Refresh => self.force_refresh_visible(),
            Action::PollDue => self.poll_due(),
            Action::OpenInBrowser => self.open_selected(),
            Action::EnterSearch => {
                self.mode = InputMode::Search;
                Command::None
            }
            Action::SearchInput(c) => {
                if self.mode == InputMode::Search {
                    self.filter.query.push(c);
                    self.clamp_selection();
                }
                Command::None
            }
            Action::SearchBackspace => {
                if self.mode == InputMode::Search {
                    self.filter.query.pop();
                    self.clamp_selection();
                }
                Command::None
            }
            Action::ConfirmSearch => {
                self.mode = InputMode::Normal;
                match self.filter.query_text() {
                    None => self.filter.query.clear(),
                    Some(q) => {
                        let msg =
                            format!("Filtering by \"{q}\" — {} match(es)", self.visible_count());
                        self.toast = Some(Toast::info(msg));
                    }
                }
                self.clamp_selection();
                Command::None
            }
            Action::FilterSelectedOwner => self.filter_selected_owner(),
            Action::ClearFilters => {
                let had = self.has_active_filter();
                self.filter.clear();
                self.mode = InputMode::Normal;
                self.clamp_selection();
                if had {
                    self.toast = Some(Toast::info("Filters cleared"));
                }
                Command::None
            }
            Action::RequestDeleteRepo => {
                self.request_delete_repo();
                Command::None
            }
            Action::RequestDeleteOwner => {
                self.request_delete_owner();
                Command::None
            }
            Action::ConfirmDelete => self.confirm_delete(),
            Action::CancelDelete => {
                self.pending_delete = None;
                Command::None
            }
            Action::Escape => self.handle_escape(),
            Action::Quit => {
                self.running = false;
                Command::Quit
            }
            Action::Tick => {
                self.spinner_frame = self.spinner_frame.wrapping_add(1);
                if let Some(t) = &mut self.toast {
                    if !t.tick() {
                        self.toast = None;
                    }
                }
                Command::None
            }
            Action::FetchCompleted { index, result } => self.apply_fetch_result(index, result),
            Action::Noop => Command::None,
        }
    }

    /// Whether polling is currently paused due to a rate-limit cooldown.
    pub fn is_rate_limited(&self, now: DateTime<Utc>) -> bool {
        self.rate_limited_until.is_some_and(|until| now < until)
    }

    /// Manual full refresh of every **visible** project (the `r` key). Ignores
    /// due-times so the user always gets fresh data on demand, but still
    /// respects an active rate-limit cooldown.
    pub fn force_refresh_visible(&mut self) -> Command {
        let now = self.now();
        if self.is_rate_limited(now) {
            self.toast = Some(Toast::warning("Rate limited — refresh paused briefly"));
            return Command::None;
        }
        // Skip anything already in flight so a manual refresh during a poll
        // doesn't double-fetch (wasting API budget / corrupting `inflight`).
        let targets: Vec<usize> = self
            .visible()
            .filter(|&i| !matches!(self.projects[i].load, LoadState::Loading))
            .collect();
        if targets.is_empty() {
            let msg = if self.is_loading() {
                "Already refreshing…"
            } else {
                "Nothing visible to refresh"
            };
            self.toast = Some(Toast::warning(msg));
            return Command::None;
        }
        self.toast = Some(Toast::info(format!(
            "Refreshing {} project(s)…",
            targets.len()
        )));
        self.start_fetch(targets, now)
    }

    /// Scheduler-driven poll: fetch only visible projects that are *due* by
    /// their adaptive tier. Silent (no toast) since it runs on a timer.
    /// `is_due` already excludes in-flight projects.
    pub fn poll_due(&mut self) -> Command {
        let now = self.now();
        if self.is_rate_limited(now) {
            return Command::None;
        }
        let targets: Vec<usize> = self
            .visible()
            .filter(|&i| self.projects[i].is_due(&self.intervals, now))
            .collect();
        if targets.is_empty() {
            return Command::None;
        }
        self.start_fetch(targets, now)
    }

    /// Mark the given physical indices as loading, stamp the attempt time, and
    /// emit a [`Command::Fetch`] carrying the `(index, project)` pairs.
    ///
    /// Defensive: skips any index already `Loading` so `inflight` can't be
    /// inflated by overlapping callers (the invariant the spinner relies on).
    fn start_fetch(&mut self, indices: Vec<usize>, now: DateTime<Utc>) -> Command {
        let mut targets = Vec::with_capacity(indices.len());
        for i in indices {
            if let Some(s) = self.projects.get_mut(i) {
                if matches!(s.load, LoadState::Loading) {
                    continue;
                }
                s.load = LoadState::Loading;
                s.last_attempt_at = Some(now);
                targets.push((i, s.project.clone()));
            }
        }
        self.inflight += targets.len();
        Command::Fetch(targets)
    }

    fn open_selected(&mut self) -> Command {
        match self.selected_status().and_then(|s| s.latest_run()) {
            Some(run) => {
                let url = run.html_url.clone();
                self.toast = Some(Toast::success("Opening run in browser…"));
                Command::OpenUrl(url)
            }
            None => {
                self.toast = Some(Toast::warning("No run to open for this project"));
                Command::None
            }
        }
    }

    fn filter_selected_owner(&mut self) -> Command {
        if let Some(status) = self.selected_status() {
            let owner = status.project.owner.clone();
            self.filter.owner = Some(owner.clone());
            self.selected = 0;
            self.clamp_selection();
            self.toast = Some(Toast::new(
                format!("Showing only {owner}/* ({} repos)", self.visible_count()),
                ToastKind::Info,
            ));
        }
        Command::None
    }

    /// Stage a confirm prompt to remove the selected repository.
    fn request_delete_repo(&mut self) {
        if let Some(status) = self.selected_status() {
            self.pending_delete = Some(PendingDelete::Repo {
                slug: status.project.slug(),
            });
        }
    }

    /// Stage a confirm prompt to remove the selected owner's whole company.
    fn request_delete_owner(&mut self) {
        if let Some(status) = self.selected_status() {
            let owner = status.project.owner.clone();
            let count = self
                .projects
                .iter()
                .filter(|s| s.project.belongs_to(&owner))
                .count();
            self.pending_delete = Some(PendingDelete::Owner { owner, count });
        }
    }

    /// Apply the pending delete to the in-memory project list and request that
    /// the runtime persist it. The reducer never writes to disk itself.
    fn confirm_delete(&mut self) -> Command {
        let Some(pending) = self.pending_delete.take() else {
            return Command::None;
        };
        let (removed, summary): (usize, String) = match &pending {
            PendingDelete::Repo { slug } => {
                let before = self.projects.len();
                self.projects.retain(|s| s.project.slug() != *slug);
                (before - self.projects.len(), format!("Removed {slug}"))
            }
            PendingDelete::Owner { owner, .. } => {
                let before = self.projects.len();
                self.projects.retain(|s| !s.project.belongs_to(owner));
                let n = before - self.projects.len();
                (n, format!("Removed {n} repo(s) of \"{owner}\""))
            }
        };

        if removed == 0 {
            self.toast = Some(Toast::warning("Nothing was removed"));
            return Command::None;
        }

        // Owner filter may now match nothing; drop it to avoid an empty list.
        if let Some(owner) = &self.filter.owner {
            if !self.projects.iter().any(|s| s.project.belongs_to(owner)) {
                self.filter.owner = None;
            }
        }
        self.clamp_selection();
        self.toast = Some(Toast::success(summary));
        Command::PersistProjects
    }

    /// Snapshot of the current projects as plain domain values — used by the
    /// runtime to persist via the [`ProjectStore`](crate::ports::ProjectStore).
    pub fn project_list(&self) -> Vec<Project> {
        self.projects.iter().map(|s| s.project.clone()).collect()
    }

    /// Context-sensitive escape, mirroring familiar TUI behavior:
    /// pending delete → cancel; search mode → leave search; else active
    /// filter → clear it; else quit.
    fn handle_escape(&mut self) -> Command {
        if self.pending_delete.is_some() {
            self.pending_delete = None;
            Command::None
        } else if self.mode == InputMode::Search {
            self.mode = InputMode::Normal;
            self.clamp_selection();
            Command::None
        } else if self.has_active_filter() {
            self.update(Action::ClearFilters)
        } else {
            self.running = false;
            Command::Quit
        }
    }

    fn apply_fetch_result(
        &mut self,
        index: usize,
        result: Result<Vec<crate::domain::WorkflowRun>, crate::ports::ProviderError>,
    ) -> Command {
        let now = self.now();
        let mut rate_limited = false;
        if let Some(status) = self.projects.get_mut(index) {
            match result {
                Ok(runs) => {
                    status.consecutive_errors = 0;
                    status.load = LoadState::Loaded {
                        runs,
                        fetched_at: now,
                    };
                    self.cache_dirty = true;
                }
                Err(e) => {
                    status.consecutive_errors = status.consecutive_errors.saturating_add(1);
                    rate_limited = matches!(e, crate::ports::ProviderError::RateLimited(_));
                    status.load = LoadState::Failed {
                        message: e.to_string(),
                    };
                }
            }
        }
        self.inflight = self.inflight.saturating_sub(1);

        // A rate-limit response pauses *all* polling for a cooldown window so we
        // back off globally instead of hammering the API.
        if rate_limited {
            let cooldown = chrono::Duration::from_std(self.intervals.rate_limit_cooldown)
                .unwrap_or_else(|_| chrono::Duration::seconds(60));
            self.rate_limited_until = Some(now + cooldown);
            self.toast = Some(Toast::warning(format!(
                "GitHub rate limit hit — pausing polling {}s",
                cooldown.num_seconds()
            )));
        }

        // Re-sort once the whole batch settles so indices stay stable during
        // the batch (FetchCompleted carries physical indices).
        if self.inflight == 0 {
            self.projects.sort_by(ProjectStatus::cmp_display);
            self.clamp_selection();
            if !rate_limited {
                let failures = self
                    .projects
                    .iter()
                    .filter(|s| s.headline_state().is_some_and(|st| st.is_failure()))
                    .count();
                self.toast = Some(if failures > 0 {
                    Toast::error(format!("{failures} project(s) failing"))
                } else {
                    Toast::success("All projects healthy")
                });
            }
            // Persist the freshly-updated runs to the cache, if anything changed.
            if self.cache_dirty {
                self.cache_dirty = false;
                return Command::PersistCache;
            }
        }
        Command::None
    }

    /// Move selection within the visible list, wrapping around.
    fn move_selection(&mut self, delta: isize) {
        let len = self.visible_count();
        if len == 0 {
            self.selected = 0;
            return;
        }
        let next = (self.selected as isize + delta).rem_euclid(len as isize);
        self.selected = next as usize;
    }

    /// Keep `selected` within the visible list bounds after a filter change.
    fn clamp_selection(&mut self) {
        let len = self.visible_count();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::FixedClock;
    use crate::domain::{RunState, WorkflowRun};
    use chrono::Utc;

    /// Build state with a fixed clock so tests are deterministic.
    fn app(projects: Vec<Project>) -> AppState {
        AppState::with_config(
            projects,
            PollIntervals::default(),
            Arc::new(FixedClock::new(Utc::now())),
        )
    }

    fn project(owner: &str, repo: &str) -> Project {
        Project::new(owner, repo)
    }

    fn run(state: RunState) -> WorkflowRun {
        WorkflowRun {
            name: "CI".into(),
            run_number: 1,
            state,
            branch: "main".into(),
            short_sha: "abc1234".into(),
            event: "push".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            html_url: "https://example.com".into(),
        }
    }

    #[test]
    fn navigation_wraps_over_visible() {
        let mut s = app(vec![project("a", "a"), project("b", "b")]);
        assert_eq!(s.selected, 0);
        s.update(Action::Up);
        assert_eq!(s.selected, 1);
        s.update(Action::Down);
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn selection_is_stable_at_top_across_resort() {
        // Cursor at the top must NOT follow its project when the list reorders.
        let mut s = app(vec![project("o", "ok"), project("o", "bad")]);
        assert_eq!(s.selected, 0);
        s.update(Action::Refresh);
        s.update(Action::FetchCompleted {
            index: 0,
            result: Ok(vec![run(RunState::Success)]),
        });
        s.update(Action::FetchCompleted {
            index: 1,
            result: Ok(vec![run(RunState::Failed)]),
        });
        // After the batch, failures sort to the top; the cursor stays at row 0.
        assert_eq!(s.selected, 0);
        // And row 0 is now the failing project (sorted), not dragged down.
        assert_eq!(s.selected_status().unwrap().project.repo, "bad");
    }

    #[test]
    fn refresh_marks_all_loading_and_counts_inflight() {
        let mut s = app(vec![project("a", "a"), project("b", "b")]);
        let cmd = s.update(Action::Refresh);
        match cmd {
            Command::Fetch(targets) => assert_eq!(targets.len(), 2),
            other => panic!("expected Fetch, got {other:?}"),
        }
        assert_eq!(s.inflight, 2);
        assert!(s.is_loading());
    }

    #[test]
    fn poll_due_only_fetches_due_projects() {
        let mut s = app(vec![project("a", "a"), project("b", "b")]);
        // Both idle → both due immediately.
        match s.poll_due() {
            Command::Fetch(t) => assert_eq!(t.len(), 2),
            other => panic!("expected Fetch, got {other:?}"),
        }
        // Now both are Loading → nothing due.
        assert_eq!(s.poll_due(), Command::None);
    }

    #[test]
    fn manual_refresh_during_poll_does_not_double_fetch() {
        // Regression: pressing `r` while a poll is in flight must not re-fetch
        // the same projects (would waste API budget and inflate `inflight`).
        let mut s = app(vec![project("a", "a"), project("b", "b")]);
        let cmd = s.poll_due(); // both now Loading, inflight = 2
        assert!(matches!(cmd, Command::Fetch(_)));
        assert_eq!(s.inflight, 2);
        // Manual refresh finds nothing not-already-loading.
        let cmd = s.force_refresh_visible();
        assert_eq!(cmd, Command::None);
        assert_eq!(s.inflight, 2); // unchanged — no double fetch
    }

    #[test]
    fn rate_limit_pauses_polling() {
        let mut s = app(vec![project("a", "a")]);
        s.update(Action::Refresh);
        s.update(Action::FetchCompleted {
            index: 0,
            result: Err(crate::ports::ProviderError::RateLimited("slow down".into())),
        });
        assert!(s.is_rate_limited(s.now()));
        // While rate limited, polling yields nothing.
        assert_eq!(s.poll_due(), Command::None);
    }

    #[test]
    fn rate_limit_cooldown_expires_after_configured_window() {
        // Deterministic time: a FixedClock we can advance proves the cooldown
        // ends exactly when configured — impossible to test with Utc::now().
        let clock = FixedClock::new(Utc::now());
        let intervals = PollIntervals::default();
        let cooldown = intervals.rate_limit_cooldown;
        let mut s =
            AppState::with_config(vec![project("a", "a")], intervals, Arc::new(clock.clone()));
        s.update(Action::Refresh);
        s.update(Action::FetchCompleted {
            index: 0,
            result: Err(crate::ports::ProviderError::RateLimited("slow".into())),
        });
        assert!(s.is_rate_limited(s.now()));
        // Just before the window ends: still paused.
        clock.advance(chrono::Duration::from_std(cooldown).unwrap() - chrono::Duration::seconds(1));
        assert!(s.is_rate_limited(s.now()));
        // After the window: polling resumes.
        clock.advance(chrono::Duration::seconds(2));
        assert!(!s.is_rate_limited(s.now()));
    }

    #[test]
    fn failed_fetch_increments_backoff_counter() {
        let mut s = app(vec![project("a", "a")]);
        s.update(Action::Refresh);
        s.update(Action::FetchCompleted {
            index: 0,
            result: Err(crate::ports::ProviderError::Other("boom".into())),
        });
        assert_eq!(s.projects[0].consecutive_errors, 1);
    }

    #[test]
    fn successful_fetch_marks_cache_dirty_and_persists() {
        let mut s = app(vec![project("a", "a")]);
        s.update(Action::Refresh);
        let cmd = s.update(Action::FetchCompleted {
            index: 0,
            result: Ok(vec![run(RunState::Success)]),
        });
        // Batch settled with new data → ask runtime to persist the cache.
        assert_eq!(cmd, Command::PersistCache);
    }

    #[test]
    fn delete_repo_needs_confirmation_then_persists() {
        let mut s = app(vec![project("acme", "api"), project("acme", "web")]);
        // Request opens a prompt but changes nothing yet.
        s.update(Action::RequestDeleteRepo);
        assert!(s.pending_delete.is_some());
        assert_eq!(s.projects.len(), 2);
        // Confirm removes it and asks the runtime to persist.
        let cmd = s.update(Action::ConfirmDelete);
        assert_eq!(cmd, Command::PersistProjects);
        assert_eq!(s.projects.len(), 1);
        assert!(s.pending_delete.is_none());
    }

    #[test]
    fn cancel_delete_keeps_everything() {
        let mut s = app(vec![project("acme", "api")]);
        s.update(Action::RequestDeleteRepo);
        let cmd = s.update(Action::CancelDelete);
        assert_eq!(cmd, Command::None);
        assert_eq!(s.projects.len(), 1);
        assert!(s.pending_delete.is_none());
    }

    #[test]
    fn delete_owner_removes_whole_company() {
        let mut s = app(vec![
            project("acme", "api"),
            project("acme", "web"),
            project("other", "x"),
        ]);
        s.update(Action::RequestDeleteOwner);
        match &s.pending_delete {
            Some(PendingDelete::Owner { owner, count }) => {
                assert_eq!(owner, "acme");
                assert_eq!(*count, 2);
            }
            _ => panic!("expected owner delete"),
        }
        let cmd = s.update(Action::ConfirmDelete);
        assert_eq!(cmd, Command::PersistProjects);
        assert_eq!(s.projects.len(), 1);
        assert_eq!(s.projects[0].project.owner, "other");
    }

    #[test]
    fn escape_cancels_pending_delete_first() {
        let mut s = app(vec![project("acme", "api")]);
        s.update(Action::RequestDeleteRepo);
        let cmd = s.update(Action::Escape);
        assert_eq!(cmd, Command::None);
        assert!(s.pending_delete.is_none());
        assert!(s.running);
    }

    #[test]
    fn search_filters_visible_list() {
        let mut s = app(vec![project("acme", "api"), project("other", "web")]);
        s.update(Action::EnterSearch);
        for c in "acme".chars() {
            s.update(Action::SearchInput(c));
        }
        assert_eq!(s.visible_count(), 1);
        assert_eq!(s.selected_status().unwrap().project.owner, "acme");
        s.update(Action::SearchBackspace); // "acm" still matches acme
        assert_eq!(s.visible_count(), 1);
    }

    #[test]
    fn owner_filter_limits_to_company() {
        let mut s = app(vec![
            project("acme", "api"),
            project("acme", "web"),
            project("other", "x"),
        ]);
        // Select the "other" project then filter to acme via a different one.
        s.filter.owner = Some("acme".into());
        s.clamp_selection();
        assert_eq!(s.visible_count(), 2);
    }

    #[test]
    fn escape_clears_filter_before_quitting() {
        let mut s = app(vec![project("acme", "api")]);
        s.filter.owner = Some("acme".into());
        let cmd = s.update(Action::Escape);
        assert_eq!(cmd, Command::None); // cleared filter, did not quit
        assert!(s.filter.owner.is_none());
        assert!(s.running);
        let cmd = s.update(Action::Escape); // now quits
        assert_eq!(cmd, Command::Quit);
    }

    #[test]
    fn open_in_browser_yields_url_for_loaded_project() {
        let mut s = app(vec![project("a", "a")]);
        s.update(Action::Refresh);
        s.update(Action::FetchCompleted {
            index: 0,
            result: Ok(vec![run(RunState::Success)]),
        });
        let cmd = s.update(Action::OpenInBrowser);
        assert_eq!(cmd, Command::OpenUrl("https://example.com".into()));
    }

    #[test]
    fn toast_decays_on_tick() {
        let mut s = app(vec![project("a", "a")]);
        s.toast = Some(Toast::info("hi"));
        for _ in 0..Toast::DEFAULT_TTL {
            s.update(Action::Tick);
        }
        assert!(s.toast.is_none());
    }

    #[test]
    fn quit_stops_running() {
        let mut s = app(vec![]);
        let cmd = s.update(Action::Quit);
        assert_eq!(cmd, Command::Quit);
        assert!(!s.running);
    }
}
