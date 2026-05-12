use super::AppState;
use crate::tmux;

#[derive(Debug, Clone, Default)]
pub struct FocusState {
    pub focused_pane_id: Option<String>,
}

impl AppState {
    /// Activate an arbitrary pane by id. Used when the user `Enter`s or
    /// clicks a row / tile — we switch the tmux client to that pane and
    /// signal the event loop to exit so the popup closes.
    pub fn activate_pane_by_id(&mut self, pane_id: &str) {
        self.focus_state.focused_pane_id = Some(pane_id.to_string());
        tmux::select_pane(pane_id);
    }
}
