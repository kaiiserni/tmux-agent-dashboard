//! `notify-daemon` subcommand. Polls tmux agent panes (the same query the
//! TUI uses) and posts a Google Chat webhook message when a pane
//! transitions into an attention-worthy state.
//!
//! The trigger mirrors the dashboard's `needs_attention` semantics
//! (`@pane_attention` set, or status Waiting / Error). A per-pane cooldown
//! suppresses flapping. The webhook URL is read from the tmux global
//! option `@dashboard_notify_webhook` (fallback env
//! `DASHBOARD_NOTIFY_WEBHOOK`) and is never logged.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use crate::activity::read_activity_log;
use crate::tmux::{self, PaneInfo, PaneStatus};

const POLL_INTERVAL: Duration = Duration::from_secs(5);
const COOLDOWN: Duration = Duration::from_secs(60);
const WEBHOOK_OPTION: &str = "@dashboard_notify_webhook";
const WEBHOOK_ENV: &str = "DASHBOARD_NOTIFY_WEBHOOK";
const LOG_PATH: &str = "/tmp/tmux-agent-dashboard-notify.log";

/// Same rule as `ui::dashboard::needs_attention`: an explicit attention
/// flag, or a Waiting / Error status.
fn needs_attention(status: &PaneStatus, attention: bool) -> bool {
    attention || matches!(status, PaneStatus::Waiting | PaneStatus::Error)
}

/// Per-pane tracking: the last attention state we observed and when we
/// last sent a notification for it.
struct PaneTrack {
    attention: bool,
    last_notified: Option<Instant>,
}

/// Decide whether a transition warrants a notification, mutating the
/// tracking map. Pure (no tmux / IO) so it can be unit-tested: a
/// notification fires only on a false→true edge that is not still within
/// the cooldown of a previous notification for the same pane.
fn should_notify(
    track: &mut HashMap<String, PaneTrack>,
    pane_id: &str,
    attention_now: bool,
    now: Instant,
    cooldown: Duration,
) -> bool {
    let entry = track.entry(pane_id.to_string()).or_insert(PaneTrack {
        attention: false,
        last_notified: None,
    });

    let was = entry.attention;
    entry.attention = attention_now;

    if attention_now && !was {
        let cooling = entry
            .last_notified
            .is_some_and(|t| now.duration_since(t) < cooldown);
        if !cooling {
            entry.last_notified = Some(now);
            return true;
        }
    }
    false
}

fn webhook_url() -> Option<String> {
    if let Some(url) = tmux::get_option(WEBHOOK_OPTION) {
        return Some(url);
    }
    std::env::var(WEBHOOK_ENV).ok().filter(|s| !s.is_empty())
}

fn log_line(msg: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(LOG_PATH) {
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "{ts} {msg}");
    }
}

/// Best friendly label for a pane: the user-set `@pane_name`, else a
/// non-auto window name, else empty. Mirrors `pending::friendly_pane_label`.
fn friendly_pane_label(pane: &PaneInfo) -> String {
    if !pane.pane_name.is_empty() {
        return pane.pane_name.clone();
    }
    if !pane.auto_rename && !pane.window_name.is_empty() {
        return pane.window_name.clone();
    }
    String::new()
}

/// `<session>:<window>.<pane>` style technical locator for the message.
fn pane_locator(pane: &PaneInfo) -> String {
    let session = if pane.tmux_session_name.is_empty() {
        "?"
    } else {
        &pane.tmux_session_name
    };
    let window = if pane.window_name.is_empty() {
        pane.pane_id.as_str()
    } else {
        pane.window_name.as_str()
    };
    format!("{session}:{window}")
}

fn status_word(pane: &PaneInfo) -> &'static str {
    if pane.attention {
        "attention"
    } else {
        match pane.status {
            PaneStatus::Error => "error",
            PaneStatus::Waiting => "waiting",
            _ => "attention",
        }
    }
}

/// One context line: the pending wait reason if present, else the latest
/// activity-log label for the pane.
fn context_line(pane: &PaneInfo) -> String {
    if !pane.wait_reason.is_empty() {
        return pane.wait_reason.clone();
    }
    read_activity_log(&pane.pane_id, 1)
        .into_iter()
        .next()
        .map(|e| e.label)
        .filter(|l| !l.is_empty())
        .unwrap_or_default()
}

fn build_message(pane: &PaneInfo) -> String {
    let friendly = friendly_pane_label(pane);
    let name = if friendly.is_empty() {
        pane_locator(pane)
    } else {
        format!("{friendly} ({})", pane_locator(pane))
    };
    let ctx = context_line(pane);
    let mut msg = format!("\u{26a0} {name} \u{2014} {}", status_word(pane));
    if !ctx.is_empty() {
        msg.push_str(" \u{b7} ");
        msg.push_str(&ctx);
    }
    msg
}

