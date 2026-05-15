//! Liveness check for the background shell recorded in `@pane_bg_cmd`.
//!
//! `@pane_bg_cmd` is sticky: it is set when a `run_in_background` Bash
//! launches and only cleared on SessionStart. So a `Stop` long after the
//! shell already exited still classified the pane as `background`
//! forever. This probes whether a process matching the recorded command
//! is actually still running, so callers can downgrade a stale
//! `background` pane back to `idle`.

use crate::tmux::BG_CMD_PLACEHOLDER;

/// Longest whitespace-delimited token in `bg_cmd`, used as the `pgrep -f`
/// needle. A single token sidesteps the fact that `@pane_bg_cmd` had its
/// `|` and newlines flattened to spaces (so a multi-word substring may
/// not match the real argv), while still being specific enough in
/// practice (`monitor-poll.sh`, `/tmp/watch_sig13.sh`, …).
fn needle(bg_cmd: &str) -> Option<&str> {
    bg_cmd
        .split_whitespace()
        .filter(|t| t.len() >= 8)
        .max_by_key(|t| t.len())
}

/// `true` when the recorded background shell still has a live process.
///
/// Conservative: an empty value, the unlabeled placeholder, or a command
/// with no usable needle returns `true` — never hide a background pane we
/// cannot positively prove dead.
pub fn bg_shell_alive(bg_cmd: &str) -> bool {
    let bg_cmd = bg_cmd.trim();
    if bg_cmd.is_empty() || bg_cmd == BG_CMD_PLACEHOLDER {
        return true;
    }
    let Some(needle) = needle(bg_cmd) else {
        return true;
    };
    match std::process::Command::new("pgrep")
        .arg("-f")
        .arg("--")
        .arg(needle)
        .output()
    {
        Ok(out) => !out.stdout.is_empty(),
        // pgrep missing / unexpected failure: assume alive.
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needle_picks_longest_token() {
        assert_eq!(
            needle("bash /tmp/watch_sig13.sh foo"),
            Some("/tmp/watch_sig13.sh")
        );
        assert_eq!(
            needle("monitor-poll.sh cc-x:2.0 crypto"),
            Some("monitor-poll.sh")
        );
    }

    #[test]
    fn needle_none_when_all_tokens_short() {
        assert_eq!(needle("a bc def ghij"), None);
    }

    #[test]
    fn unverifiable_values_count_as_alive() {
        assert!(bg_shell_alive(""));
        assert!(bg_shell_alive("   "));
        assert!(bg_shell_alive(BG_CMD_PLACEHOLDER));
        assert!(bg_shell_alive("short toks"));
    }
}
