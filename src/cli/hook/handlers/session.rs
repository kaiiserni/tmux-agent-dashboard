use crate::cli::{set_attention, set_status};
use crate::tmux;

use super::super::context::{
    AgentContext, clear_all_meta, clear_run_state, pane_writes_allowed, set_agent_meta,
};

pub(in crate::cli::hook) fn on_session_start(
    pane: &str,
    ctx: &AgentContext<'_>,
    source: &str,
) -> i32 {
    set_agent_meta(pane, ctx);
    set_attention(pane, "clear");
    clear_run_state(pane);
    tmux::unset_pane_option(pane, tmux::PANE_PROMPT);
    tmux::unset_pane_option(pane, tmux::PANE_PROMPT_SOURCE);
    // Bug-fix vs. upstream sidebar: also drop `@pane_bg_cmd` so a fresh
    // session does not inherit the previous run's "background hangs"
    // state. Without this, the dashboard keeps treating the pane as if
    // a long-running shell from a now-dead session is still alive.
    tmux::unset_pane_option(pane, tmux::PANE_BG_CMD);
    // `@pane_subagents` is preserved across SessionStart on purpose:
    // subagents share the parent's $TMUX_PANE and may fire their own
    // SessionStart after SubagentStart populates the list.
    match source {
        "resume" => tmux::set_pane_option(pane, tmux::PANE_WAIT_REASON, "session_resumed"),
        "compact" => tmux::set_pane_option(pane, tmux::PANE_WAIT_REASON, "session_resumed_compact"),
        _ => tmux::unset_pane_option(pane, tmux::PANE_WAIT_REASON),
    }
    set_status(pane, "idle");
    0
}

pub(in crate::cli::hook) fn on_session_end(pane: &str, _end_reason: &str) -> i32 {
    // Subagents share the parent's $TMUX_PANE — when the list is
    // non-empty, the SessionEnd is almost certainly a child's. Skip the
    // teardown so the parent state survives. A genuine parent SessionEnd
    // racing ahead of every SubagentStop is the tolerated failure mode:
    // stale metadata is far safer than wiping a live parent.
    if !pane_writes_allowed(pane) {
        return 0;
    }
    set_attention(pane, "clear");
    clear_all_meta(pane);
    set_status(pane, "clear");
    let log_path = crate::activity::log_file_path(pane);
    let _ = std::fs::remove_file(log_path);
    0
}
