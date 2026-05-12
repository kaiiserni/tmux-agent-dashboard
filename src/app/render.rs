use std::io;

use ratatui::{Terminal, backend::CrosstermBackend};

use crate::state::AppState;
use crate::ui;

pub(super) fn render_frame(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
) -> io::Result<()> {
    terminal.draw(|f| ui::dashboard::draw_dashboard(f, state))?;
    Ok(())
}
