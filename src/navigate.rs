//! Shared "jump the client to a pane" logic used by `next`, `goto` and
//! the `jump` picker popup. Records the origin pane in a global option so
//! `back` can undo the most recent jump.

use crate::tmux;

pub const JUMP_FROM: &str = "@dashboard_jump_from";
pub const JUMP_TO: &str = "@dashboard_jump_to";

/// Switch the client to `target` (session/window/pane), recording where we
/// came from so `back` works, and stamp `@pane_last_seen_at` so the
/// Responded heuristic treats the pane as seen.
pub fn jump_to(target: &str) {
    let from = tmux::run_tmux(&["display-message", "-p", "#{pane_id}"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if !from.is_empty() && from != target {
        tmux::set_global_option(JUMP_FROM, &from);
        tmux::set_global_option(JUMP_TO, target);
    }
    tmux::select_pane(target);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    tmux::set_pane_option(target, tmux::PANE_LAST_SEEN_AT, &now.to_string());
    // Drop it off the pending bar immediately rather than on the next tick.
    tmux::refresh_status();
}
