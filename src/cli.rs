//! CLI subcommand dispatch for the `hook` family. The dashboard's other
//! subcommands (`seen`, `next`, `back`, `status-line`) stay in `main.rs`;
//! only the hook port lives here.

pub mod hook;

use std::io::Read;

use crate::tmux;

pub(crate) fn read_stdin_json() -> serde_json::Value {
    let is_tty = unsafe { libc::isatty(libc::STDIN_FILENO) != 0 };
    if is_tty {
        return serde_json::Value::Null;
    }
    let mut buf = String::new();
    let _ = std::io::stdin().read_to_string(&mut buf);
    serde_json::from_str(&buf).unwrap_or(serde_json::Value::Null)
}

pub(crate) fn tmux_pane() -> String {
    std::env::var("TMUX_PANE").unwrap_or_default()
}

pub(crate) fn local_time_hhmm() -> String {
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
    }
}

pub(crate) fn set_status(pane: &str, status: &str) {
    if status == "clear" {
        tmux::unset_pane_option(pane, tmux::PANE_STATUS);
        tmux::unset_pane_option(pane, tmux::PANE_ATTENTION);
    } else {
        tmux::set_pane_option(pane, tmux::PANE_STATUS, status);
        match status {
            "running" | "idle" => {
                tmux::unset_pane_option(pane, tmux::PANE_ATTENTION);
            }
            _ => {}
        }
    }
}

pub(crate) fn set_attention(pane: &str, state: &str) {
    if state == "clear" {
        tmux::unset_pane_option(pane, tmux::PANE_ATTENTION);
    } else {
        tmux::set_pane_option(pane, tmux::PANE_ATTENTION, state);
    }
}

/// tmux pane options use `|` as a field separator and `\n` as a record
/// terminator; both must be flattened to spaces before storage.
pub(crate) fn sanitize_tmux_value(s: &str) -> String {
    s.replace(['\n', '|'], " ")
}
