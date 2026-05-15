//! Tmux `list-panes -a` reader.
//!
//! Builds the same `Vec<SessionInfo>` shape that the sidebar uses, but
//! drops the Codex process-snapshot fallback (the dashboard is a pure
//! consumer — the sidebar's hooks are the source of truth for which
//! panes have an agent).

use super::commands::run_tmux;
use super::options::{
    PANE_AGENT, PANE_ATTENTION, PANE_BG_CMD, PANE_CWD, PANE_LAST_SEEN_AT, PANE_MARKED_UNREAD_AT,
    PANE_NAME, PANE_PERMISSION_MODE, PANE_PROMPT, PANE_PROMPT_SOURCE, PANE_ROLE, PANE_SESSION_ID,
    PANE_STARTED_AT, PANE_STATUS, PANE_SUBAGENTS, PANE_WAIT_REASON, PANE_WORKTREE_BRANCH,
    PANE_WORKTREE_NAME,
};
use super::types::{
    AgentType, PaneInfo, PaneStatus, PermissionMode, SessionInfo, WindowInfo, WorktreeMetadata,
};

mod session_line_field {
    pub const SESSION_NAME: usize = 0;
    pub const WINDOW_ID: usize = 1;
    pub const WINDOW_NAME: usize = 3;
    pub const WINDOW_ACTIVE: usize = 4;
    pub const PANE_LINE_OFFSET: usize = 6;
    pub const MIN_FIELDS: usize = 27;
}

mod pane_line_field {
    pub const PANE_ACTIVE: usize = 0;
    pub const PANE_STATUS: usize = 1;
    pub const PANE_ATTENTION: usize = 2;
    pub const AGENT: usize = 3;
    pub const PANE_CURRENT_PATH: usize = 5;
    pub const PANE_CURRENT_COMMAND: usize = 6;
    pub const PANE_ROLE: usize = 7;
    pub const PANE_ID: usize = 8;
    pub const PROMPT: usize = 9;
    pub const PROMPT_SOURCE: usize = 10;
    pub const STARTED_AT: usize = 11;
    pub const WAIT_REASON: usize = 12;
    pub const SUBAGENTS: usize = 14;
    pub const PANE_CWD: usize = 15;
    pub const PERMISSION_MODE: usize = 16;
    pub const WORKTREE_NAME: usize = 17;
    pub const WORKTREE_BRANCH: usize = 18;
    pub const SESSION_ID: usize = 19;
    pub const BG_CMD: usize = 20;
    pub const LAST_SEEN_AT: usize = 21;
    pub const MARKED_UNREAD_AT: usize = 22;
    pub const MIN_FIELDS: usize = 23;
}

fn q(field: &str) -> String {
    format!("#{{q:{field}}}")
}

fn pane_format() -> String {
    [
        q("session_name"),
        q("window_id"),
        q("window_index"),
        q("window_name"),
        q("window_active"),
        q("automatic-rename"),
        q("pane_active"),
        q(PANE_STATUS),
        q(PANE_ATTENTION),
        q(PANE_AGENT),
        q(PANE_NAME),
        q("pane_current_path"),
        q("pane_current_command"),
        q(PANE_ROLE),
        q("pane_id"),
        q(PANE_PROMPT),
        q(PANE_PROMPT_SOURCE),
        q(PANE_STARTED_AT),
        q(PANE_WAIT_REASON),
        q("pane_pid"),
        q(PANE_SUBAGENTS),
        q(PANE_CWD),
        q(PANE_PERMISSION_MODE),
        q(PANE_WORKTREE_NAME),
        q(PANE_WORKTREE_BRANCH),
        q(PANE_SESSION_ID),
        q(PANE_BG_CMD),
        q(PANE_LAST_SEEN_AT),
        q(PANE_MARKED_UNREAD_AT),
    ]
    .join("|")
}

type SessionMap = indexmap::IndexMap<String, indexmap::IndexMap<String, WindowInfo>>;

