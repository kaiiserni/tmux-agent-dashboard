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
use tmux_agent_dashboard::{
    app,
    cli::hook,
    pending::{self, PendingEntry, Priority},
    tmux,
};

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
        Some("status-line") => {
            return cmd_status_line();
        }
        Some("next") => {
            return cmd_next();
        }
        Some("back") => {
            return cmd_back();
        }
        Some("hook") => {
            let code = hook::cmd_hook(&args[1..]);
            std::process::exit(code);
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

// ─── status-line ────────────────────────────────────────────────────
//
// Outputs a tmux-format string listing pending panes for the bottom
// status bar. Truncates with "+N more" if it would overflow the client
// width.

fn cmd_status_line() -> io::Result<()> {
    let entries = pending::collect_pending();
    if entries.is_empty() {
        println!();
        return Ok(());
    }

    let width: usize = tmux::run_tmux(&["display-message", "-p", "#{client_width}"])
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(200);

    const SEP: &str = "  #[fg=colour240]│#[default]  ";
    const SEP_VISIBLE: usize = 5; // "  │  "

    let total = entries.len();
    let mut shown = 0;
    let mut out = String::new();
    let mut visible_len = 0usize;

    for entry in &entries {
        let chunk = format_entry(entry);
        let chunk_visible = visible_width(&chunk);
        let add = if shown == 0 {
            chunk_visible
        } else {
            SEP_VISIBLE + chunk_visible
        };
        // Reserve space for the trailing "+N more" suffix when not last.
        let remaining = total - shown - 1;
        let suffix_visible = if remaining > 0 {
            SEP_VISIBLE + format!("+{remaining} more").len()
        } else {
            0
        };

        if visible_len + add + suffix_visible > width && shown > 0 {
            break;
        }

        if shown > 0 {
            out.push_str(SEP);
        }
        out.push_str(&chunk);
        visible_len += add;
        shown += 1;
    }

    if shown < total {
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!(
                "  #[fg=colour240]│#[fg=yellow,bold]  +{} more#[default]",
                total - shown
            ),
        );
    }

    println!("{out}");
    Ok(())
}

fn format_entry(e: &PendingEntry) -> String {
    let (icon, icon_color) = match e.priority {
        Priority::Attention => ("▲", 167),
        Priority::Error => ("✕", 167),
        Priority::Waiting => ("◐", 221),
        Priority::Responded => ("↩", 221),
        Priority::MarkedUnread => ("📌", 117),
    };
    let agent_glyph = e.agent.glyph();
    let agent_color = match e.agent {
        tmux::AgentType::Claude => 174,
        tmux::AgentType::Codex => 141,
        tmux::AgentType::OpenCode => 117,
        tmux::AgentType::Unknown => 244,
    };
    let reason = if !e.wait_reason.is_empty() {
        format!(" #[fg=colour244]({})#[default]", e.wait_reason)
    } else {
        String::new()
    };
    format!(
        "#[fg=colour{icon_color},bold]{icon}#[default] \
         #[fg=colour{agent_color}]{agent_glyph}#[default] \
         #[fg=colour255]{repo}#[default]\
         #[fg=colour109]/{label}#[default]{reason}",
        repo = e.repo,
        label = e.label,
    )
}

/// Display width of a tmux-format string — strip `#[...]` escapes first.
fn visible_width(s: &str) -> usize {
    let mut w = 0;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '#'
            && let Some('[') = chars.clone().next()
        {
            for c2 in chars.by_ref() {
                if c2 == ']' {
                    break;
                }
            }
            continue;
        }
        w += unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
    }
    w
}

// ─── next ───────────────────────────────────────────────────────────
//
// Switches the tmux client to the highest-priority pending pane. Same
// heuristic as `status-line` so the bottom bar and `prefix + n` agree.

// Global option keys used to record the most recent `next` jump so
// `back` can undo it.
const JUMP_FROM: &str = "@dashboard_jump_from";
const JUMP_TO: &str = "@dashboard_jump_to";

fn cmd_next() -> io::Result<()> {
    let entries = pending::collect_pending();
    let target = match entries.first() {
        Some(t) => t,
        None => {
            let _ = tmux::run_tmux(&["display-message", "No pending agents"]);
            return Ok(());
        }
    };

    let from = tmux::run_tmux(&["display-message", "-p", "#{pane_id}"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if !from.is_empty() && from != target.pane_id {
        let _ = tmux::run_tmux(&["set", "-g", JUMP_FROM, &from]);
        let _ = tmux::run_tmux(&["set", "-g", JUMP_TO, &target.pane_id]);
    }

    tmux::select_pane(&target.pane_id);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    tmux::set_pane_option(&target.pane_id, tmux::PANE_LAST_SEEN_AT, &now.to_string());
    Ok(())
}

/// Undo the most recent `next` jump: switch back to the pane we came
/// from AND mark the pane we just visited with
/// `@dashboard_marked_unread_at` so it surfaces in the Marked Unread
/// box. Without a `next`-history, mark the current pane in place.
fn cmd_back() -> io::Result<()> {
    let from = tmux::run_tmux(&["show", "-gv", JUMP_FROM])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let to = tmux::run_tmux(&["show", "-gv", JUMP_TO])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let target = if !to.is_empty() {
        to.clone()
    } else {
        // No jump history → mark the current active pane in place.
        tmux::run_tmux(&["display-message", "-p", "#{pane_id}"])
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };

    if target.is_empty() {
        return Ok(());
    }

    tmux::set_pane_option(&target, tmux::PANE_MARKED_UNREAD_AT, &now.to_string());

    if !from.is_empty() {
        tmux::select_pane(&from);
    }

    let _ = tmux::run_tmux(&["set", "-gu", JUMP_FROM]);
    let _ = tmux::run_tmux(&["set", "-gu", JUMP_TO]);
    Ok(())
}
