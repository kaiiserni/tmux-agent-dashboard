pub mod claude;
pub mod codex;
pub mod opencode;

pub(crate) fn json_str<'a>(val: &'a serde_json::Value, key: &str) -> &'a str {
    val.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

pub(crate) fn optional_str(val: &serde_json::Value, key: &str) -> Option<String> {
    let s = json_str(val, key);
    if s.is_empty() { None } else { Some(s.into()) }
}

pub(crate) fn json_value_or_null(val: &serde_json::Value, key: &str) -> serde_json::Value {
    val.get(key).cloned().unwrap_or(serde_json::Value::Null)
}

use crate::event::AgentEventKind;

/// Binding between an upstream agent-side hook trigger and the internal
/// `AgentEventKind` produced when it fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookRegistration {
    pub trigger: &'static str,
    pub matcher: Option<&'static str>,
    pub kind: AgentEventKind,
}
