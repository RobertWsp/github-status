//! Adaptive polling tiers — the Single Source of Truth for *how often* a given
//! project should be re-checked.
//!
//! Rather than polling every repository on a fixed global timer (which spikes
//! the GitHub API and risks rate limits when tracking hundreds of repos), each
//! project is classified into a [`PollTier`] from its current state and recent
//! history. Each tier maps to a base interval, so a repo with a run *in
//! progress* is checked every few seconds while a long-green or CI-less repo is
//! checked rarely. This module is pure: no clock, no I/O.

use std::time::Duration;

/// How "hot" a project is, deciding its polling cadence. Ordered hottest-first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollTier {
    /// Has not been fetched yet — poll immediately.
    Idle,
    /// A run is currently in progress or queued — poll very frequently.
    Active,
    /// Recently active or last run failed — poll often (a fix may land).
    Recent,
    /// Healthy and quiet for a while — poll slowly.
    Stable,
    /// No CI/CD at all (loaded, but zero runs) — poll very rarely.
    Dormant,
    /// Repeated fetch errors — poll on an exponential backoff.
    Backoff,
}

impl PollTier {
    /// Human label for diagnostics / the detail pane.
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Active => "active",
            Self::Recent => "recent",
            Self::Stable => "stable",
            Self::Dormant => "dormant",
            Self::Backoff => "backoff",
        }
    }
}

/// Base polling intervals per tier. Tunable via config so users can trade
/// freshness for fewer API calls. `Idle` is always "now" (zero) and `Backoff`
/// is computed from `backoff_base` × 2^errors, so they aren't stored here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollIntervals {
    pub active: Duration,
    pub recent: Duration,
    pub stable: Duration,
    pub dormant: Duration,
    pub backoff_base: Duration,
    pub backoff_cap: Duration,
    /// How long to pause *all* polling after GitHub returns a rate-limit error.
    pub rate_limit_cooldown: Duration,
}

impl Default for PollIntervals {
    fn default() -> Self {
        Self {
            active: Duration::from_secs(10),
            recent: Duration::from_secs(45),
            stable: Duration::from_secs(300),
            dormant: Duration::from_secs(1800),
            backoff_base: Duration::from_secs(30),
            backoff_cap: Duration::from_secs(900),
            rate_limit_cooldown: Duration::from_secs(60),
        }
    }
}

impl PollIntervals {
    /// The wait before a project of `tier` becomes due again.
    ///
    /// `consecutive_errors` only matters for [`PollTier::Backoff`], where the
    /// wait grows exponentially (`base × 2^(errors-1)`), capped at `backoff_cap`.
    pub fn interval_for(&self, tier: PollTier, consecutive_errors: u32) -> Duration {
        match tier {
            PollTier::Idle => Duration::ZERO,
            PollTier::Active => self.active,
            PollTier::Recent => self.recent,
            PollTier::Stable => self.stable,
            PollTier::Dormant => self.dormant,
            PollTier::Backoff => {
                let shift = consecutive_errors.saturating_sub(1).min(16);
                let scaled = self.backoff_base.saturating_mul(1u32 << shift);
                scaled.min(self.backoff_cap)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_is_immediate() {
        let i = PollIntervals::default();
        assert_eq!(i.interval_for(PollTier::Idle, 0), Duration::ZERO);
    }

    #[test]
    fn active_polls_faster_than_stable() {
        let i = PollIntervals::default();
        assert!(i.interval_for(PollTier::Active, 0) < i.interval_for(PollTier::Stable, 0));
    }

    #[test]
    fn dormant_is_the_slowest_non_backoff() {
        let i = PollIntervals::default();
        assert!(i.interval_for(PollTier::Dormant, 0) > i.interval_for(PollTier::Stable, 0));
    }

    #[test]
    fn backoff_grows_exponentially_and_caps() {
        let i = PollIntervals::default();
        assert_eq!(
            i.interval_for(PollTier::Backoff, 1),
            Duration::from_secs(30)
        );
        assert_eq!(
            i.interval_for(PollTier::Backoff, 2),
            Duration::from_secs(60)
        );
        assert_eq!(
            i.interval_for(PollTier::Backoff, 3),
            Duration::from_secs(120)
        );
        // Caps out rather than growing forever.
        assert_eq!(i.interval_for(PollTier::Backoff, 99), i.backoff_cap);
    }
}
