//! Grok Build (xAI's `grok` CLI, config in `~/.grok/`) adapter. Grok ships a
//! native, Claude-Code-compatible hook system (docs.x.ai/build/features/hooks):
//! events like `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`,
//! `Stop`, `Notification` run a `command` hook with the event JSON on stdin —
//! exactly the push model `cli/hook.rs` expects, so no shim is needed.
//!
//! `install-hooks grok` maps Grok's native PascalCase triggers to the wire
//! kebab-case event names; this adapter parses those. Grok's payload keys are
//! camelCase (`sessionId`, `toolName`, `toolInput`, `workspaceRoot`) where
//! Claude uses snake_case — the only real difference. A few fields
//! (prompt/last message/tool result) aren't documented, so we read the likely
//! camelCase key and fall back to the snake_case/Claude name.

use serde_json::Value;

use crate::event::{AgentEvent, AgentEventKind, EventAdapter};
use crate::tmux::GROK_AGENT;
use crate::tool_name::CanonicalTool;

use super::HookRegistration;

/// First non-empty string across `keys`, else "".
fn first_str<'a>(val: &'a Value, keys: &[&str]) -> &'a str {
    keys.iter()
        .filter_map(|key| val.get(*key).and_then(|v| v.as_str()))
        .find(|s| !s.is_empty())
        .unwrap_or("")
}

/// First non-empty string across `keys` as an owned Option.
fn first_optional_str(val: &Value, keys: &[&str]) -> Option<String> {
    let s = first_str(val, keys);
    if s.is_empty() { None } else { Some(s.into()) }
}

/// First non-null JSON value across `keys`, else Null.
fn first_value(val: &Value, keys: &[&str]) -> Value {
    keys.iter()
        .filter_map(|key| val.get(*key))
        .find(|v| !v.is_null())
        .cloned()
        .unwrap_or(Value::Null)
}

fn cwd_of(input: &Value) -> String {
    first_str(input, &["cwd", "workspaceRoot", "workspace_root"]).into()
}

fn session_of(input: &Value) -> Option<String> {
    first_optional_str(input, &["sessionId", "session_id"])
}

fn agent_id_of(input: &Value) -> Option<String> {
    first_optional_str(input, &["agentId", "agent_id", "id"])
}

/// Map Grok's tool ids to the dashboard's canonical PascalCase names so
/// activity-log colors and label extraction work. Grok's real tool names are
/// its own snake_case ids (`run_terminal_command`, `read_file`, `search_replace`,
/// `list_dir`, …), per ~/.grok/docs/user-guide/10-hooks.md. MCP calls surface as
/// qualified `server__tool` names — left unmapped, shown verbatim.
fn normalize_tool_name(raw: &str) -> String {
    let canonical = match raw.to_ascii_lowercase().as_str() {
        // Grok-native ids
        "run_terminal_command" | "bash" | "shell" | "run" => CanonicalTool::Bash,
        "read_file" | "read" | "view" => CanonicalTool::Read,
        "search_replace" | "edit" | "multiedit" | "str_replace" => CanonicalTool::Edit,
        "write_file" | "create_file" | "write" => CanonicalTool::Write,
        "list_dir" | "glob" | "listdir" => CanonicalTool::Glob,
        "grep" | "search" => CanonicalTool::Grep,
        "web_search" | "websearch" => CanonicalTool::WebSearch,
        "web_fetch" | "webfetch" | "fetch" => CanonicalTool::WebFetch,
        "spawn_subagent" | "task" | "agent" | "subagent" => CanonicalTool::Agent,
        "skill" => CanonicalTool::Skill,
        "lsp" => CanonicalTool::Lsp,
        "todowrite" | "todo" => CanonicalTool::TodoWrite,
        _ => return raw.to_string(),
    };
    canonical.as_str().to_string()
}

pub struct GrokAdapter;

impl GrokAdapter {
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
            trigger: "PostToolUse",
            matcher: None,
            kind: AgentEventKind::ActivityLog,
        },
        HookRegistration {
            trigger: "Notification",
            matcher: None,
            kind: AgentEventKind::Notification,
        },
        HookRegistration {
            trigger: "PermissionDenied",
            matcher: None,
            kind: AgentEventKind::PermissionDenied,
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
            trigger: "SubagentStart",
            matcher: None,
            kind: AgentEventKind::SubagentStart,
        },
        HookRegistration {
            trigger: "SubagentStop",
            matcher: None,
            kind: AgentEventKind::SubagentStop,
        },
    ];
}

