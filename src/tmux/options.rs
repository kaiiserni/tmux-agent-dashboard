use super::commands::run_tmux;

// ─── Pane-scoped option keys ─────────────────────────────────────────
//
// Single source of truth for the `@pane_*` tmux options the dashboard
// reads from each pane. Most of these are *written* by the
// `tmux-agent-sidebar` plugin's hook handlers — we just consume them.
// The one exception is `PANE_LAST_SEEN_AT`, which the dashboard's
// `seen` subcommand writes itself on focus change.

pub const PANE_AGENT: &str = "@pane_agent";
pub const PANE_NAME: &str = "@pane_name";
pub const PANE_ATTENTION: &str = "@pane_attention";
/// Epoch seconds at which the user last focused the pane (written by
/// the `seen` CLI subcommand, wired to tmux's `after-select-pane` hook).
/// The dashboard compares this against the activity log mtime to derive
/// the Responded set.
pub const PANE_LAST_SEEN_AT: &str = "@pane_last_seen_at";
/// Epoch seconds when the user explicitly marked the pane as unread
/// (via `prefix + N` or the dashboard's `m` toggle). Lives in the
/// `@dashboard_*` namespace so it survives the sidebar's
/// `clear_agent_pane_state` sweep.
pub const PANE_MARKED_UNREAD_AT: &str = "@dashboard_marked_unread_at";
pub const PANE_CWD: &str = "@pane_cwd";
pub const PANE_BG_CMD: &str = "@pane_bg_cmd";
pub const PANE_PERMISSION_MODE: &str = "@pane_permission_mode";
pub const PANE_PROMPT: &str = "@pane_prompt";
pub const PANE_PROMPT_SOURCE: &str = "@pane_prompt_source";
pub const PANE_ROLE: &str = "@pane_role";
pub const PANE_SESSION_ID: &str = "@pane_session_id";
pub const PANE_STARTED_AT: &str = "@pane_started_at";
pub const PANE_STATUS: &str = "@pane_status";
pub const PANE_SUBAGENTS: &str = "@pane_subagents";
pub const PANE_WAIT_REASON: &str = "@pane_wait_reason";
pub const PANE_WORKTREE_BRANCH: &str = "@pane_worktree_branch";
pub const PANE_WORKTREE_NAME: &str = "@pane_worktree_name";

/// Stored in `@pane_bg_cmd` when the background Bash invocation arrives
/// without a command string we can echo back to the user.
pub const BG_CMD_PLACEHOLDER: &str = "(background shell)";

// ─── Sidebar-compatible color / icon overrides ──────────────────────
//
// We accept the same `@sidebar_color_*` and `@sidebar_icon_*` options
// the sidebar plugin documents so any user customisation already in
// `.tmux.conf` carries over without changes.

pub const SIDEBAR_COLOR_ACCENT: &str = "@sidebar_color_accent";
pub const SIDEBAR_COLOR_BORDER: &str = "@sidebar_color_border";
pub const SIDEBAR_COLOR_ALL: &str = "@sidebar_color_all";
pub const SIDEBAR_COLOR_RUNNING: &str = "@sidebar_color_running";
pub const SIDEBAR_COLOR_WAITING: &str = "@sidebar_color_waiting";
pub const SIDEBAR_COLOR_IDLE: &str = "@sidebar_color_idle";
pub const SIDEBAR_COLOR_ERROR: &str = "@sidebar_color_error";
pub const SIDEBAR_COLOR_FILTER_INACTIVE: &str = "@sidebar_color_filter_inactive";
pub const SIDEBAR_COLOR_AGENT_CLAUDE: &str = "@sidebar_color_agent_claude";
pub const SIDEBAR_COLOR_AGENT_CODEX: &str = "@sidebar_color_agent_codex";
pub const SIDEBAR_COLOR_AGENT_OPENCODE: &str = "@sidebar_color_agent_opencode";
pub const SIDEBAR_COLOR_TEXT_ACTIVE: &str = "@sidebar_color_text_active";
pub const SIDEBAR_COLOR_TEXT_MUTED: &str = "@sidebar_color_text_muted";
pub const SIDEBAR_COLOR_TEXT_INACTIVE: &str = "@sidebar_color_text_inactive";
pub const SIDEBAR_COLOR_SESSION: &str = "@sidebar_color_session";
pub const SIDEBAR_COLOR_WAIT_REASON: &str = "@sidebar_color_wait_reason";
pub const SIDEBAR_COLOR_SELECTION: &str = "@sidebar_color_selection";
pub const SIDEBAR_COLOR_BRANCH: &str = "@sidebar_color_branch";
pub const SIDEBAR_COLOR_SECTION_TITLE: &str = "@sidebar_color_section_title";
pub const SIDEBAR_COLOR_ACTIVITY_TIMESTAMP: &str = "@sidebar_color_activity_timestamp";
pub const SIDEBAR_COLOR_RESPONSE_ARROW: &str = "@sidebar_color_response_arrow";
pub const SIDEBAR_COLOR_BADGE_DANGER: &str = "@sidebar_color_badge_danger";
pub const SIDEBAR_COLOR_BADGE_AUTO: &str = "@sidebar_color_badge_auto";
pub const SIDEBAR_COLOR_BADGE_PLAN: &str = "@sidebar_color_badge_plan";

pub const SIDEBAR_ICON_ALL: &str = "@sidebar_icon_all";
pub const SIDEBAR_ICON_RUNNING: &str = "@sidebar_icon_running";
pub const SIDEBAR_ICON_BACKGROUND: &str = "@sidebar_icon_background";
pub const SIDEBAR_ICON_WAITING: &str = "@sidebar_icon_waiting";
pub const SIDEBAR_ICON_IDLE: &str = "@sidebar_icon_idle";
pub const SIDEBAR_ICON_ERROR: &str = "@sidebar_icon_error";
pub const SIDEBAR_ICON_UNKNOWN: &str = "@sidebar_icon_unknown";

pub fn get_option(name: &str) -> Option<String> {
    run_tmux(&["show", "-gv", name])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
