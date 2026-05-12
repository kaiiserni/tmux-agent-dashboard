//! Entry point: dispatches `dashboard` (default) and `seen` subcommands,
//! resolves TMUX_PANE around tmux 3.5's format-expansion gap in
//! `display-popup`, then opens the TUI.

use std::io;

use crossterm::{
    cursor::{Hide, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tmux_agent_dashboard::{app, tmux};

struct TuiSession {
    entered_alt_screen: bool,
}

impl TuiSession {
    fn enter(stdout: &mut io::Stdout) -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(err) = execute!(stdout, EnterAlternateScreen, EnableMouseCapture, Hide) {
            let _ = disable_raw_mode();
            return Err(err);
        }
        Ok(Self {
            entered_alt_screen: true,
        })
    }
}

impl Drop for TuiSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        if self.entered_alt_screen {
            let mut stdout = io::stdout();
            let _ = execute!(stdout, Show, LeaveAlternateScreen, DisableMouseCapture);
        }
    }
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        Some("seen") => {
            return cmd_seen(&args[1..]);
        }
        Some("--version") | Some("version") => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        // Treat `dashboard` (or no args) as the default TUI invocation.
        _ => {}
    }

    let tmux_pane = resolve_tmux_pane();
    if tmux_pane.is_empty() {
        eprintln!("TMUX_PANE not set and tmux display-message returned empty");
        std::process::exit(1);
    }

    let mut stdout = io::stdout();
    let _tui_session = TuiSession::enter(&mut stdout)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    app::run(&mut terminal, tmux_pane)
}

/// Resolve the origin pane id. tmux 3.5's `display-popup` does NOT expand
/// `#{pane_id}` in its command argument or `-e` values, so the popup is
/// launched without a usable `$TMUX_PANE`. As a fallback we ask tmux
/// directly via `display-message`.
fn resolve_tmux_pane() -> String {
    let from_env = std::env::var("TMUX_PANE").unwrap_or_default();
    if from_env.starts_with('%') {
        return from_env;
    }
    let resolved = tmux::run_tmux(&["display-message", "-p", "#{pane_id}"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if resolved.starts_with('%') {
        resolved
    } else {
        String::new()
    }
}

/// `seen <pane_id>` — write the current epoch to `@pane_last_seen_at` so the
/// dashboard's Responded heuristic knows when the user last focused this
/// pane. Bound to tmux's `after-select-pane` hook.
fn cmd_seen(args: &[String]) -> io::Result<()> {
    let pane = match args.first() {
        Some(p) if !p.is_empty() => p.clone(),
        _ => std::env::var("TMUX_PANE").unwrap_or_default(),
    };
    if pane.is_empty() {
        return Ok(());
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    tmux::set_pane_option(&pane, tmux::PANE_LAST_SEEN_AT, &now.to_string());
    Ok(())
}