impl EventAdapter for GrokAdapter {
    fn parse(&self, event_name: &str, input: &Value) -> Option<AgentEvent> {
        match event_name {
            "session-start" => Some(AgentEvent::SessionStart {
                agent: GROK_AGENT.into(),
                cwd: cwd_of(input),
                permission_mode: String::new(),
                source: first_str(input, &["source", "reason"]).into(),
                worktree: None,
                agent_id: agent_id_of(input),
                session_id: session_of(input),
            }),
            "session-end" => Some(AgentEvent::SessionEnd {
                end_reason: first_str(input, &["endReason", "end_reason", "reason"]).into(),
            }),
            "user-prompt-submit" => Some(AgentEvent::UserPromptSubmit {
                agent: GROK_AGENT.into(),
                cwd: cwd_of(input),
                permission_mode: String::new(),
                prompt: first_str(input, &["prompt", "userPrompt", "message"]).into(),
                worktree: None,
                agent_id: agent_id_of(input),
                session_id: session_of(input),
            }),
            "notification" => {
                let wait_reason =
                    first_str(input, &["notificationType", "notification_type", "type", "message"]);
                Some(AgentEvent::Notification {
                    agent: GROK_AGENT.into(),
                    cwd: cwd_of(input),
                    permission_mode: String::new(),
                    wait_reason: wait_reason.into(),
                    meta_only: false,
                    worktree: None,
                    agent_id: agent_id_of(input),
                    session_id: session_of(input),
                })
            }
            "permission-denied" => Some(AgentEvent::PermissionDenied {
                agent: GROK_AGENT.into(),
                cwd: cwd_of(input),
                permission_mode: String::new(),
                worktree: None,
                agent_id: agent_id_of(input),
                session_id: session_of(input),
            }),
            "stop" => Some(AgentEvent::Stop {
                agent: GROK_AGENT.into(),
                cwd: cwd_of(input),
                permission_mode: String::new(),
                last_message: first_str(
                    input,
                    &["lastMessage", "last_message", "last_assistant_message", "message"],
                )
                .into(),
                response: None,
                worktree: None,
                agent_id: agent_id_of(input),
                session_id: session_of(input),
            }),
            "stop-failure" => Some(AgentEvent::StopFailure {
                agent: GROK_AGENT.into(),
                cwd: cwd_of(input),
                permission_mode: String::new(),
                error: first_str(
                    input,
                    &["error", "errorMessage", "error_message", "errorType", "message"],
                )
                .into(),
                worktree: None,
                agent_id: agent_id_of(input),
                session_id: session_of(input),
            }),
            "subagent-start" => {
                let agent_type = first_str(input, &["agentType", "agent_type", "name"]);
                if agent_type.is_empty() {
                    return None;
                }
                Some(AgentEvent::SubagentStart {
                    agent_type: agent_type.into(),
                    agent_id: agent_id_of(input),
                })
            }
            "subagent-stop" => {
                let agent_type = first_str(input, &["agentType", "agent_type", "name"]);
                if agent_type.is_empty() {
                    return None;
                }
                Some(AgentEvent::SubagentStop {
                    agent_type: agent_type.into(),
                    agent_id: agent_id_of(input),
                    last_message: first_str(input, &["lastMessage", "last_message", "message"])
                        .into(),
                    transcript_path: first_str(
                        input,
                        &["agentTranscriptPath", "agent_transcript_path", "transcriptPath"],
                    )
                    .into(),
                })
            }
            "activity-log" => {
                let raw_name = first_str(input, &["toolName", "tool_name"]);
                if raw_name.is_empty() {
                    return None;
                }
                let tool_name = normalize_tool_name(raw_name);
                Some(AgentEvent::ActivityLog {
                    tool_name,
                    tool_input: first_value(input, &["toolInput", "tool_input"]),
                    tool_response: first_value(
                        input,
                        &["toolResult", "toolResponse", "toolOutput", "tool_response"],
                    ),
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn session_start_reads_camelcase() {
        let ev = GrokAdapter
            .parse(
                "session-start",
                &json!({ "sessionId": "s-1", "workspaceRoot": "/repo", "source": "startup" }),
            )
            .expect("event");
        match ev {
            AgentEvent::SessionStart { cwd, source, session_id, agent, .. } => {
                assert_eq!(agent, GROK_AGENT);
                assert_eq!(cwd, "/repo"); // falls back from cwd → workspaceRoot
                assert_eq!(source, "startup");
                assert_eq!(session_id.as_deref(), Some("s-1"));
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn activity_log_normalizes_grok_native_tool_name() {
        // Grok's real tool id for a shell command is `run_terminal_command`.
        let ev = GrokAdapter
            .parse(
                "activity-log",
                &json!({ "toolName": "run_terminal_command", "toolInput": { "command": "ls" } }),
            )
            .expect("event");
        match ev {
            AgentEvent::ActivityLog { tool_name, tool_input, .. } => {
                assert_eq!(tool_name, "Bash");
                assert_eq!(tool_input["command"], json!("ls"));
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn activity_log_maps_grok_file_and_edit_tools() {
        for (raw, want) in [("read_file", "Read"), ("search_replace", "Edit"), ("list_dir", "Glob")]
        {
            let ev = GrokAdapter
                .parse("activity-log", &json!({ "toolName": raw }))
                .expect("event");
            match ev {
                AgentEvent::ActivityLog { tool_name, .. } => assert_eq!(tool_name, want),
                other => panic!("wrong variant for {raw}: {other:?}"),
            }
        }
    }

    #[test]
    fn activity_log_without_tool_is_dropped() {
        assert!(GrokAdapter.parse("activity-log", &json!({})).is_none());
    }

    #[test]
    fn user_prompt_submit_reads_prompt() {
        let ev = GrokAdapter
            .parse("user-prompt-submit", &json!({ "prompt": "hi", "cwd": "/x" }))
            .expect("event");
        match ev {
            AgentEvent::UserPromptSubmit { prompt, cwd, .. } => {
                assert_eq!(prompt, "hi");
                assert_eq!(cwd, "/x");
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }
}
