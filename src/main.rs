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
        Some("goto") => {
            return cmd_goto(&args[1..]);
        }
        Some("back") => {
            return cmd_back();
        }
        Some("mark") => {
            return cmd_mark();
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
        // Wrap the clickable chunk in a tmux mouse range keyed on the
        // pane id; the separator stays outside so gaps aren't clickable.
        // `visible_width` strips `#[...]` so the width math is unaffected.
        out.push_str("#[range=user|");
        out.push_str(&entry.pane_id);
        out.push(']');
        out.push_str(&chunk);
        out.push_str("#[norange]");
        visible_len += add;
        shown += 1;
    }

    if shown < total {
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!(
                "  #[fg=colour240]│#[range=user|+more]#[fg=yellow,bold]  +{} more#[norange]#[default]",
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
    // Marked Unread is a user-curated parking lot, not an urgency
    // signal — don't surface it via `prefix + n`. It stays visible in
    // the status-line.
    let entries: Vec<_> = pending::collect_pending()
        .into_iter()
        .filter(|e| e.priority != Priority::MarkedUnread)
        .collect();
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

/// `goto <pane_id>` — jump straight to a specific pane. Bound to a tmux
/// `MouseDown1Status` range so clicking a status-line item activates its
/// agent. Mirrors `cmd_next`'s jump-recording so `back` still works.
/// `%` followed by at least one digit — tmux's pane-id shape.
fn is_pane_id(s: &str) -> bool {
    s.len() >= 2 && s.starts_with('%') && s[1..].chars().all(|c| c.is_ascii_digit())
}

fn cmd_goto(args: &[String]) -> io::Result<()> {
    let target = match args.first() {
        Some(p) => p.trim(),
        None => return Ok(()),
    };
    // Range value is attacker-free (we emit it ourselves) but guard the
    // pane-id shape anyway; ignore anything else (e.g. a stale id).
    if !is_pane_id(target) {
        return Ok(());
    }

    let from = tmux::run_tmux(&["display-message", "-p", "#{pane_id}"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if !from.is_empty() && from != target {
        let _ = tmux::run_tmux(&["set", "-g", JUMP_FROM, &from]);
        let _ = tmux::run_tmux(&["set", "-g", JUMP_TO, target]);
    }

    tmux::select_pane(target);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    tmux::set_pane_option(target, tmux::PANE_LAST_SEEN_AT, &now.to_string());
    Ok(())
}

/// Undo the most recent `next` jump: switch back to the pane we came
/// from. Marking is no longer part of this — use `mark` for that.
fn cmd_back() -> io::Result<()> {
    let from = tmux::run_tmux(&["show", "-gv", JUMP_FROM])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if from.is_empty() {
        let _ = tmux::run_tmux(&["display-message", "Nothing to go back to"]);
        return Ok(());
    }
    tmux::select_pane(&from);
    let _ = tmux::run_tmux(&["set", "-gu", JUMP_FROM]);
    let _ = tmux::run_tmux(&["set", "-gu", JUMP_TO]);
    Ok(())
}

/// Toggle `@dashboard_marked_unread_at` on the currently active pane.
/// Bound to `prefix + m`, mirrors the dashboard's `m` key.
fn cmd_mark() -> io::Result<()> {
    let pane = tmux::run_tmux(&["display-message", "-p", "#{pane_id}"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if pane.is_empty() {
        return Ok(());
    }
    let existing = tmux::get_pane_option_value(&pane, tmux::PANE_MARKED_UNREAD_AT);
    if existing.is_empty() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        tmux::set_pane_option(&pane, tmux::PANE_MARKED_UNREAD_AT, &now.to_string());
        let _ = tmux::run_tmux(&["display-message", "Marked unread"]);
    } else {
        tmux::unset_pane_option(&pane, tmux::PANE_MARKED_UNREAD_AT);
        let _ = tmux::run_tmux(&["display-message", "Unmarked"]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_pane_id;

    #[test]
    fn pane_id_shape() {
        assert!(is_pane_id("%0"));
        assert!(is_pane_id("%208"));
        assert!(!is_pane_id("%"));
        assert!(!is_pane_id("208"));
        assert!(!is_pane_id("%2a8"));
        assert!(!is_pane_id("+more"));
        assert!(!is_pane_id(""));
    }
}
