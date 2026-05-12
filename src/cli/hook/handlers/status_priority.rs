//! Central resolver for the dashboard's pane status priority.
//!
//! Priority (highest → lowest):
//!
//! ```text
//! running > permission > background > waiting > idle
//! ```
//!
//! Permission-class wait reasons stay as `waiting` even when a background
//! shell is live — the user still has to act on the prompt.

pub(in crate::cli::hook) fn is_permission_wait_reason(wait_reason: &str) -> bool {
    matches!(
        wait_reason,
        "permission" | "permission_prompt" | "permission_denied" | "elicitation_dialog"
    )
}

pub(in crate::cli::hook) fn resolve_stop_status(bg_shell_live: bool) -> &'static str {
    if bg_shell_live { "background" } else { "idle" }
}

pub(in crate::cli::hook) fn resolve_notification_status(
    wait_reason: &str,
    bg_shell_live: bool,
) -> &'static str {
    if bg_shell_live && !is_permission_wait_reason(wait_reason) {
        "background"
    } else {
        "waiting"
    }
}
