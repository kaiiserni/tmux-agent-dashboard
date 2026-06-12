pub const CLAUDE_AGENT: &str = "claude";
pub const CODEX_AGENT: &str = "codex";
pub const OPENCODE_AGENT: &str = "opencode";
pub const ANTIGRAVITY_AGENT: &str = "antigravity";
pub const PI_AGENT: &str = "pi";

#[derive(Debug, Clone)]
pub struct PaneInfo {
    pub pane_id: String,
    pub pane_active: bool,
    pub status: PaneStatus,
    pub attention: bool,
    /// Epoch seconds at which the user last focused this pane (via tmux's
    /// `after-select-pane` hook → `seen` CLI subcommand). Compared against
    /// the activity log mtime to derive Responded state.
    pub last_seen_at: Option<u64>,
    /// Epoch seconds when the user marked the pane as unread (`prefix + N`
    /// or dashboard `m` toggle). Auto-cleared when the pane's status
    /// leaves Idle, gains attention, or new auto-Responded activity fires.
    pub marked_unread_at: Option<u64>,
    pub agent: AgentType,
    pub path: String,
    pub current_command: String,
    pub prompt: String,
    pub started_at: Option<u64>,
    pub wait_reason: String,
    pub permission_mode: PermissionMode,
    pub worktree: WorktreeMetadata,
    pub session_id: Option<String>,
    /// Claude session label resolved from `~/.claude/sessions/*.json`
    /// using `session_id`. Empty if no matching Claude session file.
    pub session_name: String,
    /// Tmux session this pane belongs to (e.g. `cc-helion-orbit`).
    /// Always populated.
    pub tmux_session_name: String,
    /// Friendly pane label set via Claude's `/rename` (stored in tmux as
    /// `@pane_name`). Empty when the user never renamed the pane.
    pub pane_name: String,
    /// Tmux window name (e.g. set via `prefix + ,` or `rename-window`).
    /// Always populated; meaningful as a friendly label only when
    /// `auto_rename == false`.
    pub window_name: String,
    /// `true` when tmux's `automatic-rename` option is on for the
    /// window — i.e. `window_name` is process-derived, not user-chosen.
    pub auto_rename: bool,
    /// Recorded `run_in_background` command (sticky per session). Used to
    /// probe whether a `Background` pane's shell is actually still alive.
    pub bg_cmd: String,
    /// One-line summary from `@pane_summary` (agent-overview job). Empty
    /// when the job hasn't run or the pane wasn't active during its pass.
    pub summary: String,
}

#[derive(Debug, Clone, Default)]
pub struct WorktreeMetadata {
    pub name: String,
    pub branch: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PaneStatus {
    Running,
    Background,
    Waiting,
    Idle,
    Error,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PermissionMode {
    Default,
    Plan,
    AcceptEdits,
    Auto,
    DontAsk,
    BypassPermissions,
    Defer,
}

impl PermissionMode {
    pub fn from_label(s: &str) -> Self {
        match s {
            "plan" => Self::Plan,
            "acceptEdits" => Self::AcceptEdits,
            "auto" => Self::Auto,
            "dontAsk" => Self::DontAsk,
            "bypassPermissions" => Self::BypassPermissions,
            "defer" => Self::Defer,
            _ => Self::Default,
        }
    }

    pub fn badge(&self) -> &str {
        match self {
            Self::Default => "",
            Self::Plan => "plan",
            Self::AcceptEdits => "edit",
            Self::Auto => "auto",
            Self::DontAsk => "dontAsk",
            Self::BypassPermissions => "!",
            Self::Defer => "defer",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentType {
    Claude,
    Codex,
    OpenCode,
    Antigravity,
    Pi,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub window_id: String,
    pub window_name: String,
    pub window_active: bool,
    pub panes: Vec<PaneInfo>,
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_name: String,
    pub windows: Vec<WindowInfo>,
}

impl AgentType {
    pub fn from_label(s: &str) -> Option<Self> {
        match s {
            CLAUDE_AGENT => Some(Self::Claude),
            CODEX_AGENT => Some(Self::Codex),
            OPENCODE_AGENT => Some(Self::OpenCode),
            ANTIGRAVITY_AGENT => Some(Self::Antigravity),
            PI_AGENT => Some(Self::Pi),
            _ => None,
        }
    }

    /// Single-character glyph used in dashboard rows / tiles so different
    /// agent vendors are visually distinguishable at a glance.
    pub fn glyph(&self) -> &'static str {
        match self {
            Self::Claude => "✦",
            Self::Codex => "◉",
            Self::OpenCode => "◇",
            Self::Antigravity => "▲",
            Self::Pi => "π",
            Self::Unknown => "·",
        }
    }
}

impl PaneStatus {
    pub fn from_label(s: &str) -> Self {
        match s {
            "running" => Self::Running,
            "background" => Self::Background,
            "waiting" | "notification" => Self::Waiting,
            "idle" => Self::Idle,
            "error" => Self::Error,
            _ => Self::Unknown,
        }
    }
}
