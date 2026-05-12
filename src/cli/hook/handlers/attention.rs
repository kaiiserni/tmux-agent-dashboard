use crate::cli::{set_attention, set_status};
use crate::tmux;

use super::super::context::{AgentContext, set_agent_meta};
use super::status_priority::resolve_notification_status;

pub(in crate::cli::hook) fn on_notification(
    pane: &str,
    ctx: &AgentContext<'_>,
    wait_reason: &str,
    meta_only: bool,
) -> i32 {
    set_agent_meta(pane, ctx);
    if meta_only {
        return 0;
    }
    let bg_shell_live = !tmux::get_pane_option_value(pane, tmux::PANE_BG_CMD).is_empty();
    set_status(
        pane,
        resolve_notification_status(wait_reason, bg_shell_live),
    );
    set_attention(pane, "notification");
    if wait_reason.is_empty() {
        tmux::unset_pane_option(pane, tmux::PANE_WAIT_REASON);
    } else {
        tmux::set_pane_option(pane, tmux::PANE_WAIT_REASON, wait_reason);
    }
    0
}

pub(in crate::cli::hook) fn on_permission_denied(pane: &str, ctx: &AgentContext<'_>) -> i32 {
    set_agent_meta(pane, ctx);
    set_status(pane, "waiting");
    set_attention(pane, "notification");
    tmux::set_pane_option(pane, tmux::PANE_WAIT_REASON, "permission_denied");
    0
}

pub(in crate::cli::hook) fn on_teammate_idle(
    pane: &str,
    teammate_name: &str,
    idle_reason: &str,
) -> i32 {
    set_attention(pane, "notification");
    let reason = if idle_reason.is_empty() {
        format!("teammate_idle:{teammate_name}")
    } else {
        format!("teammate_idle:{teammate_name}:{idle_reason}")
    };
    tmux::set_pane_option(pane, tmux::PANE_WAIT_REASON, &reason);
    0
}
