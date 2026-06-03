//! Small pure formatting helpers shared by widgets (relative time, durations).

use chrono::{DateTime, Duration, Utc};

/// Compact "time ago" string, e.g. `3m ago`, `2h ago`, `just now`.
pub fn relative_time(then: DateTime<Utc>) -> String {
    let delta = Utc::now() - then;
    humanize(delta, "ago")
}

/// Compact duration like `1m 12s`, `45s`, `2h 3m`.
pub fn humanize_duration(d: Duration) -> String {
    let secs = d.num_seconds().max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

fn humanize(delta: Duration, suffix: &str) -> String {
    let secs = delta.num_seconds();
    if secs < 5 {
        return "just now".to_string();
    }
    let (value, unit) = if secs < 60 {
        (secs, "s")
    } else if secs < 3600 {
        (secs / 60, "m")
    } else if secs < 86_400 {
        (secs / 3600, "h")
    } else {
        (secs / 86_400, "d")
    };
    format!("{value}{unit} {suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_is_just_now() {
        assert_eq!(relative_time(Utc::now()), "just now");
    }

    #[test]
    fn minutes_ago() {
        let then = Utc::now() - Duration::minutes(3);
        assert_eq!(relative_time(then), "3m ago");
    }

    #[test]
    fn duration_formats() {
        assert_eq!(humanize_duration(Duration::seconds(45)), "45s");
        assert_eq!(humanize_duration(Duration::seconds(72)), "1m 12s");
        assert_eq!(humanize_duration(Duration::seconds(7380)), "2h 3m");
    }
}
