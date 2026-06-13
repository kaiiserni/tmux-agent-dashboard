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

## Overview tab

The third tab ("Overview") renders a prioritised, per-project briefing from an
external JSON file. The dashboard is a **pure consumer** with no built-in
producer — point it at a file and it polls that file on every refresh:

```tmux
set -g @dashboard_overview_file '~/.local/state/agent-overview/overview.json'
```

Unset → the tab shows a setup hint. Any tool can produce the file; the
[`agent-overview`](https://github.com/kaiiserni/agent-overview) job is one.
Rows are clickable (mouse) and navigable (`j`/`k` select, `Enter` jumps to the
pane → window → session; `Ctrl+D`/`Ctrl+U` half-page scroll). Clicking falls
back to the session/window when the pane id is gone.

### File schema

UTF-8 JSON, rewritten in place each update. `updated_at` is epoch seconds; all
strings are plain text.

```json
{
  "updated_at": 1781330413,
  "tldr": ["short bullets for the header"],
  "projects": [
    {
      "name": "my-project",
      "cwd": "/abs/path/to/project",
      "attention": true,
      "doing": "what's happening right now",
      "needs_from_you": "blocker text, or empty string",
      "next_steps": ["suggestion 1", "suggestion 2"],
      "active_md": ["optional verbatim notes"],
      "panes": [
        {
          "pane_id": "%123",
          "target": "session:window.pane",
          "agent": "claude",
          "status": "running",
          "age_minutes": 12,
          "summary": "one-line of what this pane is doing"
        }
      ]
    }
  ],
  "idle": [
    { "pane_id": "%99", "target": "session:window.pane", "project": "name", "task": "what it was doing" }
  ]
}
```

Field notes: `attention: true` sorts a project to the top and flags it. `pane_id`
(`%N`) is the tmux pane id used for click/Enter navigation; `target` is the
`session:window.pane` fallback. Unknown extra fields are ignored, so producers
can add their own.

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

### Pane state contract (`@pane_*`)

The dashboard is a consumer of per-pane tmux options written by the
[`tmux-agent-sidebar`](https://github.com/hiroppy/tmux-agent-sidebar) hooks
(except `@pane_last_seen_at`/`@dashboard_marked_unread_at`, which this binary
writes itself, and `@pane_summary`, which an external producer like
`agent-overview` writes). This is the shared contract; any reader (this TUI,
the `agent-overview` job) reads the same options:

| Option | Meaning |
|---|---|
| `@pane_agent` | agent kind (`claude`/`codex`/`opencode`/`antigravity`/`pi`) — a pane is only an "agent pane" when set |
| `@pane_status` | `running` / `waiting` / `idle` / `background` / `error` |
| `@pane_attention` | set when the agent flagged it needs the developer |
| `@pane_cwd` | working directory |
| `@pane_prompt` (+ `@pane_prompt_source`) | the task prompt the agent is running |
| `@pane_wait_reason` | what a `waiting` pane is blocked on |
| `@pane_permission_mode` | Claude permission mode (`default`/`plan`/`acceptEdits`/`auto`/…) |
| `@pane_started_at` / `@pane_last_seen_at` | epoch seconds: pane start / last user focus |
| `@pane_session_id` | Claude session id (resolves a friendly session name) |
| `@pane_worktree_name` / `@pane_worktree_branch` | git worktree context |
| `@pane_summary` | one-line LLM summary of what the pane is doing now (external producer; may lag) |

Plus `/tmp/tmux-agent-activity_<N>.log` lines (`epoch|tool|label`) for the
recent-activity feed.

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
