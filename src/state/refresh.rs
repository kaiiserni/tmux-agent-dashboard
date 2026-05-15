use std::time::{Duration, Instant};

use super::AppState;
use crate::group::group_panes_with_cache;
use crate::session;
use crate::tmux::{self, PaneStatus};

/// Re-scan `~/.claude/sessions/*.json` at most once per this interval.
const SESSION_NAMES_TTL: Duration = Duration::from_secs(10);

impl AppState {
    /// Pull a fresh `list-panes` snapshot, regroup by repo, and apply the
    /// attention-first sort.
    pub fn refresh(&mut self) {
        let sessions = tmux::query_sessions();
        self.repo_groups = group_panes_with_cache(&sessions, &mut self.git_cache);
        self.reconcile_stale_background();
        self.refresh_session_names();
        self.apply_session_names();
        crate::pending::sweep_stale_marks(&mut self.repo_groups);
        self.sort_groups_if_needed();
    }

    /// Self-heal panes left on `background` after their `run_in_background`
    /// shell already exited. `@pane_bg_cmd` is sticky per session and a
    /// pane that Stopped while it was set never gets re-evaluated, so the
    /// pane would otherwise show `background` forever. Probes liveness and
    /// rewrites the tmux state once so every consumer (counts, status
    /// line, `next`) agrees.
    fn reconcile_stale_background(&mut self) {
        for group in &mut self.repo_groups {
            for (pane, _) in &mut group.panes {
                if pane.status != PaneStatus::Background || pane.bg_cmd.is_empty() {
                    continue;
                }
                if crate::bg::bg_shell_alive(&pane.bg_cmd) {
                    continue;
                }
                tmux::unset_pane_option(&pane.pane_id, tmux::PANE_BG_CMD);
                tmux::set_pane_option(&pane.pane_id, tmux::PANE_STATUS, "idle");
                pane.status = PaneStatus::Idle;
                pane.bg_cmd.clear();
            }
        }
    }

    /// Refresh the cached Claude session-name map, but only when stale.
    /// Sticky: an empty scan never overwrites a non-empty cache (so a
    /// transient FS hiccup can't blank labels).
    fn refresh_session_names(&mut self) {
        let now = Instant::now();
        let stale = self
            .session_names_refreshed_at
            .is_none_or(|t| now.duration_since(t) >= SESSION_NAMES_TTL);
        if !stale {
            return;
        }
        let fresh = session::scan_session_names();
        self.session_names_refreshed_at = Some(now);
        if !fresh.is_empty() || self.session_names.is_empty() {
            self.session_names = fresh;
        }
    }

    /// Patch each pane's `session_name` from the cached lookup. Only
    /// applied when the pane has a known `session_id` and no name yet,
    /// so explicit tmux session names still win.
    fn apply_session_names(&mut self) {
        if self.session_names.is_empty() {
            return;
        }
        for group in &mut self.repo_groups {
            for (pane, _) in &mut group.panes {
                if !pane.session_name.is_empty() {
                    continue;
                }
                if let Some(sid) = pane.session_id.as_deref()
                    && let Some(name) = self.session_names.get(sid)
                {
                    pane.session_name = name.clone();
                }
            }
        }
    }

    /// Attention-first sort applied to panes WITHIN each group only.
    /// Group order stays alphabetical (set by `group_panes_with_cache`)
    /// so `d`/`u` navigation cycles predictably — otherwise the group's
    /// position shifts every time a pane's status flips, and the next
    /// keypress lands somewhere the user doesn't expect.
    pub fn sort_groups_if_needed(&mut self) {
        if !self.sort_by_activity {
            return;
        }
        fn pane_sort_key(p: &tmux::PaneInfo) -> (u8, u8, std::cmp::Reverse<u64>) {
            let attention = if p.attention { 0 } else { 1 };
            let status = match p.status {
                PaneStatus::Waiting => 0,
                PaneStatus::Error => 1,
                PaneStatus::Idle => 2,
                PaneStatus::Running => 3,
                PaneStatus::Background => 3,
                PaneStatus::Unknown => 4,
            };
            (
                attention,
                status,
                std::cmp::Reverse(p.started_at.unwrap_or(0)),
            )
        }
        for group in &mut self.repo_groups {
            group
                .panes
                .sort_by(|(a, _), (b, _)| pane_sort_key(a).cmp(&pane_sort_key(b)));
        }
    }

    /// `(total, running, background, waiting, idle, error)` counts across
    /// every pane in `repo_groups`.
    pub fn status_counts(&self) -> (usize, usize, usize, usize, usize, usize) {
        let mut all = 0;
        let mut running = 0;
        let mut background = 0;
        let mut waiting = 0;
        let mut idle = 0;
        let mut error = 0;
        for group in &self.repo_groups {
            for (pane, _) in &group.panes {
                all += 1;
                match pane.status {
                    PaneStatus::Running => running += 1,
                    PaneStatus::Background => background += 1,
                    PaneStatus::Waiting => waiting += 1,
                    PaneStatus::Idle => idle += 1,
                    PaneStatus::Error => error += 1,
                    PaneStatus::Unknown => {}
                }
            }
        }
        (all, running, background, waiting, idle, error)
    }
}
