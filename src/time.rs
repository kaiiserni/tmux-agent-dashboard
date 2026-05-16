//! Wall-clock helper used by the hook subcommand. Centralized so the
//! UNIX_EPOCH fallback (`0` on clock skew) lives in one place.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Shortest-possible relative age, e.g. `45s`, `12m`, `3h`, `2d`, `1w`,
/// `5mo`, `1y`. `None` when `ts` is the epoch fallback (no real log) or
/// lies in the future (clock skew) — callers then render no age.
pub fn compact_ago(ts: SystemTime) -> Option<String> {
    if ts <= UNIX_EPOCH {
        return None;
    }
    let secs = SystemTime::now().duration_since(ts).ok()?.as_secs();
    Some(match secs {
        0..=59 => format!("{secs}s"),
        60..=3_599 => format!("{}m", secs / 60),
        3_600..=86_399 => format!("{}h", secs / 3_600),
        86_400..=604_799 => format!("{}d", secs / 86_400),
        604_800..=2_591_999 => format!("{}w", secs / 604_800),
        2_592_000..=31_535_999 => format!("{}mo", secs / 2_592_000),
        _ => format!("{}y", secs / 31_536_000),
    })
}

#[cfg(test)]
mod tests {
    use super::compact_ago;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn ago(d: Duration) -> Option<String> {
        compact_ago(SystemTime::now() - d)
    }

    #[test]
    fn units() {
        assert_eq!(ago(Duration::from_secs(5)).as_deref(), Some("5s"));
        assert_eq!(ago(Duration::from_secs(90)).as_deref(), Some("1m"));
        assert_eq!(ago(Duration::from_secs(2 * 3600)).as_deref(), Some("2h"));
        assert_eq!(ago(Duration::from_secs(3 * 86400)).as_deref(), Some("3d"));
        assert_eq!(ago(Duration::from_secs(2 * 604800)).as_deref(), Some("2w"));
        assert_eq!(ago(Duration::from_secs(60 * 86400)).as_deref(), Some("2mo"));
        assert_eq!(ago(Duration::from_secs(800 * 86400)).as_deref(), Some("2y"));
    }

    #[test]
    fn epoch_and_future_are_none() {
        assert_eq!(compact_ago(UNIX_EPOCH), None);
        assert_eq!(compact_ago(SystemTime::now() + Duration::from_secs(60)), None);
    }
}
