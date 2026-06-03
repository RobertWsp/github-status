//! Aggregate: the fetched status of a project at a point in time.

use std::cmp::Ordering;

use chrono::{DateTime, Utc};

use super::{project::Project, run::WorkflowRun, status::RunState};

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
#[derive(Debug, Clone)]
pub struct ProjectStatus {
    pub project: Project,
    pub load: LoadState,
}

impl ProjectStatus {
    pub fn new(project: Project) -> Self {
        Self {
            project,
            load: LoadState::Idle,
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
}
