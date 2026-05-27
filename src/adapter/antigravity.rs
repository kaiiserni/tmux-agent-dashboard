//! Antigravity CLI (`agy`) adapter. agy v1.0.x only fires PreToolUse,
//! PostToolUse, and Stop — no SessionStart / UserPromptSubmit / Notification.
//! Payload shape differs from Claude: nested `toolCall.{name,args}`,
//! `workspacePaths[]` instead of `cwd`, `conversationId` instead of
//! `session_id`. No `tool_response`, no `last_assistant_message` on Stop.

use crate::event::{AgentEvent, AgentEventKind, EventAdapter, WorktreeInfo};
use crate::tmux::ANTIGRAVITY_AGENT;
use serde_json::Value;

use super::{HookRegistration, json_str, optional_str};

fn agy_cwd(input: &Value) -> String {
    input
        .get("workspacePaths")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn agy_session_id(input: &Value) -> Option<String> {
    let s = json_str(input, "conversationId");
    if s.is_empty() { None } else { Some(s.into()) }
}

fn parse_worktree(input: &Value) -> Option<WorktreeInfo> {
    let obj = input.get("worktree")?;
    if !obj.is_object() {
        return None;
    }
    let name = json_str(obj, "name");
    let path = json_str(obj, "path");
    let branch = json_str(obj, "branch");
    let original = json_str(obj, "originalRepoDir");
    if name.is_empty() && path.is_empty() && branch.is_empty() && original.is_empty() {
        return None;
    }
    Some(WorktreeInfo {
        name: name.into(),
        path: path.into(),
        branch: branch.into(),
        original_repo_dir: original.into(),
    })
}

fn parse_json_field(input: &Value, field: &str) -> Value {
    input
        .get(field)
        .and_then(|v| {
            if let Some(s) = v.as_str() {
                serde_json::from_str(s).ok()
            } else if v.is_object() {
                Some(v.clone())
            } else {
                None
            }
        })
        .unwrap_or(Value::Null)
}

pub struct AntigravityAdapter;

impl AntigravityAdapter {
    pub const HOOK_REGISTRATIONS: &'static [HookRegistration] = &[
        HookRegistration {
            trigger: "SessionStart",
            matcher: None,
            kind: AgentEventKind::SessionStart,
        },
        HookRegistration {
            trigger: "SessionEnd",
            matcher: None,
            kind: AgentEventKind::SessionEnd,
        },
        HookRegistration {
            trigger: "UserPromptSubmit",
            matcher: None,
            kind: AgentEventKind::UserPromptSubmit,
        },
        HookRegistration {
            trigger: "Notification",
            matcher: None,
            kind: AgentEventKind::Notification,
        },
        HookRegistration {
            trigger: "PreToolUse",
            matcher: Some("ExitPlanMode"),
            kind: AgentEventKind::PlanReview,
        },
        HookRegistration {
            trigger: "PermissionRequest",
            matcher: None,
            kind: AgentEventKind::PermissionRequest,
        },
        HookRegistration {
            trigger: "Stop",
            matcher: None,
            kind: AgentEventKind::Stop,
        },
        HookRegistration {
            trigger: "StopFailure",
            matcher: None,
            kind: AgentEventKind::StopFailure,
        },
        HookRegistration {
            trigger: "PermissionDenied",
            matcher: None,
            kind: AgentEventKind::PermissionDenied,
        },
        HookRegistration {
            trigger: "CwdChanged",
            matcher: None,
            kind: AgentEventKind::CwdChanged,
        },
        HookRegistration {
            trigger: "SubagentStart",
            matcher: None,
            kind: AgentEventKind::SubagentStart,
        },
        HookRegistration {
            trigger: "SubagentStop",
            matcher: None,
            kind: AgentEventKind::SubagentStop,
        },
        HookRegistration {
            trigger: "PostToolUse",
            matcher: None,
            kind: AgentEventKind::ActivityLog,
        },
    ];
}

