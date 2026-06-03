//! Single Source of Truth for GitHub Actions run status semantics.
//!
//! The GitHub REST API encodes the outcome of a workflow run across two
//! orthogonal string fields: `status` (lifecycle) and `conclusion` (result,
//! only meaningful once `status == "completed"`). Leaving those raw strings to
//! leak through the codebase would scatter status logic everywhere. Instead we
//! collapse them into a single, exhaustive [`RunState`] enum here — every other
//! layer reasons about *this* type, never the raw strings.

use std::fmt;

/// Semantic state of a workflow run, derived from the GitHub `(status, conclusion)` pair.
///
/// Ordering of variants encodes severity (worst first) so a list of runs can be
/// sorted to surface the most attention-worthy state via [`RunState::severity`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunState {
    /// Run failed (`conclusion == "failure"`).
    Failed,
    /// Run was cancelled by a user or the system.
    Cancelled,
    /// Run timed out.
    TimedOut,
    /// Run requires manual action (e.g. waiting approval) or action required.
    ActionRequired,
    /// Run is currently executing.
    InProgress,
    /// Run is queued / waiting / requested / pending.
    Queued,
    /// Run completed successfully.
    Success,
    /// Run was skipped.
    Skipped,
    /// Run was neutral (completed without a pass/fail verdict).
    Neutral,
    /// State could not be mapped — preserves the raw label for diagnostics.
    Unknown,
}

impl RunState {
    /// Derive the semantic state from GitHub's raw `status` and `conclusion`.
    ///
    /// This is the *only* place the raw API strings are interpreted.
    pub fn from_api(status: &str, conclusion: Option<&str>) -> Self {
        // Once completed, the conclusion carries the real verdict.
        if status.eq_ignore_ascii_case("completed") {
            return match conclusion.map(str::to_ascii_lowercase).as_deref() {
                Some("success") => Self::Success,
                Some("failure") => Self::Failed,
                Some("cancelled") | Some("canceled") => Self::Cancelled,
                Some("timed_out") => Self::TimedOut,
                Some("action_required") => Self::ActionRequired,
                Some("skipped") => Self::Skipped,
                Some("neutral") => Self::Neutral,
                Some("startup_failure") => Self::Failed,
                Some("stale") => Self::Cancelled,
                _ => Self::Unknown,
            };
        }

        match status.to_ascii_lowercase().as_str() {
            "in_progress" => Self::InProgress,
            "queued" | "waiting" | "requested" | "pending" => Self::Queued,
            "action_required" => Self::ActionRequired,
            _ => Self::Unknown,
        }
    }

    /// True while the run has not reached a terminal state — used to drive polling.
    pub fn is_active(self) -> bool {
        matches!(self, Self::InProgress | Self::Queued)
    }

    /// True for terminal failure-like states worth alerting on.
    pub fn is_failure(self) -> bool {
        matches!(self, Self::Failed | Self::TimedOut | Self::Cancelled)
    }

    /// Severity rank (lower = more urgent). Drives sorting of project lists.
    pub fn severity(self) -> u8 {
        match self {
            Self::Failed => 0,
            Self::TimedOut => 1,
            Self::ActionRequired => 2,
            Self::Cancelled => 3,
            Self::InProgress => 4,
            Self::Queued => 5,
            Self::Unknown => 6,
            Self::Neutral => 7,
            Self::Skipped => 8,
            Self::Success => 9,
        }
    }

    /// Short human label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
            Self::TimedOut => "Timed out",
            Self::ActionRequired => "Action required",
            Self::InProgress => "In progress",
            Self::Queued => "Queued",
            Self::Success => "Success",
            Self::Skipped => "Skipped",
            Self::Neutral => "Neutral",
            Self::Unknown => "Unknown",
        }
    }

    /// A presentation-agnostic glyph (no color — color belongs to the theme).
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Failed | Self::TimedOut => "✗",
            Self::Cancelled => "⊘",
            Self::ActionRequired => "!",
            Self::InProgress => "●",
            Self::Queued => "◷",
            Self::Success => "✓",
            Self::Skipped => "»",
            Self::Neutral => "–",
            Self::Unknown => "?",
        }
    }
}

impl fmt::Display for RunState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_success_maps_to_success() {
        assert_eq!(
            RunState::from_api("completed", Some("success")),
            RunState::Success
        );
    }

    #[test]
    fn completed_failure_maps_to_failed() {
        assert_eq!(
            RunState::from_api("completed", Some("failure")),
            RunState::Failed
        );
        assert_eq!(
            RunState::from_api("completed", Some("startup_failure")),
            RunState::Failed
        );
    }

    #[test]
    fn in_progress_is_active() {
        let s = RunState::from_api("in_progress", None);
        assert_eq!(s, RunState::InProgress);
        assert!(s.is_active());
    }

    #[test]
    fn queued_variants() {
        for raw in ["queued", "waiting", "requested", "pending"] {
            assert_eq!(RunState::from_api(raw, None), RunState::Queued, "{raw}");
        }
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(
            RunState::from_api("COMPLETED", Some("SUCCESS")),
            RunState::Success
        );
    }

    #[test]
    fn severity_orders_failure_first() {
        assert!(RunState::Failed.severity() < RunState::Success.severity());
        assert!(RunState::InProgress.severity() < RunState::Success.severity());
    }

    #[test]
    fn unknown_for_unmapped() {
        assert_eq!(
            RunState::from_api("completed", Some("???")),
            RunState::Unknown
        );
        assert_eq!(RunState::from_api("weird", None), RunState::Unknown);
    }
}
