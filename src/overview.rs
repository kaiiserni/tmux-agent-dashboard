//! Reader for the `agent-overview` job's structured output
//! (`~/.local/state/agent-overview/overview.json`). The dashboard's
//! Overview tab is a pure consumer: the file is rewritten every time the
//! external job runs, so all data here can lag by up to its cadence.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::adapter::json_str;

#[derive(Debug, Clone)]
pub struct OverviewPane {
    pub pane_id: String,
    pub target: String,
    pub agent: String,
    pub status: String,
    pub age_minutes: Option<u64>,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct OverviewProject {
    pub name: String,
    pub attention: bool,
    pub doing: String,
    pub needs_from_you: String,
    pub next_steps: Vec<String>,
    pub active_md: Vec<String>,
    pub panes: Vec<OverviewPane>,
}

#[derive(Debug, Clone)]
pub struct OverviewIdle {
    pub pane_id: String,
    pub target: String,
    pub project: String,
    pub task: String,
}

#[derive(Debug, Clone)]
pub struct Overview {
    pub updated_at: SystemTime,
    pub tldr: Vec<String>,
    pub projects: Vec<OverviewProject>,
    pub idle: Vec<OverviewIdle>,
}

/// Path to the overview JSON, from the `@dashboard_overview_file` tmux option.
/// `None` when unset — the dashboard has no built-in producer or default path.
pub fn overview_path() -> Option<PathBuf> {
    let raw = crate::tmux::get_option(crate::tmux::DASHBOARD_OVERVIEW_FILE)?;
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let expanded = if let Some(rest) = raw.strip_prefix("~/") {
        // Without a real HOME (stripped cron/tmux env) a default would make a
        // bogus absolute path that silently never loads — bail instead.
        let home = std::env::var("HOME").ok().filter(|h| !h.is_empty())?;
        PathBuf::from(home).join(rest)
    } else {
        PathBuf::from(raw)
    };
    Some(expanded)
}

fn str_vec(val: &serde_json::Value, key: &str) -> Vec<String> {
    val.get(key)
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// Whether the Overview tab is configured (the option is set), regardless of
/// whether the file currently parses. Lets the UI tell "not configured" apart
/// from "configured but no data yet".
pub fn is_configured() -> bool {
    overview_path().is_some()
}

pub fn load() -> Option<Overview> {
    parse(&std::fs::read_to_string(overview_path()?).ok()?)
}

pub fn parse(content: &str) -> Option<Overview> {
    let val: serde_json::Value = serde_json::from_str(content).ok()?;

    let updated_at = SystemTime::UNIX_EPOCH
        + Duration::from_secs(val.get("updated_at").and_then(|v| v.as_u64()).unwrap_or(0));

    let projects = val
        .get("projects")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|p| OverviewProject {
                    name: json_str(p, "name").to_string(),
                    attention: p.get("attention").and_then(|v| v.as_bool()).unwrap_or(false),
                    doing: json_str(p, "doing").to_string(),
                    needs_from_you: json_str(p, "needs_from_you").to_string(),
                    next_steps: str_vec(p, "next_steps"),
                    active_md: str_vec(p, "active_md"),
                    panes: p
                        .get("panes")
                        .and_then(|v| v.as_array())
                        .map(|panes| {
                            panes
                                .iter()
                                .map(|pane| OverviewPane {
                                    pane_id: json_str(pane, "pane_id").to_string(),
                                    target: json_str(pane, "target").to_string(),
                                    agent: json_str(pane, "agent").to_string(),
                                    status: json_str(pane, "status").to_string(),
                                    age_minutes: pane.get("age_minutes").and_then(|v| v.as_u64()),
                                    summary: json_str(pane, "summary").to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();

    let idle = val
        .get("idle")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|p| OverviewIdle {
                    pane_id: json_str(p, "pane_id").to_string(),
                    target: json_str(p, "target").to_string(),
                    project: json_str(p, "project").to_string(),
                    task: json_str(p, "task").to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    Some(Overview {
        updated_at,
        tldr: str_vec(&val, "tldr"),
        projects,
        idle,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_overview() {
        let json = r#"{
            "updated_at": 1781248553,
            "tldr": ["a", "b"],
            "projects": [{
                "name": "proj", "cwd": "/p", "attention": true,
                "doing": "stuff", "needs_from_you": "review",
                "next_steps": ["s1"], "active_md": ["- [ ] x"],
                "panes": [{"pane_id": "%1", "target": "s:1.0", "agent": "claude",
                           "status": "running", "age_minutes": 5, "summary": "sum"}]
            }],
            "idle": [{"pane_id": "%2", "target": "s:1.1", "project": "other", "task": "t"}]
        }"#;
        let o = parse(json).expect("parses");
        assert_eq!(o.tldr.len(), 2);
        assert_eq!(o.projects.len(), 1);
        let p = &o.projects[0];
        assert!(p.attention);
        assert_eq!(p.panes[0].pane_id, "%1");
        assert_eq!(p.panes[0].age_minutes, Some(5));
        assert_eq!(o.idle[0].project, "other");
    }

    #[test]
    fn null_needs_from_you_becomes_empty() {
        let json = r#"{"updated_at": 1, "tldr": [], "projects": [{
            "name": "p", "cwd": "/p", "attention": false, "doing": "",
            "needs_from_you": null, "next_steps": [], "active_md": [], "panes": []
        }], "idle": []}"#;
        let o = parse(json).expect("parses");
        assert_eq!(o.projects[0].needs_from_you, "");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("not json").is_none());
    }
}
