use super::AppState;
use crate::tmux;

#[derive(Debug, Clone, Default)]
pub struct FocusState {
    pub focused_pane_id: Option<String>,
}

impl AppState {
    /// Activate an arbitrary pane by id. Used when the user `Enter`s or
    /// clicks a row / tile — we switch the tmux client to that pane and
    /// signal the event loop to exit so the popup closes. Also stamps
    /// `@pane_last_seen_at` directly (in addition to the
    /// `after-select-pane` hook) so the dashboard's Responded heuristic
    /// updates immediately on next refresh, even if the hook is slow or
    /// not registered yet.
    pub fn activate_pane_by_id(&mut self, pane_id: &str) {
        self.focus_state.focused_pane_id = Some(pane_id.to_string());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        tmux::set_pane_option(pane_id, tmux::PANE_LAST_SEEN_AT, &now.to_string());
        tmux::select_pane(pane_id);
    }
}
