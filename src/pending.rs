//! Shared "panes that need attention" computation used by the
//! `status-line` and `next` subcommands.
//!
//! Same heuristic the dashboard's Responded / Waiting / Attention lists
//! apply — so the bottom bar, the `prefix + n` jump, and the popup all
//! agree on what's pending.

use std::time::SystemTime;

use crate::activity::log_mtime;
use crate::group::{PaneGitInfo, RepoGroup, group_panes_by_repo};
use crate::tmux::{self, PaneInfo, PaneStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// `@pane_attention` explicitly set by a hook (Notification,
    /// PermissionDenied, TeammateIdle).
    Attention,
    Error,
    Waiting,
    /// Agent finished a turn but the user hasn't looked yet.
    Responded,
    /// User-pinned via `prefix + N` or dashboard `m`. Lowest urgency.
    MarkedUnread,
}

#[derive(Debug, Clone)]
pub struct PendingEntry {
    pub priority: Priority,
    pub pane_id: String,
    pub repo: String,
    pub label: String,
    pub status: PaneStatus,
    pub mtime: SystemTime,
    pub wait_reason: String,
    pub agent: tmux::AgentType,
}

pub fn collect_pending() -> Vec<PendingEntry> {
    let sessions = tmux::query_sessions();
    let mut groups: Vec<RepoGroup> = group_panes_by_repo(&sessions);
    sweep_stale_marks(&mut groups);

    let show_technical = tmux::get_option(tmux::DASHBOARD_SHOW_TECHNICAL_NAMES)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let mut out: Vec<PendingEntry> = Vec::new();
    for group in &groups {
        for (pane, info) in &group.panes {
            if let Some(entry) = classify(pane, info, &group.name, show_technical) {
                out.push(entry);
            }
        }
    }

    // Priority first (lowest enum variant = most urgent), then most-recent
    // mtime first.
    out.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| b.mtime.cmp(&a.mtime))
    });

    // `o` in the jump picker flips the order. Marked-unread stays at the
    // back regardless (it's the lowest priority), so only the rest reverses.
    let reverse = tmux::get_option(tmux::DASHBOARD_PENDING_REVERSE)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if reverse {
        let (mut rest, marked): (Vec<_>, Vec<_>) = out
            .into_iter()
            .partition(|e| e.priority != Priority::MarkedUnread);
        rest.reverse();
        rest.extend(marked);
        rest
    } else {
        out
    }
}

/// Every agent pane as an entry, regardless of pending state — feeds the
/// jump picker's fuzzy search mode. Ranking is left to the caller (fuzzy
/// score); here entries come out in the stable group order.
pub fn collect_all() -> Vec<PendingEntry> {
    let sessions = tmux::query_sessions();
    let mut groups: Vec<RepoGroup> = group_panes_by_repo(&sessions);
    sweep_stale_marks(&mut groups);

    let show_technical = tmux::get_option(tmux::DASHBOARD_SHOW_TECHNICAL_NAMES)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let mut out: Vec<PendingEntry> = Vec::new();
    for group in &groups {
        for (pane, info) in &group.panes {
            let mtime = log_mtime(&pane.pane_id).unwrap_or(SystemTime::UNIX_EPOCH);
            let priority = pane_priority(pane, mtime);
            out.push(build_entry(pane, info, &group.name, show_technical, priority, mtime));
        }
    }
    out
}

/// Best-effort priority for any pane, falling back to MarkedUnread (lowest
/// urgency) for plain running/idle panes that aren't really "pending".
fn pane_priority(pane: &PaneInfo, mtime: SystemTime) -> Priority {
    if pane.attention {
        return Priority::Attention;
    }
    match pane.status {
        PaneStatus::Error => Priority::Error,
        PaneStatus::Waiting => Priority::Waiting,
        PaneStatus::Idle if is_unseen(pane, mtime) => Priority::Responded,
        _ => Priority::MarkedUnread,
    }
}

