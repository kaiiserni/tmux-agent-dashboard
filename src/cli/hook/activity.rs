//! Activity-log writer. Appends entries shaped as `HH:MM|tool|label` to
//! `/tmp/tmux-agent-activity<pane>.log` and trims the file once it grows
//! past 210 lines.

use crate::time::now_epoch_secs;
use crate::tmux;
use crate::tool_name::CanonicalTool;

use super::context::pane_writes_allowed;
use super::label::extract_tool_label;
use crate::cli::{local_time_hhmm, sanitize_tmux_value, set_status};

pub(in crate::cli::hook) fn write_activity_entry(pane: &str, tool_name: &str, label: &str) {
    let log_path = crate::activity::log_file_path(pane);
    let label = sanitize_tmux_value(label);
    let timestamp = local_time_hhmm();
    let line = format!("{}|{}|{}\n", timestamp, tool_name, label);

    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        let _ = f.write_all(line.as_bytes());
    }

    trim_log_file(&log_path, 200, 210);
}

fn trim_log_file(path: &std::path::Path, keep: usize, threshold: usize) {
    if let Ok(content) = std::fs::read_to_string(path) {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() > threshold {
            let start = lines.len() - keep;
            let _ = std::fs::write(path, lines[start..].join("\n") + "\n");
        }
    }
}

pub(in crate::cli::hook) fn handle_activity_log(
    pane: &str,
    tool_name: &str,
    tool_input: &serde_json::Value,
    tool_response: &serde_json::Value,
) -> i32 {
    let label = extract_tool_label(tool_name, tool_input, tool_response);
    if is_background_bash(tool_name, tool_input) {
        let stored = if label.is_empty() {
            tmux::BG_CMD_PLACEHOLDER
        } else {
            label.as_str()
        };
        tmux::set_pane_option(pane, tmux::PANE_BG_CMD, &sanitize_tmux_value(stored));
    }

    let current_status = tmux::get_pane_option_value(pane, tmux::PANE_STATUS);
    if current_status != "running" && !current_status.is_empty() {
        set_status(pane, "running");
        if current_status == "waiting" {
            tmux::unset_pane_option(pane, tmux::PANE_ATTENTION);
            tmux::unset_pane_option(pane, tmux::PANE_WAIT_REASON);
        }
        let existing_started = tmux::get_pane_option_value(pane, tmux::PANE_STARTED_AT);
        if existing_started.is_empty() {
            tmux::set_pane_option(pane, tmux::PANE_STARTED_AT, &now_epoch_secs().to_string());
        }
    }

    // Plan-mode tool transitions flip the parent badge — but only when no
    // subagent is masking the parent's identity.
    if pane_writes_allowed(pane) {
        match tool_name {
            "EnterPlanMode" => {
                tmux::set_pane_option(pane, tmux::PANE_PERMISSION_MODE, "plan");
            }
            "ExitPlanMode" => {
                tmux::set_pane_option(pane, tmux::PANE_PERMISSION_MODE, "default");
            }
            _ => {}
        }
    }

    write_activity_entry(pane, tool_name, &label);
    0
}

fn is_background_bash(tool_name: &str, tool_input: &serde_json::Value) -> bool {
    tool_name == CanonicalTool::Bash.as_str()
        && ["run_in_background", "runInBackground"]
            .iter()
            .any(|key| tool_input.get(key).and_then(|v| v.as_bool()) == Some(true))
}
