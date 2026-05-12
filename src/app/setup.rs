use crate::state::AppState;
use crate::ui;

/// Prime the initial AppState before the event loop starts.
pub(super) fn init_state(tmux_pane: String) -> AppState {
    let mut state = AppState::new(tmux_pane);
    state.theme = ui::colors::ColorTheme::from_tmux();
    state.icons = ui::icons::StatusIcons::from_tmux();
    state.refresh();
    state
}
