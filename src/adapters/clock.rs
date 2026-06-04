//! Clock adapters — concrete [`Clock`] implementations.

use chrono::{DateTime, Utc};

use crate::ports::Clock;

/// The real wall-clock, backed by the system time.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// A clock pinned to a fixed instant — for deterministic tests. Lives here (not
/// behind `#[cfg(test)]`) so integration tests and benches can use it too.
#[derive(Debug, Clone)]
pub struct FixedClock {
    now: std::sync::Arc<std::sync::Mutex<DateTime<Utc>>>,
}

impl FixedClock {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            now: std::sync::Arc::new(std::sync::Mutex::new(now)),
        }
    }

    /// Advance the clock by `delta` (lets a test simulate time passing).
    pub fn advance(&self, delta: chrono::Duration) {
        let mut guard = self.now.lock().expect("clock mutex");
        *guard += delta;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self.now.lock().expect("clock mutex")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_is_stable_then_advances() {
        let t0 = Utc::now();
        let clock = FixedClock::new(t0);
        assert_eq!(clock.now(), t0);
        clock.advance(chrono::Duration::seconds(30));
        assert_eq!(clock.now(), t0 + chrono::Duration::seconds(30));
    }
}
