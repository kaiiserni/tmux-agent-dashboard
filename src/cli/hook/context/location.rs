use crate::event::WorktreeInfo;
use crate::tmux;

/// While subagents are active, pane-scoped writes from the child must not
/// overwrite the parent pane's metadata.
pub(in crate::cli::hook) fn should_update_cwd(current_subagents: &str) -> bool {
    current_subagents.is_empty()
}

pub(in crate::cli::hook) fn resolve_cwd<'a>(
    raw_cwd: &'a str,
    worktree: &'a Option<WorktreeInfo>,
) -> &'a str {
    if let Some(wt) = worktree
        && !wt.original_repo_dir.is_empty()
    {
        return &wt.original_repo_dir;
    }
    raw_cwd
}

pub(in crate::cli::hook) fn sync_worktree_meta(pane: &str, worktree: &Option<WorktreeInfo>) {
    if let Some(wt) = worktree {
        if wt.name.is_empty() {
            tmux::unset_pane_option(pane, tmux::PANE_WORKTREE_NAME);
        } else {
            tmux::set_pane_option(pane, tmux::PANE_WORKTREE_NAME, &wt.name);
        }
        if wt.branch.is_empty() {
            tmux::unset_pane_option(pane, tmux::PANE_WORKTREE_BRANCH);
        } else {
            tmux::set_pane_option(pane, tmux::PANE_WORKTREE_BRANCH, &wt.branch);
        }
    } else {
        tmux::unset_pane_option(pane, tmux::PANE_WORKTREE_NAME);
        tmux::unset_pane_option(pane, tmux::PANE_WORKTREE_BRANCH);
    }
}

pub(in crate::cli::hook) fn sync_pane_location(
    pane: &str,
    cwd: &str,
    worktree: &Option<WorktreeInfo>,
    session_id: &Option<String>,
) {
    let current_subagents = tmux::get_pane_option_value(pane, tmux::PANE_SUBAGENTS);
    if !should_update_cwd(&current_subagents) {
        return;
    }
    match session_id.as_deref() {
        Some(sid) if !sid.is_empty() => tmux::set_pane_option(pane, tmux::PANE_SESSION_ID, sid),
        _ => tmux::unset_pane_option(pane, tmux::PANE_SESSION_ID),
    }
    if !cwd.is_empty() {
        let effective_cwd = resolve_cwd(cwd, worktree);
        tmux::set_pane_option(pane, tmux::PANE_CWD, effective_cwd);
    }
    sync_worktree_meta(pane, worktree);
}

pub(in crate::cli::hook) fn pane_writes_allowed(pane: &str) -> bool {
    let current_subagents = tmux::get_pane_option_value(pane, tmux::PANE_SUBAGENTS);
    should_update_cwd(&current_subagents)
}
