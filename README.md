# tmux-agent-dashboard

A tmux popup dashboard for Claude Code / Codex / OpenCode agents. Shows
aggregate counters, attention / waiting / responded lists on the left,
running / idle lists on the right, and a global recent-activity feed at
the bottom. Also includes a tiles grid view for a quick visual overview.

## Dependency

Requires [`tmux-agent-sidebar`](https://github.com/hiroppy/tmux-agent-sidebar)
installed and running. The dashboard is a **pure consumer** — every
pane metadata value comes from the tmux options and activity logs that
the sidebar plugin's hooks write.

## Install

### TPM (build from source)

```tmux
set -g @plugin 'kaiiserni/tmux-agent-dashboard'
```

After `prefix + I`, run `cargo build --release` inside the plugin
directory so the binary lands in `target/release/`.

### Manual

```bash
git clone https://github.com/kaiiserni/tmux-agent-dashboard.git \
  ~/projects/tmux-agent-dashboard
cd ~/projects/tmux-agent-dashboard
cargo build --release
```

In `~/.tmux.conf`:

```tmux
run-shell '~/projects/tmux-agent-dashboard/tmux-agent-dashboard.tmux'
```

## Keybindings

| Key | Action |
|---|---|
| `prefix + ñ` | Open dashboard popup |
| `Tab` | Switch between Summary and Tiles |
| `s` | Toggle attention-first sort |
| `p` | Toggle redact mode (hide ages/timestamps + mask text, for screenshots) |
| `j` / `k` / `↓` / `↑` | Move row selection (auto-scrolls across lists) |
| `h` / `l` / `←` / `→` | Jump between columns (Summary view) |
| `g` / `G` | Jump to first / last row (Summary) |
| `PageDown` / `PageUp` | Scroll the active list |
| `Enter` or click | Activate pane and close popup |
| `Space` | Jump to the top pane that needs attention and close popup |
| `L` | Jump back to the origin pane (toggles back on a second press) |
| Mouse wheel | Scroll the list under the cursor |
| `q` / `Esc` | Close popup |

The header items are clickable too: clicking one runs the matching key (and clicking the tab label or `Tab: switch` flips the view).

Each tile in the Tiles view also shows a context-preview line: the pane's
most recent activity-log entry (tool, label, and age). Hidden in redact mode.

## Configuration

```tmux
set -g @dashboard_key    'ñ'      # default
set -g @dashboard_width  '90%'    # default
set -g @dashboard_height '85%'    # default
```

The sidebar's `@sidebar_color_*` and `@sidebar_icon_*` options are
reused so visual customisation carries over.

## Notifications

A background daemon can push a Google Chat message whenever a pane
transitions into an attention-worthy state (an explicit attention flag,
or status Waiting / Error) — handy when you work remotely over
mosh/ssh and don't want to keep opening the popup.

1. In Google Chat, open the target space → space name → **Apps &
   integrations** → **Webhooks** → **Add webhook**, and copy the URL.
2. Point the dashboard at it:

   ```tmux
   set -g @dashboard_notify_webhook 'https://chat.googleapis.com/v1/spaces/AAAA/messages?key=...&token=...'
   ```

   (Or export `DASHBOARD_NOTIFY_WEBHOOK` in the environment as a
   fallback.)

When `@dashboard_notify_webhook` is set, the plugin auto-starts the
daemon on load (guarded against duplicate instances via
`/tmp/tmux-agent-dashboard-notify.pid`). It polls every ~5s and applies
a 60s per-pane cooldown so a flapping pane won't spam the space.

Run it standalone:

```bash
tmux-agent-dashboard notify-daemon
```

Messages look like `⚠ auth refactor (cc-helion:editor) — waiting · …`,
using the friendly pane name when set. Activity is appended to
`/tmp/tmux-agent-dashboard-notify.log` (start/stop and sent
notifications; the webhook URL is never logged). With no webhook
configured the daemon exits with a message on stderr.

## Architecture

- `prefix + ñ` triggers `tmux display-popup -E "<bin>"` which runs the
  Rust TUI
- The TUI reads `tmux list-panes -F` for pane state and
  `/tmp/tmux-agent-activity*.log` for the recent activity feed
- `seen <pane_id>` (bound to `after-select-pane`) writes
  `@pane_last_seen_at` so the dashboard can derive when you last
  looked at a pane — the basis for the Responded heuristic

## Build from source

macOS (arm64):

```bash
cd ~/projects/tmux-agent-dashboard
cargo build --release
cp target/release/tmux-agent-dashboard bin/
codesign --force --sign - bin/tmux-agent-dashboard
```

Linux: `cargo build --release` is enough; no codesigning needed.

## License

MIT
