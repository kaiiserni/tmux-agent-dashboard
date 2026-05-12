use super::location::{pane_writes_allowed, sync_pane_location};
use crate::event::WorktreeInfo;
use crate::tmux;

/// Bundle of payload fields shared by every "lifecycle" event variant.
pub(in crate::cli::hook) struct AgentContext<'a> {
    pub(in crate::cli::hook) agent: &'a str,
    pub(in crate::cli::hook) cwd: &'a str,
    pub(in crate::cli::hook) permission_mode: &'a str,
    pub(in crate::cli::hook) worktree: &'a Option<WorktreeInfo>,
    pub(in crate::cli::hook) session_id: &'a Option<String>,
}

pub(in crate::cli::hook) fn make_ctx<'a>(
    agent: &'a str,
    cwd: &'a str,
    permission_mode: &'a str,
    worktree: &'a Option<WorktreeInfo>,
    session_id: &'a Option<String>,
) -> AgentContext<'a> {
    AgentContext {
        agent,
        cwd,
        permission_mode,
        worktree,
        session_id,
    }
}

pub(in crate::cli::hook) fn set_agent_meta(pane: &str, ctx: &AgentContext<'_>) {
    tmux::set_pane_option(pane, tmux::PANE_AGENT, ctx.agent);
    // Permission mode is parent-owned: gate the write so a subagent does
    // not flip the parent badge.
    if !ctx.permission_mode.is_empty() && pane_writes_allowed(pane) {
        tmux::set_pane_option(pane, tmux::PANE_PERMISSION_MODE, ctx.permission_mode);
    }
    sync_pane_location(pane, ctx.cwd, ctx.worktree, ctx.session_id);
}

pub(in crate::cli::hook) fn clear_run_state(pane: &str) {
    tmux::unset_pane_option(pane, tmux::PANE_STARTED_AT);
    tmux::unset_pane_option(pane, tmux::PANE_WAIT_REASON);
}

pub(in crate::cli::hook) fn is_system_message(s: &str) -> bool {
    s.contains("<task-notification>") || s.contains("<system-reminder>") || s.contains("<task-")
}

pub(in crate::cli::hook) fn clear_all_meta(pane: &str) {
    for key in &[
        tmux::PANE_AGENT,
        tmux::PANE_PROMPT,
        tmux::PANE_PROMPT_SOURCE,
        tmux::PANE_BG_CMD,
        tmux::PANE_SUBAGENTS,
        tmux::PANE_CWD,
        tmux::PANE_PERMISSION_MODE,
        tmux::PANE_WORKTREE_NAME,
        tmux::PANE_WORKTREE_BRANCH,
        tmux::PANE_SESSION_ID,
    ] {
        tmux::unset_pane_option(pane, key);
    }
    clear_run_state(pane);
}

/// Write a task-reset sentinel to the activity log so future task-progress
/// parsing treats the next run as a fresh batch.
///
/// Skipped while subagents are still active so a parent Stop doesn't wipe
/// task state children are still driving.
pub(in crate::cli::hook) fn mark_task_reset(pane: &str) {
    if !pane_writes_allowed(pane) {
        return;
    }
    crate::cli::hook::activity::write_activity_entry(pane, crate::activity::TASK_RESET_MARKER, "");
}
