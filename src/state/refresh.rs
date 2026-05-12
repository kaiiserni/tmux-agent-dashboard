use super::AppState;
use crate::group::group_panes_by_repo;
use crate::tmux::{self, PaneStatus};

impl AppState {
    /// Pull a fresh `list-panes` snapshot, regroup by repo, and apply the
    /// attention-first sort.
    pub fn refresh(&mut self) {
        let sessions = tmux::query_sessions();
        self.repo_groups = group_panes_by_repo(&sessions);
        self.sort_groups_if_needed();
    }

    /// Attention-first sort: panes the user might want to look at bubble
    /// up. Within each repo: attention-flagged first, then by status
    /// (Waiting > Error > Idle > Running > Background > Unknown), then by
    /// most-recent start time. Across groups: the group with the
    /// most-urgent pane sits at the top.
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
        self.repo_groups.sort_by(|a, b| {
            let ka = a.panes.first().map(|(p, _)| pane_sort_key(p)).unwrap_or((
                1,
                4,
                std::cmp::Reverse(0),
            ));
            let kb = b.panes.first().map(|(p, _)| pane_sort_key(p)).unwrap_or((
                1,
                4,
                std::cmp::Reverse(0),
            ));
            ka.cmp(&kb)
        });
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
