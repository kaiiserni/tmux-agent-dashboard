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
| `j` / `k` / `↓` / `↑` | Move row selection (auto-scrolls across lists) |
| `h` / `l` / `←` / `→` | Jump between columns (Summary view) |
| `g` / `G` | Jump to first / last row (Summary) |
| `PageDown` / `PageUp` | Scroll the active list |
| `Enter` or click | Activate pane and close popup |
| Mouse wheel | Scroll the list under the cursor |
| `q` / `Esc` | Close popup |

## Configuration

```tmux
set -g @dashboard_key    'ñ'      # default
set -g @dashboard_width  '90%'    # default
set -g @dashboard_height '85%'    # default
```

The sidebar's `@sidebar_color_*` and `@sidebar_icon_*` options are
reused so visual customisation carries over.

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
