use chrono::{DateTime, Duration as ChronoDuration, Utc};
use std::time::Duration;

pub const DEFAULT_CAPTURE_INTERVAL: Duration = Duration::from_secs(5 * 60);
pub const FIRST_CAPTURE_DELAY: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakePolicy {
    /// Resume the normal schedule without recreating events missed during sleep.
    SkipMissedCaptures,
}

pub fn next_capture_at(now: DateTime<Utc>, delay: Duration) -> DateTime<Utc> {
    now + ChronoDuration::from_std(delay).expect("capture delays fit in chrono durations")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_capture_is_scheduled_ten_seconds_after_start() {
        let now = DateTime::parse_from_rfc3339("2026-07-28T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        assert_eq!(
            next_capture_at(now, FIRST_CAPTURE_DELAY).to_rfc3339(),
            "2026-07-28T12:00:10+00:00"
        );
    }

    #[test]
    fn later_capture_uses_the_configured_interval() {
        let now = DateTime::parse_from_rfc3339("2026-07-28T12:00:10Z")
            .unwrap()
            .with_timezone(&Utc);

        assert_eq!(
            next_capture_at(now, Duration::from_secs(2 * 60)).to_rfc3339(),
            "2026-07-28T12:02:10+00:00"
        );
    }
}