fn classify(
    pane: &PaneInfo,
    info: &PaneGitInfo,
    repo: &str,
    show_technical: bool,
) -> Option<PendingEntry> {
    let mtime = log_mtime(&pane.pane_id).unwrap_or(SystemTime::UNIX_EPOCH);
    let priority = if pane.attention {
        Priority::Attention
    } else {
        match pane.status {
            PaneStatus::Error => Priority::Error,
            PaneStatus::Waiting => Priority::Waiting,
            PaneStatus::Idle if is_unseen(pane, mtime) => Priority::Responded,
            PaneStatus::Idle if pane.marked_unread_at.is_some() => Priority::MarkedUnread,
            _ => return None,
        }
    };

    Some(build_entry(pane, info, repo, show_technical, priority, mtime))
}

fn build_entry(
    pane: &PaneInfo,
    info: &PaneGitInfo,
    repo: &str,
    show_technical: bool,
    priority: Priority,
    mtime: SystemTime,
) -> PendingEntry {
    let (repo, label) = if show_technical {
        (repo.to_string(), label_technical(pane, info))
    } else {
        let r = if !pane.tmux_session_name.is_empty() {
            pane.tmux_session_name.clone()
        } else {
            repo.to_string()
        };
        let friendly = friendly_pane_label(pane);
        let l = if friendly.is_empty() {
            label_technical(pane, info)
        } else {
            friendly
        };
        (r, l)
    };

    PendingEntry {
        priority,
        pane_id: pane.pane_id.clone(),
        repo,
        label,
        status: pane.status.clone(),
        mtime,
        wait_reason: pane.wait_reason.clone(),
        agent: pane.agent.clone(),
    }
}

fn friendly_pane_label(pane: &PaneInfo) -> String {
    if !pane.pane_name.is_empty() {
        return pane.pane_name.clone();
    }
    if !pane.auto_rename && !pane.window_name.is_empty() {
        return pane.window_name.clone();
    }
    String::new()
}

fn is_unseen(pane: &PaneInfo, mtime: SystemTime) -> bool {
    pane_is_unseen_with(pane, mtime)
}

/// Public: same heuristic the dashboard's Responded box uses. Compares
/// the activity log mtime against `@pane_last_seen_at`. Re-exported here
/// so dashboard rendering and pending list agree.
pub fn pane_is_unseen(pane: &PaneInfo) -> bool {
    let mtime = log_mtime(&pane.pane_id).unwrap_or(SystemTime::UNIX_EPOCH);
    pane_is_unseen_with(pane, mtime)
}

fn pane_is_unseen_with(pane: &PaneInfo, mtime: SystemTime) -> bool {
    let log_secs = mtime
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if log_secs == 0 {
        return false;
    }
    match pane.last_seen_at {
        None => true,
        Some(seen) => log_secs > seen,
    }
}

/// Clears stale `@dashboard_marked_unread_at` markers per the auto-clear
/// rule: a mark only survives while the pane is purely Idle (no
/// attention, no auto-Responded activity). Mutates `groups` so the
/// caller sees the cleared field on this pass, and writes through to
/// tmux so the next process invocation also sees the cleared state.
pub fn sweep_stale_marks(groups: &mut [crate::group::RepoGroup]) {
    for group in groups.iter_mut() {
        for (pane, _) in group.panes.iter_mut() {
            if pane.marked_unread_at.is_none() {
                continue;
            }
            let stale =
                pane.attention || !matches!(pane.status, PaneStatus::Idle) || pane_is_unseen(pane);
            if stale {
                tmux::unset_pane_option(&pane.pane_id, tmux::PANE_MARKED_UNREAD_AT);
                pane.marked_unread_at = None;
            }
        }
    }
}

fn label_technical(pane: &PaneInfo, info: &PaneGitInfo) -> String {
    if !pane.session_name.is_empty() {
        return pane.session_name.clone();
    }
    if let Some(b) = info.branch.as_ref().filter(|s| !s.is_empty()) {
        return b.clone();
    }
    if !pane.worktree.branch.is_empty() {
        return pane.worktree.branch.clone();
    }
    "-".into()
}