pub fn query_sessions() -> Vec<SessionInfo> {
    let pane_format = pane_format();
    let raw = match run_tmux(&["list-panes", "-a", "-F", &pane_format]) {
        Some(s) => s,
        None => return vec![],
    };
    let mut sessions_map: SessionMap = indexmap::IndexMap::new();

    for line in raw.lines() {
        let parts = split_tmux_fields(line, '|');
        if parts.len() < session_line_field::MIN_FIELDS {
            continue;
        }

        let session_name = parts[session_line_field::SESSION_NAME].as_str();
        let window_id = parts[session_line_field::WINDOW_ID].as_str();
        let pane_fields = &parts[session_line_field::PANE_LINE_OFFSET..];

        let sessions_entry = sessions_map.entry(session_name.to_string()).or_default();

        let window = sessions_entry
            .entry(window_id.to_string())
            .or_insert_with(|| WindowInfo {
                window_id: window_id.to_string(),
                window_name: parts[session_line_field::WINDOW_NAME].to_string(),
                window_active: parts[session_line_field::WINDOW_ACTIVE] == "1",
                panes: Vec::new(),
            });

        if let Some(pane) = parse_pane_fields(pane_fields) {
            window.panes.push(pane);
        }
    }

    finalize_sessions(sessions_map)
}

fn finalize_sessions(sessions_map: SessionMap) -> Vec<SessionInfo> {
    let mut sessions = Vec::new();
    for (session_name, windows) in sessions_map {
        let windows: Vec<WindowInfo> = windows
            .into_values()
            .filter(|w| !w.panes.is_empty())
            .collect();
        if !windows.is_empty() {
            sessions.push(SessionInfo {
                session_name,
                windows,
            });
        }
    }
    sessions
}

fn parse_pane_fields(parts: &[String]) -> Option<PaneInfo> {
    if parts.len() < pane_line_field::MIN_FIELDS {
        return None;
    }

    if parts[pane_line_field::PANE_ROLE] == "sidebar" {
        return None;
    }

    let agent = AgentType::from_label(&parts[pane_line_field::AGENT])?;

    let pane_cwd = &parts[pane_line_field::PANE_CWD];
    let path = if !pane_cwd.is_empty() {
        pane_cwd.to_string()
    } else {
        parts[pane_line_field::PANE_CURRENT_PATH].to_string()
    };

    let permission_mode = if agent == AgentType::Claude {
        PermissionMode::from_label(&parts[pane_line_field::PERMISSION_MODE])
    } else {
        PermissionMode::Default
    };

    let prompt = sanitize_prompt(&parts[pane_line_field::PROMPT]);

    let session_id = if parts[pane_line_field::SESSION_ID].is_empty() {
        None
    } else {
        Some(parts[pane_line_field::SESSION_ID].to_string())
    };

    // Subagents column is also read so the format string indices line up
    // with the sidebar — we just don't expose it on the dashboard.
    let _subagents = &parts[pane_line_field::SUBAGENTS];
    let _prompt_source = &parts[pane_line_field::PROMPT_SOURCE];

    Some(PaneInfo {
        pane_active: parts[pane_line_field::PANE_ACTIVE] == "1",
        status: PaneStatus::from_label(&parts[pane_line_field::PANE_STATUS]),
        attention: !parts[pane_line_field::PANE_ATTENTION].is_empty(),
        last_seen_at: parts[pane_line_field::LAST_SEEN_AT].parse().ok(),
        marked_unread_at: parts[pane_line_field::MARKED_UNREAD_AT].parse().ok(),
        agent,
        path,
        current_command: parts[pane_line_field::PANE_CURRENT_COMMAND].to_string(),
        pane_id: parts[pane_line_field::PANE_ID].to_string(),
        prompt,
        started_at: parts[pane_line_field::STARTED_AT].parse().ok(),
        wait_reason: parts[pane_line_field::WAIT_REASON].to_string(),
        permission_mode,
        worktree: WorktreeMetadata {
            name: parts[pane_line_field::WORKTREE_NAME].to_string(),
            branch: parts[pane_line_field::WORKTREE_BRANCH].to_string(),
        },
        session_id,
        session_name: String::new(),
        bg_cmd: parts[pane_line_field::BG_CMD].to_string(),
    })
}

fn sanitize_prompt(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    if raw.contains("<task-notification>")
        || raw.contains("<system-reminder>")
        || raw.contains("<task-status>")
    {
        return String::new();
    }
    if raw.chars().count() > 200 {
        raw.chars().take(200).collect()
    } else {
        raw.to_string()
    }
}

/// Split a tmux format line while honoring `#{q:...}` backslash escapes.
fn split_tmux_fields(line: &str, delimiter: char) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut escaped = false;

    for ch in line.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == delimiter {
            fields.push(current);
            current = String::new();
            continue;
        }
        current.push(ch);
    }
    if escaped {
        current.push('\\');
    }
    fields.push(current);
    fields
}
