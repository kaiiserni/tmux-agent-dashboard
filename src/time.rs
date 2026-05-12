//! Wall-clock helper used by the hook subcommand. Centralized so the
//! UNIX_EPOCH fallback (`0` on clock skew) lives in one place.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
