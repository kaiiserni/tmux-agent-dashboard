use std::process::Command;

pub fn run_tmux(args: &[&str]) -> Option<String> {
    let output = Command::new("tmux").args(args).output().ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}

pub fn display_message(target: &str, format: &str) -> String {
    run_tmux(&["display-message", "-t", target, "-p", format])
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Switch the user's tmux client to the session/window/pane containing
/// `pane_id`. Invoked when the dashboard's `Enter` key activates a tile.
pub fn select_pane(pane_id: &str) {
    let session_id = display_message(pane_id, "#{session_id}");
    if !session_id.is_empty() {
        let _ = run_tmux(&["switch-client", "-t", &session_id]);
    }
    let window_id = display_message(pane_id, "#{window_id}");
    if !window_id.is_empty() {
        let _ = run_tmux(&["select-window", "-t", &window_id]);
    }
    let _ = run_tmux(&["select-pane", "-t", pane_id]);
}

pub fn set_pane_option(pane_id: &str, key: &str, value: &str) {
    let _ = run_tmux(&["set", "-t", pane_id, "-p", key, value]);
}

pub fn unset_pane_option(pane_id: &str, key: &str) {
    let _ = run_tmux(&["set", "-t", pane_id, "-pu", key]);
}

pub fn get_pane_option_value(pane_id: &str, key: &str) -> String {
    run_tmux(&["show", "-t", pane_id, "-pv", key])
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub fn get_all_global_options() -> std::collections::HashMap<String, String> {
    let out = run_tmux(&["show", "-g"]).unwrap_or_default();
    let mut map = std::collections::HashMap::new();
    for line in out.lines() {
        let mut parts = line.splitn(2, ' ');
        if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
            let v = v.trim().trim_matches('"').to_string();
            map.insert(k.to_string(), v);
        }
    }
    map
}
