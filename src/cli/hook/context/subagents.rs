/// Append `agent_type:agent_id` to a comma-separated subagent list. The
/// id suffix lets `remove_subagent` match the exact instance when its
/// SubagentStop arrives.
pub(in crate::cli::hook) fn append_subagent(
    current: &str,
    agent_type: &str,
    agent_id: &str,
) -> String {
    let entry = format!("{}:{}", agent_type, agent_id);
    if current.is_empty() {
        entry
    } else {
        format!("{},{}", current, entry)
    }
}

/// Remove the entry whose id matches `agent_id`. Returns `None` if not
/// present, `Some(new_list)` otherwise (empty string when drained).
pub(in crate::cli::hook) fn remove_subagent(current: &str, agent_id: &str) -> Option<String> {
    if current.is_empty() || agent_id.is_empty() {
        return None;
    }
    let needle = format!(":{}", agent_id);
    let items: Vec<&str> = current.split(',').collect();
    let idx = items.iter().position(|entry| entry.ends_with(&needle))?;
    let filtered: Vec<&str> = items
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != idx)
        .map(|(_, s)| *s)
        .collect();
    Some(filtered.join(","))
}
