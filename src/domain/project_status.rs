//! Aggregate: the fetched status of a project at a point in time.

use std::cmp::Ordering;

use chrono::{DateTime, Duration, Utc};

use super::poll::{PollIntervals, PollTier};
use super::{project::Project, run::WorkflowRun, status::RunState};

/// How recent a run must be for a project to count as "recently active" (and
/// thus polled more often even after the run finishes).
const RECENT_ACTIVITY_WINDOW_MINS: i64 = 30;

/// Loading lifecycle of a project's status — drives spinners and error UI.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LoadState {
    /// Never fetched yet.
    #[default]
    Idle,
    /// A fetch is in flight.
    Loading,
    /// Fetched successfully, carrying the latest runs (newest first).
    Loaded {
        runs: Vec<WorkflowRun>,
        fetched_at: DateTime<Utc>,
    },
    /// Fetch failed; carries a human-readable message.
    Failed { message: String },
}

/// A project paired with its current load state. This is the unit the UI lists.
///
/// Beyond the fetched [`LoadState`], it carries lightweight *runtime* polling
/// bookkeeping (`last_attempt_at`, `consecutive_errors`) that the scheduler
/// uses to decide when this project is next due. These are not persisted as
/// part of the user's config — they're transient session state.
#[derive(Debug, Clone)]
pub struct ProjectStatus {
    pub project: Project,
    pub load: LoadState,
    /// When a fetch was last *started* for this project (drives due-time).
    pub last_attempt_at: Option<DateTime<Utc>>,
    /// Number of consecutive failed fetches (drives exponential backoff).
    pub consecutive_errors: u32,
}

impl ProjectStatus {
    pub fn new(project: Project) -> Self {
        Self {
            project,
            load: LoadState::Idle,
            last_attempt_at: None,
            consecutive_errors: 0,
        }
    }

    /// Construct already-loaded with prior runs (used to hydrate from cache).
    pub fn from_cache(project: Project, runs: Vec<WorkflowRun>, fetched_at: DateTime<Utc>) -> Self {
        Self {
            project,
            load: LoadState::Loaded { runs, fetched_at },
            // Treat the cache fetch time as the last attempt so the scheduler
            // doesn't immediately re-poll everything on startup.
            last_attempt_at: Some(fetched_at),
            consecutive_errors: 0,
        }
    }

    /// The most recent run, if loaded.
    pub fn latest_run(&self) -> Option<&WorkflowRun> {
        match &self.load {
            LoadState::Loaded { runs, .. } => runs.first(),
            _ => None,
        }
    }

    /// The headline state for this project (state of its latest run).
    pub fn headline_state(&self) -> Option<RunState> {
        self.latest_run().map(|r| r.state)
    }

    /// Timestamp of the latest CI activity (the newest run's update time), if
    /// loaded. Used to order projects by most-recent CI/CD interaction.
    pub fn last_activity(&self) -> Option<DateTime<Utc>> {
        self.latest_run().map(|r| r.updated_at)
    }

    /// When this project's data was last successfully fetched, if ever.
    pub fn fetched_at(&self) -> Option<DateTime<Utc>> {
        match &self.load {
            LoadState::Loaded { fetched_at, .. } => Some(*fetched_at),
            _ => None,
        }
    }

    // --- Adaptive polling (SSoT for cadence lives in `domain::poll`) ---------

