use serde_json::Value;

use super::AgentEvent;
use crate::adapter;
use crate::tmux::{CLAUDE_AGENT, CODEX_AGENT, OPENCODE_AGENT};

pub trait EventAdapter {
    fn parse(&self, event_name: &str, input: &Value) -> Option<AgentEvent>;
}

pub fn resolve_adapter(agent_name: &str) -> Option<Box<dyn EventAdapter>> {
    match agent_name {
        CLAUDE_AGENT => Some(Box::new(adapter::claude::ClaudeAdapter)),
        CODEX_AGENT => Some(Box::new(adapter::codex::CodexAdapter)),
        OPENCODE_AGENT => Some(Box::new(adapter::opencode::OpenCodeAdapter)),
        _ => None,
    }
}
