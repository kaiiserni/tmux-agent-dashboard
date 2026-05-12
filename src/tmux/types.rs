pub const CLAUDE_AGENT: &str = "claude";
pub const CODEX_AGENT: &str = "codex";
pub const OPENCODE_AGENT: &str = "opencode";

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
    pub agent: AgentType,
    pub path: String,
    pub current_command: String,
    pub prompt: String,
    pub started_at: Option<u64>,
    pub wait_reason: String,
    pub permission_mode: PermissionMode,
    pub worktree: WorktreeMetadata,
    pub session_id: Option<String>,
    pub session_name: String,
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