    /// Classify this project into a [`PollTier`] from its state + history.
    /// Pure: `now` is supplied so it stays deterministic and testable.
    pub fn poll_tier(&self, now: DateTime<Utc>) -> PollTier {
        // Errors dominate: anything failing to fetch backs off.
        if self.consecutive_errors > 0 {
            return PollTier::Backoff;
        }
        match &self.load {
            LoadState::Idle => PollTier::Idle,
            // A fetch in flight keeps the project hot so we re-check promptly.
            LoadState::Loading => PollTier::Active,
            LoadState::Failed { .. } => PollTier::Backoff,
            LoadState::Loaded { runs, .. } => {
                let Some(latest) = runs.first() else {
                    // Loaded but zero runs → the repo has no CI/CD: poll rarely.
                    return PollTier::Dormant;
                };
                if latest.state.is_active() {
                    return PollTier::Active;
                }
                let recent =
                    now - latest.updated_at <= Duration::minutes(RECENT_ACTIVITY_WINDOW_MINS);
                if recent || latest.state.is_failure() {
                    PollTier::Recent
                } else {
                    PollTier::Stable
                }
            }
        }
    }

    /// When this project next becomes due for a fetch, given the configured
    /// intervals. `None` means "due now" (never attempted, or `Idle`).
    pub fn due_at(&self, intervals: &PollIntervals, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let last = self.last_attempt_at?;
        let tier = self.poll_tier(now);
        let wait = intervals.interval_for(tier, self.consecutive_errors);
        Some(last + Duration::from_std(wait).unwrap_or(Duration::zero()))
    }

    /// Whether this project is due for a (re)fetch at `now`.
    pub fn is_due(&self, intervals: &PollIntervals, now: DateTime<Utc>) -> bool {
        // A fetch already in flight is never "due" again.
        if matches!(self.load, LoadState::Loading) {
            return false;
        }
        match self.due_at(intervals, now) {
            None => true, // never attempted → due immediately
            Some(at) => now >= at,
        }
    }

    /// Coarse attention bucket (lower = more urgent). This is the **primary**
    /// sort dimension: failures first, then things needing action, then active
    /// runs, then healthy/loaded, then errored/loading/idle.
    fn attention_bucket(&self) -> u8 {
        match &self.load {
            LoadState::Loaded { .. } => match self.headline_state() {
                Some(s) if s.is_failure() => 0,
                Some(RunState::ActionRequired) => 1,
                Some(s) if s.is_active() => 2,
                Some(_) => 3, // healthy/neutral/etc.
                None => 4,    // loaded but no runs
            },
            LoadState::Failed { .. } => 5, // fetch error (infra, not CI)
            LoadState::Loading => 6,
            LoadState::Idle => 7,
        }
    }

    /// Total ordering for the display list. Sorts by:
    ///   1. attention bucket (failures first),
    ///   2. most-recent CI activity (newest first),
    ///   3. owner then repo (stable, allocation-free tie-break).
    ///
    /// Allocation-free so it's cheap to run on every refresh, even with many
    /// hundreds of projects.
    pub fn cmp_display(&self, other: &Self) -> Ordering {
        self.attention_bucket()
            .cmp(&other.attention_bucket())
            // Newer activity first → reverse the timestamp comparison. Projects
            // without activity (None) sort last within their bucket.
            .then_with(|| cmp_activity_desc(self.last_activity(), other.last_activity()))
            .then_with(|| {
                self.project
                    .owner
                    .cmp(&other.project.owner)
                    .then_with(|| self.project.repo.cmp(&other.project.repo))
            })
    }
}

