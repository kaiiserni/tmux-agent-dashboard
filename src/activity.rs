use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ActivityEntry {
    pub timestamp: String,
    pub tool: String,
    pub label: String,
}

impl ActivityEntry {
    pub fn tool_color_index(&self) -> u8 {
        if self.tool.starts_with("mcp__") {
            return 183;
        }
        match self.tool.as_str() {
            "Edit" | "Write" => 180,
            "Bash" | "PowerShell" | "Monitor" => 114,
            "Read" | "Glob" | "Grep" => 110,
            "Agent" => 181,
            "WebFetch" | "WebSearch" => 117,
            "Skill" => 218,
            "TaskCreate" | "TaskUpdate" | "TaskGet" | "TaskList" | "TaskStop" | "TaskOutput" => 223,
            "SendMessage" | "TeamCreate" | "TeamDelete" => 182,
            "LSP" => 146,
            "NotebookEdit" => 180,
            "AskUserQuestion" | "PushNotification" => 216,
            "CronCreate" | "CronDelete" | "CronList" | "RemoteTrigger" => 151,
            "EnterPlanMode" | "ExitPlanMode" => 189,
            "EnterWorktree" | "ExitWorktree" => 179,
            "ToolSearch" => 250,
            _ => 244,
        }
    }
}

pub fn log_file_path(pane_id: &str) -> PathBuf {
    let encoded = pane_id.replace('%', "_");
    PathBuf::from(format!("/tmp/tmux-agent-activity{encoded}.log"))
}

pub fn log_mtime(pane_id: &str) -> Option<std::time::SystemTime> {
    fs::metadata(log_file_path(pane_id))
        .ok()
        .and_then(|m| m.modified().ok())
}

fn parse_entry(line: &str) -> Option<ActivityEntry> {
    let mut parts = line.splitn(3, '|');
    let timestamp = parts.next()?.to_string();
    let tool = parts.next()?.to_string();
    let label = parts.next().unwrap_or("").to_string();
    Some(ActivityEntry {
        timestamp,
        tool,
        label,
    })
}

pub fn read_activity_log(pane_id: &str, max_entries: usize) -> Vec<ActivityEntry> {
    let path = log_file_path(pane_id);
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    if max_entries > 0 {
        content
            .rsplit('\n')
            .filter(|l| !l.is_empty())
            .take(max_entries)
            .filter_map(parse_entry)
            .collect()
    } else {
        content
            .rsplit('\n')
            .filter(|l| !l.is_empty())
            .filter_map(parse_entry)
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct GlobalActivityEntry {
    pub pane_id: String,
    pub entry: ActivityEntry,
    pub mtime: std::time::SystemTime,
}

fn decode_pane_id_from_log(filename: &str) -> Option<String> {
    let stem = filename
        .strip_prefix("tmux-agent-activity")?
        .strip_suffix(".log")?;
    if stem.is_empty() {
        return None;
    }
    Some(stem.replacen('_', "%", 1))
}

/// Merge activity entries from every `/tmp/tmux-agent-activity*.log` file.
/// Returns at most `max_entries` total, newest-first.
pub fn read_all_activity(max_entries: usize) -> Vec<GlobalActivityEntry> {
    let dir = match fs::read_dir("/tmp") {
        Ok(d) => d,
        Err(_) => return vec![],
    };

    let mut per_file: Vec<(std::time::SystemTime, String, Vec<ActivityEntry>)> = Vec::new();
    for entry in dir.flatten() {
        let filename = entry.file_name();
        let name = match filename.to_str() {
            Some(s) => s,
            None => continue,
        };
        if !name.starts_with("tmux-agent-activity") || !name.ends_with(".log") {
            continue;
        }
        let pane_id = match decode_pane_id_from_log(name) {
            Some(p) => p,
            None => continue,
        };
        let mtime = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        let entries = read_activity_log(&pane_id, max_entries);
        if !entries.is_empty() {
            per_file.push((mtime, pane_id, entries));
        }
    }

    per_file.sort_by(|a, b| b.0.cmp(&a.0));

    let mut merged: Vec<GlobalActivityEntry> = Vec::new();
    for (mtime, pane_id, entries) in per_file {
        for entry in entries {
            merged.push(GlobalActivityEntry {
                pane_id: pane_id.clone(),
                entry,
                mtime,
            });
            if merged.len() >= max_entries {
                break;
            }
        }
        if merged.len() >= max_entries {
            break;
        }
    }
    merged
}
