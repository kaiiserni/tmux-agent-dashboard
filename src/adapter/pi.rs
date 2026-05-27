//! Pi (pi.dev, `@earendil-works/pi-coding-agent`) adapter. Pi exposes a
//! TypeScript extension API with events like `session_start`, `input`,
//! `tool_call`, `tool_result`, `agent_end`. A companion TS extension
//! forwards those events to `tmux-agent-dashboard hook pi <event-name>`
//! over stdin with a JSON payload.
//!
//! Event names on the wire use kebab-case to match the convention the
//! rest of the dashboard uses (`session-start`, `user-prompt-submit`,
//! `activity-log`, `stop`, `stop-failure`, `notification`). The Pi
//! extension translates between Pi's native `session_start` and the
//! dashboard's `session-start`.

use serde_json::Value;

use crate::event::{AgentEvent, AgentEventKind, EventAdapter};
use crate::tmux::PI_AGENT;
use crate::tool_name::CanonicalTool;

use super::{HookRegistration, json_str, json_value_or_null, optional_str};

/// Map Pi's lower-case tool ids to the dashboard's canonical PascalCase
/// names so activity-log colors and label extraction work.
fn normalize_tool_name(raw: &str) -> String {
    let canonical = match raw.to_ascii_lowercase().as_str() {
        "bash" | "shell" => CanonicalTool::Bash,
        "read" | "view" => CanonicalTool::Read,
        "write" => CanonicalTool::Write,
        "edit" | "multiedit" | "str_replace" => CanonicalTool::Edit,
        "glob" => CanonicalTool::Glob,
        "grep" | "search" => CanonicalTool::Grep,
        "webfetch" | "fetch" => CanonicalTool::WebFetch,
        "websearch" => CanonicalTool::WebSearch,
        "task" | "agent" | "subagent" => CanonicalTool::Agent,
        "skill" => CanonicalTool::Skill,
        "lsp" => CanonicalTool::Lsp,
        "todowrite" | "todo" => CanonicalTool::TodoWrite,
        _ => return raw.to_string(),
    };
    canonical.as_str().to_string()
}

pub struct PiAdapter;

impl PiAdapter {
    pub const HOOK_REGISTRATIONS: &'static [HookRegistration] = &[
        HookRegistration {
            trigger: "session_start",
            matcher: None,
            kind: AgentEventKind::SessionStart,
        },
        HookRegistration {
            trigger: "input",
            matcher: None,
            kind: AgentEventKind::UserPromptSubmit,
        },
        HookRegistration {
            trigger: "tool_call",
            matcher: None,
            kind: AgentEventKind::ActivityLog,
        },
        HookRegistration {
            trigger: "tool_result",
            matcher: None,
            kind: AgentEventKind::ActivityLog,
        },
        HookRegistration {
            trigger: "agent_end",
            matcher: None,
            kind: AgentEventKind::Stop,
        },
        HookRegistration {
            trigger: "session_shutdown",
            matcher: None,
            kind: AgentEventKind::StopFailure,
        },
    ];
}

impl EventAdapter for PiAdapter {
    fn parse(&self, event_name: &str, input: &Value) -> Option<AgentEvent> {
        match event_name {
            "session-start" => Some(AgentEvent::SessionStart {
                agent: PI_AGENT.into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: String::new(),
                source: json_str(input, "source").into(),
                worktree: None,
                agent_id: None,
                session_id: optional_str(input, "session_id"),
            }),
            "user-prompt-submit" => Some(AgentEvent::UserPromptSubmit {
                agent: PI_AGENT.into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: String::new(),
                prompt: json_str(input, "prompt").into(),
                worktree: None,
                agent_id: None,
                session_id: optional_str(input, "session_id"),
            }),
            "notification" => Some(AgentEvent::Notification {
                agent: PI_AGENT.into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: String::new(),
                wait_reason: json_str(input, "wait_reason").into(),
                meta_only: false,
                worktree: None,
                agent_id: None,
                session_id: optional_str(input, "session_id"),
            }),
            "stop" => Some(AgentEvent::Stop {
                agent: PI_AGENT.into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: String::new(),
                last_message: json_str(input, "last_message").into(),
                response: None,
                worktree: None,
                agent_id: None,
                session_id: optional_str(input, "session_id"),
            }),
            "stop-failure" => Some(AgentEvent::StopFailure {
                agent: PI_AGENT.into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: String::new(),
                error: json_str(input, "error").into(),
                worktree: None,
                agent_id: None,
                session_id: optional_str(input, "session_id"),
            }),
            "activity-log" => {
                let raw_name = json_str(input, "tool_name");
                if raw_name.is_empty() {
                    return None;
                }
                let tool_name = normalize_tool_name(raw_name);
                Some(AgentEvent::ActivityLog {
                    tool_name,
                    tool_input: json_value_or_null(input, "tool_input"),
                    tool_response: json_value_or_null(input, "tool_response"),
                })
            }
            _ => None,
        }
    }
}
