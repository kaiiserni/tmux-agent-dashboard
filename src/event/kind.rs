/// Discriminant of `AgentEvent`. Single compile-time source of truth for
/// the mapping between internal events and their external CLI names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentEventKind {
    SessionStart,
    SessionEnd,
    UserPromptSubmit,
    Notification,
    PlanReview,
    PermissionRequest,
    Stop,
    StopFailure,
    PermissionDenied,
    CwdChanged,
    SubagentStart,
    SubagentStop,
    ActivityLog,
    TaskCreated,
    TaskCompleted,
    TeammateIdle,
    WorktreeCreate,
    WorktreeRemove,
}

impl AgentEventKind {
    pub const ALL: &'static [Self] = &[
        Self::SessionStart,
        Self::SessionEnd,
        Self::UserPromptSubmit,
        Self::Notification,
        Self::PlanReview,
        Self::PermissionRequest,
        Self::Stop,
        Self::StopFailure,
        Self::PermissionDenied,
        Self::CwdChanged,
        Self::SubagentStart,
        Self::SubagentStop,
        Self::ActivityLog,
        Self::TaskCreated,
        Self::TaskCompleted,
        Self::TeammateIdle,
        Self::WorktreeCreate,
        Self::WorktreeRemove,
    ];

    pub const fn external_name(self) -> &'static str {
        match self {
            Self::SessionStart => "session-start",
            Self::SessionEnd => "session-end",
            Self::UserPromptSubmit => "user-prompt-submit",
            Self::Notification => "notification",
            Self::PlanReview => "plan-review",
            Self::PermissionRequest => "permission-request",
            Self::Stop => "stop",
            Self::StopFailure => "stop-failure",
            Self::PermissionDenied => "permission-denied",
            Self::CwdChanged => "cwd-changed",
            Self::SubagentStart => "subagent-start",
            Self::SubagentStop => "subagent-stop",
            Self::ActivityLog => "activity-log",
            Self::TaskCreated => "task-created",
            Self::TaskCompleted => "task-completed",
            Self::TeammateIdle => "teammate-idle",
            Self::WorktreeCreate => "worktree-create",
            Self::WorktreeRemove => "worktree-remove",
        }
    }

    pub fn from_external_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|k| k.external_name() == name)
    }
}
