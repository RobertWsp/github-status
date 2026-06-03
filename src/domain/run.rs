//! Workflow run — the domain's view of a single GitHub Actions execution.
//!
//! This is deliberately decoupled from `octocrab::models::workflows::Run`. The
//! adapter in `adapters/github` maps the rich API type down to this lean,
//! presentation-relevant shape (anti-corruption layer), so the rest of the app
//! never depends on octocrab's data model.

use chrono::{DateTime, Utc};

use super::status::RunState;

/// A single workflow run, normalized for the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowRun {
    /// Workflow display name (e.g. "CI").
    pub name: String,
    /// Monotonic run number within the workflow.
    pub run_number: i64,
    /// Derived semantic state — SSoT for status.
    pub state: RunState,
    /// Branch the run executed against.
    pub branch: String,
    /// Short commit SHA (first 7 chars).
    pub short_sha: String,
    /// Event that triggered the run (push, pull_request, ...).
    pub event: String,
    /// When the run was created.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
    /// Web URL to open in a browser.
    pub html_url: String,
}

impl WorkflowRun {
    /// Wall-clock duration between creation and last update.
    pub fn duration(&self) -> chrono::Duration {
        self.updated_at - self.created_at
    }
}
