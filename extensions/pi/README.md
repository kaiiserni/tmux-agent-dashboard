# pi-tmux-agent-dashboard

Pi extension that forwards Pi's `session_start`, `before_agent_start`,
`tool_call`, `tool_result`, and `agent_end` events to the
`tmux-agent-dashboard` hook CLI so Pi panes show up in the dashboard
next to Claude / Codex / OpenCode / Antigravity.

## Install

```sh
pi install /path/to/tmux-agent-dashboard/extensions/pi
```

(Add `-l` to install locally to the current project instead of globally.)

## Binary resolution

The extension shells out to `tmux-agent-dashboard hook pi <event>`. It looks
for the binary in this order:

1. `$TMUX_AGENT_DASHBOARD_BIN` (if set and pointing to an executable)
2. `~/.tmux/plugins/tmux-agent-dashboard/bin/tmux-agent-dashboard`
3. `~/.tmux/plugins/tmux-agent-dashboard/target/release/tmux-agent-dashboard`
4. `~/projects/tmux-agent-dashboard/bin/tmux-agent-dashboard`
5. `~/projects/tmux-agent-dashboard/target/release/tmux-agent-dashboard`
6. `tmux-agent-dashboard` on `$PATH`

Override with `export TMUX_AGENT_DASHBOARD_BIN=/abs/path/to/binary`.

## Event mapping

| Pi event | dashboard event | effect |
|---|---|---|
| `session_start` | `session-start` | pane claimed, status `running` |
| `before_agent_start` | `user-prompt-submit` | prompt stored |
| `tool_call` | `activity-log` | tool name + input |
| `tool_result` | `activity-log` | adds response/error |
| `agent_end` | `stop` | status `idle` |
