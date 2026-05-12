//! Tool-name → short label extraction. Keyed off `CanonicalTool` so a typo
//! becomes a compile error. The label is what the dashboard shows in its
//! activity tab next to the tool icon.

use serde_json::Value;

use crate::tool_name::CanonicalTool;

enum LabelStrategy {
    None,
    Field(&'static str),
    FilePath(&'static str),
    UrlStrip(&'static str),
    Custom(fn(&Value, &Value) -> String),
}

const STRATEGY_TABLE: &[(CanonicalTool, LabelStrategy)] = &[
    (CanonicalTool::Read, LabelStrategy::FilePath("file_path")),
    (CanonicalTool::Edit, LabelStrategy::FilePath("file_path")),
    (CanonicalTool::Write, LabelStrategy::FilePath("file_path")),
    (
        CanonicalTool::NotebookEdit,
        LabelStrategy::FilePath("notebook_path"),
    ),
    (CanonicalTool::Bash, LabelStrategy::Field("command")),
    (CanonicalTool::PowerShell, LabelStrategy::Field("command")),
    (CanonicalTool::Monitor, LabelStrategy::Field("command")),
    (
        CanonicalTool::PushNotification,
        LabelStrategy::Field("message"),
    ),
    (CanonicalTool::Glob, LabelStrategy::Field("pattern")),
    (CanonicalTool::Grep, LabelStrategy::Field("pattern")),
    (CanonicalTool::WebFetch, LabelStrategy::UrlStrip("url")),
    (CanonicalTool::WebSearch, LabelStrategy::Field("query")),
    (CanonicalTool::ToolSearch, LabelStrategy::Field("query")),
    (CanonicalTool::Skill, LabelStrategy::Field("skill")),
    (CanonicalTool::SendMessage, LabelStrategy::Field("to")),
    (CanonicalTool::TeamCreate, LabelStrategy::Field("team_name")),
    (CanonicalTool::Lsp, LabelStrategy::Field("operation")),
    (CanonicalTool::CronCreate, LabelStrategy::Field("cron")),
    (CanonicalTool::CronDelete, LabelStrategy::Field("id")),
    (CanonicalTool::EnterWorktree, LabelStrategy::Field("name")),
    (CanonicalTool::ExitWorktree, LabelStrategy::Field("name")),
    (CanonicalTool::Agent, LabelStrategy::Custom(label_agent)),
    (
        CanonicalTool::TaskCreate,
        LabelStrategy::Custom(label_task_create),
    ),
    (
        CanonicalTool::TaskUpdate,
        LabelStrategy::Custom(label_task_update),
    ),
    (CanonicalTool::TaskGet, LabelStrategy::Custom(label_task_id)),
    (
        CanonicalTool::TaskStop,
        LabelStrategy::Custom(label_task_id),
    ),
    (
        CanonicalTool::TaskOutput,
        LabelStrategy::Custom(label_task_id),
    ),
    (
        CanonicalTool::AskUserQuestion,
        LabelStrategy::Custom(label_ask_user_question),
    ),
];

pub(crate) fn extract_tool_label(
    tool_name: &str,
    tool_input: &Value,
    tool_response: &Value,
) -> String {
    let strategy = STRATEGY_TABLE
        .iter()
        .find(|(name, _)| name.as_str() == tool_name)
        .map(|(_, s)| s)
        .unwrap_or(&LabelStrategy::None);

    match strategy {
        LabelStrategy::None => String::new(),
        LabelStrategy::Field(key) => field_str(tool_input, key),
        LabelStrategy::FilePath(key) => basename(&field_str(tool_input, key)),
        LabelStrategy::UrlStrip(key) => {
            let url = field_str(tool_input, key);
            url.trim_start_matches("https://")
                .trim_start_matches("http://")
                .to_string()
        }
        LabelStrategy::Custom(f) => f(tool_input, tool_response),
    }
}

fn field_str(input: &Value, key: &str) -> String {
    input
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn basename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn label_agent(input: &Value, response: &Value) -> String {
    let response_text = response
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|block| block.get("type").and_then(|t| t.as_str()) == Some("text"))
        })
        .and_then(|block| block.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if response_text.is_empty() {
        field_str(input, "description")
    } else {
        response_text
    }
}

fn label_task_create(input: &Value, response: &Value) -> String {
    let task_id = response
        .get("task")
        .and_then(|t| t.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let subject = field_str(input, "subject");
    if !task_id.is_empty() {
        format!("#{task_id} {subject}")
    } else {
        subject
    }
}

fn label_task_update(input: &Value, _: &Value) -> String {
    let status = field_str(input, "status");
    let task_id = field_str(input, "taskId");
    let mut parts = Vec::new();
    if !status.is_empty() {
        parts.push(status);
    }
    if !task_id.is_empty() {
        parts.push(format!("#{task_id}"));
    }
    parts.join(" ")
}

fn label_task_id(input: &Value, _: &Value) -> String {
    let id = field_str(input, "taskId");
    let id = if id.is_empty() {
        field_str(input, "task_id")
    } else {
        id
    };
    if id.is_empty() {
        String::new()
    } else {
        format!("#{id}")
    }
}

fn label_ask_user_question(input: &Value, _: &Value) -> String {
    input
        .get("questions")
        .and_then(|q| q.as_array())
        .and_then(|arr| arr.first())
        .and_then(|q| q.get("question"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}