/// Compare two optional activity timestamps so that newer sorts first and
/// `None` (no activity) sorts last.
fn cmp_activity_desc(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => b.cmp(&a),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn loaded(owner: &str, repo: &str, state: RunState, ago_secs: i64) -> ProjectStatus {
        let updated = Utc::now() - Duration::seconds(ago_secs);
        let run = WorkflowRun {
            name: "CI".into(),
            run_number: 1,
            state,
            branch: "main".into(),
            short_sha: "abc1234".into(),
            event: "push".into(),
            created_at: updated,
            updated_at: updated,
            html_url: "https://example.com".into(),
        };
        ProjectStatus {
            project: Project::new(owner, repo),
            load: LoadState::Loaded {
                runs: vec![run],
                fetched_at: Utc::now(),
            },
            last_attempt_at: Some(Utc::now()),
            consecutive_errors: 0,
        }
    }

    fn sorted(mut v: Vec<ProjectStatus>) -> Vec<String> {
        v.sort_by(ProjectStatus::cmp_display);
        v.into_iter().map(|s| s.project.slug()).collect()
    }

    #[test]
    fn failures_come_before_success() {
        let order = sorted(vec![
            loaded("o", "ok", RunState::Success, 10),
            loaded("o", "bad", RunState::Failed, 9999),
        ]);
        assert_eq!(order, vec!["o/bad", "o/ok"]);
    }

    #[test]
    fn within_bucket_newer_activity_first() {
        let order = sorted(vec![
            loaded("o", "old", RunState::Success, 1000),
            loaded("o", "new", RunState::Success, 5),
        ]);
        assert_eq!(order, vec!["o/new", "o/old"]);
    }

    #[test]
    fn action_required_outranks_active_and_healthy() {
        let order = sorted(vec![
            loaded("o", "healthy", RunState::Success, 1),
            loaded("o", "running", RunState::InProgress, 1),
            loaded("o", "action", RunState::ActionRequired, 1),
        ]);
        assert_eq!(order, vec!["o/action", "o/running", "o/healthy"]);
    }

    #[test]
    fn idle_and_loading_sort_after_loaded() {
        let mut idle = ProjectStatus::new(Project::new("o", "idle"));
        idle.load = LoadState::Idle;
        let order = sorted(vec![idle, loaded("o", "done", RunState::Success, 1)]);
        assert_eq!(order, vec!["o/done", "o/idle"]);
    }

    // --- adaptive polling tiers ---------------------------------------------

    #[test]
    fn idle_project_is_due_and_idle_tier() {
        let s = ProjectStatus::new(Project::new("o", "r"));
        let now = Utc::now();
        assert_eq!(s.poll_tier(now), PollTier::Idle);
        assert!(s.is_due(&PollIntervals::default(), now));
    }

    #[test]
    fn in_progress_run_is_active_tier() {
        let s = loaded("o", "r", RunState::InProgress, 1);
        assert_eq!(s.poll_tier(Utc::now()), PollTier::Active);
    }

    #[test]
    fn loaded_without_runs_is_dormant() {
        let mut s = ProjectStatus::new(Project::new("o", "nocicd"));
        s.load = LoadState::Loaded {
            runs: vec![],
            fetched_at: Utc::now(),
        };
        s.last_attempt_at = Some(Utc::now());
        assert_eq!(s.poll_tier(Utc::now()), PollTier::Dormant);
    }

    #[test]
    fn old_green_run_is_stable_recent_failure_is_recent() {
        let stable = loaded("o", "green", RunState::Success, 60 * 60 /* 1h */);
        assert_eq!(stable.poll_tier(Utc::now()), PollTier::Stable);

        let failing = loaded("o", "red", RunState::Failed, 60 * 60);
        assert_eq!(failing.poll_tier(Utc::now()), PollTier::Recent);
    }

    #[test]
    fn errors_force_backoff_tier() {
        let mut s = loaded("o", "r", RunState::Success, 5);
        s.consecutive_errors = 2;
        assert_eq!(s.poll_tier(Utc::now()), PollTier::Backoff);
    }

    #[test]
    fn stable_project_not_due_until_interval_elapses() {
        let intervals = PollIntervals::default();
        let now = Utc::now();
        let mut s = loaded("o", "green", RunState::Success, 60 * 60);
        // Last attempted just now → not due (stable = 5 min).
        s.last_attempt_at = Some(now);
        assert!(!s.is_due(&intervals, now));
        // Six minutes later → due.
        assert!(s.is_due(&intervals, now + Duration::seconds(360)));
    }

    #[test]
    fn loading_project_is_never_due() {
        let mut s = ProjectStatus::new(Project::new("o", "r"));
        s.load = LoadState::Loading;
        assert!(!s.is_due(&PollIntervals::default(), Utc::now()));
    }
}
