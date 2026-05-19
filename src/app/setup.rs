use crate::state::AppState;
use crate::ui;

/// Prime the initial AppState before the event loop starts.
pub(super) fn init_state(tmux_pane: String) -> AppState {
    let mut state = AppState::new(tmux_pane);
    state.theme = ui::colors::ColorTheme::from_tmux();
    state.icons = ui::icons::StatusIcons::from_tmux();
    state.refresh();
    // Tiles view defaults to all-expanded; only seed a single expanded
    // group when that mode is off so the `f` cycle has a target.
    if !state.expand_all_groups {
        super::input::init_expanded_group(&mut state);
    }
    state
}
