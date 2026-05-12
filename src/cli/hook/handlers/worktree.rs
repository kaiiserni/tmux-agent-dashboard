use crate::tmux;

use super::super::context::{pane_writes_allowed, sync_worktree_meta};

pub(in crate::cli::hook) fn on_worktree_remove(pane: &str) -> i32 {
    // Skip teardown while subagents are active: the removed worktree may
    // belong to a child, and we cannot tell parent from child at this
    // point. The deferred-teardown dance from the sidebar fork was
    // dropped intentionally for v1 — accept the small leak of stale
    // worktree metadata vs. wiping a live parent.
    if !pane_writes_allowed(pane) {
        return 0;
    }
    sync_worktree_meta(pane, &None);
    tmux::unset_pane_option(pane, tmux::PANE_CWD);
    0
}
