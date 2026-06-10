#!/usr/bin/env bash
# tmux-agent-dashboard — popup dashboard for Claude / Codex agents.
# Depends on tmux-agent-sidebar (hooks write the pane state we consume).

PLUGIN_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Resolve our own binary. Order: pre-built bin/, local cargo build, then $PATH.
BIN=""
for candidate in \
    "$PLUGIN_DIR/bin/tmux-agent-dashboard" \
    "$PLUGIN_DIR/target/release/tmux-agent-dashboard"; do
    if [[ -x "$candidate" ]]; then
        BIN="$candidate"
        break
    fi
done

if [[ -z "$BIN" ]] && command -v tmux-agent-dashboard &>/dev/null; then
    BIN="$(command -v tmux-agent-dashboard)"
fi

if [[ -z "$BIN" ]]; then
    tmux display-message "tmux-agent-dashboard: binary not found; run 'cargo build --release' in $PLUGIN_DIR"
    exit 0
fi

# Sanity check: tmux-agent-sidebar must be installed since we read the
# pane options + activity logs it writes.
if [[ -z "$(tmux show -gv @agent_sidebar_bin 2>/dev/null)" ]] \
   && [[ ! -x "$HOME/.tmux/plugins/tmux-agent-sidebar/target/release/tmux-agent-sidebar" ]]; then
    tmux display-message "tmux-agent-dashboard: tmux-agent-sidebar plugin not detected — install it first"
fi

tmux set -g @dashboard_bin "$BIN"

tmux source-file "$PLUGIN_DIR/agent-dashboard.conf"

# Notification daemon: when @dashboard_notify_webhook is set, run the
# poller in the background. Guard against duplicate instances via a
# pidfile + liveness check so re-sourcing the plugin doesn't stack daemons.
if [[ -n "$(tmux show -gv @dashboard_notify_webhook 2>/dev/null)" ]]; then
    PIDFILE="/tmp/tmux-agent-dashboard-notify.pid"
    RUNNING=0
    if [[ -f "$PIDFILE" ]] && kill -0 "$(cat "$PIDFILE" 2>/dev/null)" 2>/dev/null; then
        RUNNING=1
    fi
    if [[ "$RUNNING" -eq 0 ]]; then
        tmux run-shell -b "'$BIN' notify-daemon & echo \$! > '$PIDFILE'"
    fi
fi