impl EventAdapter for AntigravityAdapter {
    fn parse(&self, event_name: &str, input: &Value) -> Option<AgentEvent> {
        match event_name {
            "session-start" => Some(AgentEvent::SessionStart {
                agent: ANTIGRAVITY_AGENT.into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: json_str(input, "permission_mode").into(),
                source: json_str(input, "source").into(),
                worktree: parse_worktree(input),
                agent_id: optional_str(input, "agent_id"),
                session_id: optional_str(input, "session_id"),
            }),
            "session-end" => Some(AgentEvent::SessionEnd {
                end_reason: json_str(input, "end_reason").into(),
            }),
            "user-prompt-submit" => Some(AgentEvent::UserPromptSubmit {
                agent: ANTIGRAVITY_AGENT.into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: json_str(input, "permission_mode").into(),
                prompt: json_str(input, "prompt").into(),
                worktree: parse_worktree(input),
                agent_id: optional_str(input, "agent_id"),
                session_id: optional_str(input, "session_id"),
            }),
            "notification" => {
                let wait_reason = json_str(input, "notification_type");
                let meta_only = wait_reason == "idle_prompt";
                Some(AgentEvent::Notification {
                    agent: ANTIGRAVITY_AGENT.into(),
                    cwd: json_str(input, "cwd").into(),
                    permission_mode: json_str(input, "permission_mode").into(),
                    wait_reason: wait_reason.into(),
                    meta_only,
                    worktree: parse_worktree(input),
                    agent_id: optional_str(input, "agent_id"),
                    session_id: optional_str(input, "session_id"),
                })
            }
            "plan-review" => Some(AgentEvent::PlanReview {
                agent: ANTIGRAVITY_AGENT.into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: json_str(input, "permission_mode").into(),
                worktree: parse_worktree(input),
                agent_id: optional_str(input, "agent_id"),
                session_id: optional_str(input, "session_id"),
            }),
            "permission-request" => Some(AgentEvent::PermissionRequest {
                agent: ANTIGRAVITY_AGENT.into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: json_str(input, "permission_mode").into(),
                worktree: parse_worktree(input),
                agent_id: optional_str(input, "agent_id"),
                session_id: optional_str(input, "session_id"),
            }),
            "stop" => Some(AgentEvent::Stop {
                agent: ANTIGRAVITY_AGENT.into(),
                cwd: agy_cwd(input),
                permission_mode: String::new(),
                last_message: String::new(),
                response: None,
                worktree: None,
                agent_id: None,
                session_id: agy_session_id(input),
            }),
            "stop-failure" => {
                let error_type = json_str(input, "error_type");
                let error_legacy = json_str(input, "error");
                let error_message = json_str(input, "error_message");
                let error_details = json_str(input, "error_details");
                let error = if !error_type.is_empty() {
                    error_type
                } else if !error_legacy.is_empty() {
                    error_legacy
                } else if !error_message.is_empty() {
                    error_message
                } else {
                    error_details
                };
                Some(AgentEvent::StopFailure {
                    agent: ANTIGRAVITY_AGENT.into(),
                    cwd: json_str(input, "cwd").into(),
                    permission_mode: json_str(input, "permission_mode").into(),
                    error: error.into(),
                    worktree: parse_worktree(input),
                    agent_id: optional_str(input, "agent_id"),
                    session_id: optional_str(input, "session_id"),
                })
            }
            "permission-denied" => Some(AgentEvent::PermissionDenied {
                agent: ANTIGRAVITY_AGENT.into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: json_str(input, "permission_mode").into(),
                worktree: parse_worktree(input),
                agent_id: optional_str(input, "agent_id"),
                session_id: optional_str(input, "session_id"),
            }),
            "cwd-changed" => Some(AgentEvent::CwdChanged {
                cwd: json_str(input, "cwd").into(),
                worktree: parse_worktree(input),
                agent_id: optional_str(input, "agent_id"),
                session_id: optional_str(input, "session_id"),
            }),
            "subagent-start" => {
                let agent_type = json_str(input, "agent_type");
                if agent_type.is_empty() {
                    return None;
                }
                Some(AgentEvent::SubagentStart {
                    agent_type: agent_type.into(),
                    agent_id: optional_str(input, "agent_id"),
                })
            }
            "subagent-stop" => {
                let agent_type = json_str(input, "agent_type");
                if agent_type.is_empty() {
                    return None;
                }
                Some(AgentEvent::SubagentStop {
                    agent_type: agent_type.into(),
                    agent_id: optional_str(input, "agent_id"),
                    last_message: json_str(input, "last_assistant_message").into(),
                    transcript_path: json_str(input, "agent_transcript_path").into(),
                })
            }
            "activity-log" => {
                // agy uses nested toolCall.{name,args}; skip null-tool steps.
                let tool_call = input.get("toolCall")?;
                if tool_call.is_null() {
                    return None;
                }
                let tool_name = json_str(tool_call, "name");
                if tool_name.is_empty() {
                    return None;
                }
                let tool_input = tool_call.get("args").cloned().unwrap_or(Value::Null);
                Some(AgentEvent::ActivityLog {
                    tool_name: tool_name.into(),
                    tool_input,
                    tool_response: Value::Null,
                })
            }
            _ => None,
        }
    }
}