/// POST `{"text": ...}` to the webhook via curl. Failures are logged, not
/// fatal. The URL is never written to the log.
fn post_webhook(url: &str, text: &str) {
    let payload = serde_json::json!({ "text": text }).to_string();
    let result = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "10",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "-d",
            &payload,
            url,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output();

    match result {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            eprintln!("notify: webhook POST failed: {}", err.trim());
            log_line(&format!("webhook POST failed (exit {:?})", o.status.code()));
        }
        Err(e) => {
            eprintln!("notify: failed to run curl: {e}");
            log_line("webhook POST failed (curl not runnable)");
        }
    }
}

pub fn cmd_notify_daemon() -> i32 {
    let url = match webhook_url() {
        Some(u) => u,
        None => {
            eprintln!(
                "notify-daemon: no webhook configured. Set tmux option {WEBHOOK_OPTION} \
                 or env {WEBHOOK_ENV}."
            );
            return 1;
        }
    };

    log_line("daemon started");

    let mut track: HashMap<String, PaneTrack> = HashMap::new();
    let mut first_pass = true;

    loop {
        let sessions = tmux::query_sessions();
        let now = Instant::now();
        let mut seen: Vec<String> = Vec::new();

        for session in &sessions {
            for window in &session.windows {
                for pane in &window.panes {
                    seen.push(pane.pane_id.clone());
                    let attention = needs_attention(&pane.status, pane.attention);

                    // On the first pass, seed state without firing so a
                    // pane already waiting at startup doesn't notify.
                    if first_pass {
                        track.insert(
                            pane.pane_id.clone(),
                            PaneTrack {
                                attention,
                                last_notified: None,
                            },
                        );
                        continue;
                    }

                    if should_notify(&mut track, &pane.pane_id, attention, now, COOLDOWN) {
                        let msg = build_message(pane);
                        log_line(&format!(
                            "notify {} [{}]",
                            pane.pane_id,
                            status_word(pane)
                        ));
                        post_webhook(&url, &msg);
                    }
                }
            }
        }

        // Drop tracking for panes that no longer exist so the map doesn't
        // grow unbounded and a recycled pane id starts fresh.
        track.retain(|id, _| seen.contains(id));

        first_pass = false;
        std::thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(status: PaneStatus, attention: bool) -> PaneInfo {
        PaneInfo {
            pane_id: "%1".into(),
            pane_active: false,
            status,
            attention,
            last_seen_at: None,
            marked_unread_at: None,
            agent: tmux::AgentType::Claude,
            path: String::new(),
            current_command: String::new(),
            prompt: String::new(),
            started_at: None,
            wait_reason: String::new(),
            permission_mode: tmux::PermissionMode::Default,
            worktree: tmux::WorktreeMetadata::default(),
            session_id: None,
            session_name: String::new(),
            tmux_session_name: "cc-demo".into(),
            pane_name: String::new(),
            window_name: String::new(),
            auto_rename: false,
            bg_cmd: String::new(),
            summary: String::new(),
        }
    }

    #[test]
    fn attention_semantics() {
        assert!(needs_attention(&PaneStatus::Waiting, false));
        assert!(needs_attention(&PaneStatus::Error, false));
        assert!(needs_attention(&PaneStatus::Idle, true));
        assert!(!needs_attention(&PaneStatus::Idle, false));
        assert!(!needs_attention(&PaneStatus::Running, false));
    }

    #[test]
    fn fires_only_on_rising_edge_with_cooldown() {
        let mut track = HashMap::new();
        let cooldown = Duration::from_secs(60);
        let t0 = Instant::now();

        // idle -> nothing
        assert!(!should_notify(&mut track, "%1", false, t0, cooldown));
        // idle -> attention: fire
        assert!(should_notify(&mut track, "%1", true, t0, cooldown));
        // still attention: no repeat
        assert!(!should_notify(&mut track, "%1", true, t0, cooldown));
        // drops to idle, then back to attention within cooldown: suppressed
        assert!(!should_notify(&mut track, "%1", false, t0, cooldown));
        let t_soon = t0 + Duration::from_secs(30);
        assert!(!should_notify(&mut track, "%1", true, t_soon, cooldown));
        // drops again, comes back after cooldown elapsed: fires
        assert!(!should_notify(&mut track, "%1", false, t_soon, cooldown));
        let t_late = t0 + Duration::from_secs(120);
        assert!(should_notify(&mut track, "%1", true, t_late, cooldown));
    }

    #[test]
    fn message_uses_friendly_name_when_present() {
        let mut p = pane(PaneStatus::Waiting, false);
        p.pane_name = "auth refactor".into();
        let msg = build_message(&p);
        assert!(msg.contains("auth refactor"));
        assert!(msg.contains("cc-demo"));
        assert!(msg.contains("waiting"));
    }
}
