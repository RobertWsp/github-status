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

use super::action::Action;
use super::toast::{Toast, ToastKind};
use crate::domain::{LoadState, Project, ProjectStatus};

/// Side effects the runtime should perform after an update.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Nothing to do.
    None,
    /// Begin fetching all projects.
    RefreshAll,
    /// Open this URL in the system browser.
    OpenUrl(String),
    /// Tear down and exit.
    Quit,
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
    /// Active free-text search query (applies in both modes once non-empty).
    pub query: String,
    /// Active "show only this owner/company" filter.
    pub owner_filter: Option<String>,
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
}

impl AppState {
    /// Build initial state from the configured projects.
    pub fn new(projects: Vec<Project>) -> Self {
        Self {
            projects: projects.into_iter().map(ProjectStatus::new).collect(),
            selected: 0,
            mode: InputMode::Normal,
            query: String::new(),
            owner_filter: None,
            show_help: false,
            running: true,
            inflight: 0,
            spinner_frame: 0,
            toast: None,
        }
    }

    // --- Derived views (computed, never stored) ------------------------------

    /// Physical indices of projects passing the active filters, in display
    /// order. This is the projection the UI iterates over.
    pub fn visible_indices(&self) -> Vec<usize> {
        self.projects
            .iter()
            .enumerate()
            .filter(|(_, s)| self.passes_filters(&s.project))
            .map(|(i, _)| i)
            .collect()
    }

    /// Number of currently visible projects.
    pub fn visible_count(&self) -> usize {
        self.projects
            .iter()
            .filter(|s| self.passes_filters(&s.project))
            .count()
    }

    /// Whether any filter (search query or owner) is active.
    pub fn has_active_filter(&self) -> bool {
        self.owner_filter.is_some() || !self.query.trim().is_empty()
    }

    /// The currently selected project status (resolving the visible index).
    pub fn selected_status(&self) -> Option<&ProjectStatus> {
        let visible = self.visible_indices();
        visible.get(self.selected).map(|&i| &self.projects[i])
    }

    /// Whether any background fetch is currently running.
    pub fn is_loading(&self) -> bool {
        self.inflight > 0
    }

    /// Whether the UI needs continuous repaint (spinner or live toast).
    pub fn needs_animation(&self) -> bool {
        self.is_loading() || self.toast.is_some()
    }

    fn passes_filters(&self, project: &Project) -> bool {
        let owner_ok = self
            .owner_filter
            .as_deref()
            .is_none_or(|o| project.belongs_to(o));
        owner_ok && project.matches_query(&self.query)
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
            Action::Refresh => self.begin_refresh(),
            Action::OpenInBrowser => self.open_selected(),
            Action::EnterSearch => {
                self.mode = InputMode::Search;
                Command::None
            }
            Action::SearchInput(c) => {
                if self.mode == InputMode::Search {
                    self.query.push(c);
                    self.clamp_selection();
                }
                Command::None
            }
            Action::SearchBackspace => {
                if self.mode == InputMode::Search {
                    self.query.pop();
                    self.clamp_selection();
                }
                Command::None
            }
            Action::ConfirmSearch => {
                self.mode = InputMode::Normal;
                if self.query.trim().is_empty() {
                    self.query.clear();
                } else {
                    self.toast = Some(Toast::info(format!(
                        "Filtering by \"{}\" — {} match(es)",
                        self.query.trim(),
                        self.visible_count()
                    )));
                }
                self.clamp_selection();
                Command::None
            }
            Action::FilterSelectedOwner => self.filter_selected_owner(),
            Action::ClearFilters => {
                let had = self.has_active_filter();
                self.query.clear();
                self.owner_filter = None;
                self.mode = InputMode::Normal;
                self.clamp_selection();
                if had {
                    self.toast = Some(Toast::info("Filters cleared"));
                }
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
            Action::FetchCompleted { index, result } => {
                self.apply_fetch_result(index, result);
                Command::None
            }
            Action::Noop => Command::None,
        }
    }

