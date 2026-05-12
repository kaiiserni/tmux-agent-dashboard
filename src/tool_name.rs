//! Canonical PascalCase tool name vocabulary shared by adapters and the
//! activity-log label extractor. Keeps OpenCode's lowercase tool ids and
//! Claude's/Codex's PascalCase aligned to a single namespace.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalTool {
    Bash,
    Read,
    Edit,
    Write,
    NotebookEdit,
    PowerShell,
    Monitor,
    PushNotification,
    Glob,
    Grep,
    WebFetch,
    WebSearch,
    ToolSearch,
    Skill,
    SendMessage,
    TeamCreate,
    Lsp,
    CronCreate,
    CronDelete,
    EnterWorktree,
    ExitWorktree,
    Agent,
    TaskCreate,
    TaskUpdate,
    TaskGet,
    TaskStop,
    TaskOutput,
    AskUserQuestion,
    TodoWrite,
}

impl CanonicalTool {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bash => "Bash",
            Self::Read => "Read",
            Self::Edit => "Edit",
            Self::Write => "Write",
            Self::NotebookEdit => "NotebookEdit",
            Self::PowerShell => "PowerShell",
            Self::Monitor => "Monitor",
            Self::PushNotification => "PushNotification",
            Self::Glob => "Glob",
            Self::Grep => "Grep",
            Self::WebFetch => "WebFetch",
            Self::WebSearch => "WebSearch",
            Self::ToolSearch => "ToolSearch",
            Self::Skill => "Skill",
            Self::SendMessage => "SendMessage",
            Self::TeamCreate => "TeamCreate",
            Self::Lsp => "LSP",
            Self::CronCreate => "CronCreate",
            Self::CronDelete => "CronDelete",
            Self::EnterWorktree => "EnterWorktree",
            Self::ExitWorktree => "ExitWorktree",
            Self::Agent => "Agent",
            Self::TaskCreate => "TaskCreate",
            Self::TaskUpdate => "TaskUpdate",
            Self::TaskGet => "TaskGet",
            Self::TaskStop => "TaskStop",
            Self::TaskOutput => "TaskOutput",
            Self::AskUserQuestion => "AskUserQuestion",
            Self::TodoWrite => "TodoWrite",
        }
    }
}
