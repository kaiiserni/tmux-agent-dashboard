//! Dashboard event loop. Far simpler than the sidebar: no background
//! workers, no SIGUSR1 plumbing, no git/version pollers. The popup is
//! short-lived; the loop refreshes once per second and reacts to input
//! in between.

use std::io;
use std::time::Duration;

use crossterm::event::{self};
use ratatui::{Terminal, backend::CrosstermBackend};

pub mod input;
mod render;
mod setup;

pub fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    tmux_pane: String,
) -> io::Result<()> {
    let mut state = setup::init_state(tmux_pane);
    let mut last_refresh = std::time::Instant::now();
    let refresh_interval = Duration::from_secs(1);
    let mut needs_redraw = true;

    loop {
        if needs_redraw {
            render::render_frame(terminal, &mut state)?;
            needs_redraw = false;
        }

        let timeout = refresh_interval
            .saturating_sub(last_refresh.elapsed())
            .min(Duration::from_millis(50));

        if event::poll(timeout)? {
            loop {
                let ev = event::read()?;
                if input::handle_event(ev, &mut state, terminal) {
                    needs_redraw = true;
                }
                if state.should_exit {
                    return Ok(());
                }
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }

        if last_refresh.elapsed() >= refresh_interval {
            state.refresh();
            last_refresh = std::time::Instant::now();
            needs_redraw = true;
        }
    }
}
