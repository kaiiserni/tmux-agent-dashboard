//! Subagent lifecycle. Only the parent-protection portion is ported —
//! the `@pane_subagents` append/remove list gates pane-scoped writes from
//! other handlers via `pane_writes_allowed`. The upstream sidebar also
//! tracked nested-UI metadata for its own sidebar render; the dashboard
//! doesn't need that.

use crate::tmux;

use super::super::context::{append_subagent, remove_subagent};

pub(in crate::cli::hook) fn on_subagent_start(
    pane: &str,
    agent_type: &str,
    agent_id: Option<&str>,
) -> i32 {
    // Claude always sends agent_id per the hooks spec; drop the event
    // silently if it's missing so an untrackable entry never lands in
    // the list.
    let Some(id) = agent_id.filter(|s| !s.is_empty()) else {
        return 0;
    };
    let current = tmux::get_pane_option_value(pane, tmux::PANE_SUBAGENTS);
    let new_val = append_subagent(&current, agent_type, id);
    tmux::set_pane_option(pane, tmux::PANE_SUBAGENTS, &new_val);
    0
}

pub(in crate::cli::hook) fn on_subagent_stop(pane: &str, agent_id: Option<&str>) -> i32 {
    let Some(id) = agent_id.filter(|s| !s.is_empty()) else {
        return 0;
    };
    let current = tmux::get_pane_option_value(pane, tmux::PANE_SUBAGENTS);
    match remove_subagent(&current, id) {
        None => {}
        Some(new_val) if new_val.is_empty() => {
            tmux::unset_pane_option(pane, tmux::PANE_SUBAGENTS);
        }
        Some(new_val) => {
            tmux::set_pane_option(pane, tmux::PANE_SUBAGENTS, &new_val);
        }
    }
    0
}