    /// Mark every project as loading and request a full refresh.
    pub fn begin_refresh(&mut self) -> Command {
        if self.projects.is_empty() {
            self.toast = Some(Toast::warning("No projects to refresh"));
            return Command::None;
        }
        for p in &mut self.projects {
            p.load = LoadState::Loading;
        }
        self.inflight = self.projects.len();
        self.toast = Some(Toast::info(format!(
            "Refreshing {} project(s)…",
            self.inflight
        )));
        Command::RefreshAll
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
            self.owner_filter = Some(owner.clone());
            self.selected = 0;
            self.clamp_selection();
            self.toast = Some(Toast::new(
                format!("Showing only {owner}/* ({} repos)", self.visible_count()),
                ToastKind::Info,
            ));
        }
        Command::None
    }

    /// Context-sensitive escape, mirroring familiar TUI behavior:
    /// search mode → leave search; else active filter → clear it; else quit.
    fn handle_escape(&mut self) -> Command {
        if self.mode == InputMode::Search {
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
    ) {
        if let Some(status) = self.projects.get_mut(index) {
            status.load = match result {
                Ok(runs) => LoadState::Loaded {
                    runs,
                    fetched_at: chrono::Utc::now(),
                },
                Err(e) => LoadState::Failed {
                    message: e.to_string(),
                },
            };
        }
        self.inflight = self.inflight.saturating_sub(1);
        // Re-sort once the whole batch settles so indices stay stable during
        // the batch (FetchCompleted carries physical indices).
        if self.inflight == 0 {
            self.projects.sort_by(ProjectStatus::cmp_display);
            self.clamp_selection();
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
    use crate::domain::{RunState, WorkflowRun};
    use chrono::Utc;

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
        let mut s = AppState::new(vec![project("a", "a"), project("b", "b")]);
        assert_eq!(s.selected, 0);
        s.update(Action::Up);
        assert_eq!(s.selected, 1);
        s.update(Action::Down);
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn selection_is_stable_at_top_across_resort() {
        // Cursor at the top must NOT follow its project when the list reorders.
        let mut s = AppState::new(vec![project("o", "ok"), project("o", "bad")]);
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
        let mut s = AppState::new(vec![project("a", "a"), project("b", "b")]);
        let cmd = s.update(Action::Refresh);
        assert_eq!(cmd, Command::RefreshAll);
        assert_eq!(s.inflight, 2);
        assert!(s.is_loading());
    }

    #[test]
    fn search_filters_visible_list() {
        let mut s = AppState::new(vec![project("acme", "api"), project("other", "web")]);
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
        let mut s = AppState::new(vec![
            project("acme", "api"),
            project("acme", "web"),
            project("other", "x"),
        ]);
        // Select the "other" project then filter to acme via a different one.
        s.owner_filter = Some("acme".into());
        s.clamp_selection();
        assert_eq!(s.visible_count(), 2);
    }

    #[test]
    fn escape_clears_filter_before_quitting() {
        let mut s = AppState::new(vec![project("acme", "api")]);
        s.owner_filter = Some("acme".into());
        let cmd = s.update(Action::Escape);
        assert_eq!(cmd, Command::None); // cleared filter, did not quit
        assert!(s.owner_filter.is_none());
        assert!(s.running);
        let cmd = s.update(Action::Escape); // now quits
        assert_eq!(cmd, Command::Quit);
    }

    #[test]
    fn open_in_browser_yields_url_for_loaded_project() {
        let mut s = AppState::new(vec![project("a", "a")]);
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
        let mut s = AppState::new(vec![project("a", "a")]);
        s.toast = Some(Toast::info("hi"));
        for _ in 0..Toast::DEFAULT_TTL {
            s.update(Action::Tick);
        }
        assert!(s.toast.is_none());
    }

    #[test]
    fn quit_stops_running() {
        let mut s = AppState::new(vec![]);
        let cmd = s.update(Action::Quit);
        assert_eq!(cmd, Command::Quit);
        assert!(!s.running);
    }
}
